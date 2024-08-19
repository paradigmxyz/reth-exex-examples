use alloy_sol_types::{sol, SolEventInterface};
use futures::Future;
use reth_execution_types::Chain;
use reth_exex::{ExExContext, ExExEvent};
use reth_node_api::FullNodeComponents;
use reth_node_ethereum::EthereumNode;
use reth_primitives::{address, Address, Log, SealedBlockWithSenders, TransactionSigned};
use reth_tracing::tracing::info;
use reth_node_optimism::{
    args::RollupArgs, node::OptimismAddOns, rpc::SequencerClient, OptimismNode,
};
use reth::cli::Cli;
use tokio_postgres::Error;
use std::{env, str::FromStr};
use openssl::ssl::{SslConnector, SslMethod, SslVerifyMode};
use postgres_openssl::MakeTlsConnector;
use deadpool_postgres::{Manager, ManagerConfig, Pool};
use clap::Parser;

sol!(DaimoNameRegistry, "abi/daimo_name_registry.json");
sol!(ERC20, "abi/erc20.json");
use crate::DaimoNameRegistry::{Registered as RegisteredEvent, DaimoNameRegistryEvents};
use crate::ERC20::{Transfer as TransferEvent, ERC20Events};

enum ChainEvent {
    DaimoNameRegistry(DaimoNameRegistryEvents),
    ERC20(ERC20Events),
}

const NAME_REGISTRY_ADDRESS: Address = address!("66d9d1dd3b5667dae49f135cc8f6b362f9908d89");

fn main() -> eyre::Result<()> {
     Cli::<RollupArgs>::parse().run(|builder, _| async move {
        let db_url = env::var("DAIMO_EXEX_DB_URL").unwrap_or_default();

        let handle = builder
            .node(EthereumNode::default())
            .install_exex("DaimoNames", |ctx| async move {
                let pool = api_ro_pg(&db_url);
                return init(ctx, pool).await;
            })
            .launch()
            .await?;

        handle.wait_for_node_exit().await
    })
}

/// Connect to PG
fn api_ro_pg(cstr: &str) -> Pool {
    let pg_config = tokio_postgres::Config::from_str(cstr).expect("unable to connect to ro pg");
    let mut builder = SslConnector::builder(SslMethod::tls()).expect("tls builder");
    builder.set_verify(SslVerifyMode::NONE);
    let connector = MakeTlsConnector::new(builder.build());
    let pg_mgr = Manager::from_config(
        pg_config,
        connector,
        ManagerConfig {
            recycling_method: deadpool_postgres::RecyclingMethod::Fast,
        },
    );
    Pool::builder(pg_mgr)
        .max_size(16)
        .build()
        .expect("unable to build new ro pool")
}



/// Initializes the ExEx.
///
/// Opens up a SQLite database and creates the tables (if they don't exist).
async fn init<Node: FullNodeComponents>(
    ctx: ExExContext<Node>,
    mut pool: Pool,
) -> eyre::Result<impl Future<Output = eyre::Result<()>>> {
    create_tables(&mut pool).await?;
    Ok(daimo_names_exex(ctx, pool))
}

/// Create SQLite tables if they do not exist.
async fn create_tables(pool: &mut Pool) -> Result<(), Error> {
    let client = pool.get().await.unwrap();

    // Create address <-> name mapping table
    client.execute(
        r#"
            CREATE TABLE IF NOT EXISTS index.names (
                chain_id         INTEGER,
                block_number     INTEGER NOT NULL,
                block_hash       TEXT NOT NULL,
                tx_index         INTEGER NOT NULL,
                tx_hash          TEXT NOT NULL,
                log_address      TEXT NOT NULL,
                log_index        INTEGER NOT NULL,
                name             TEXT NOT NULL,
                addr             TEXT NOT NULL
            );
            "#,
        &[],
    ).await?;

    // todo index tx_hash for efficient revert handling

    info!("Initialized database tables");

    Ok(())
}

/// An example of ExEx that listens to Daimo Name Registry events and stores
/// address <-> name mappings in a SQLite database.
async fn daimo_names_exex<Node: FullNodeComponents>(
    mut ctx: ExExContext<Node>,
    pool: Pool,
) -> eyre::Result<()> {
    // Process all new chain state notifications
    while let Some(notification) = ctx.notifications.recv().await {
        let client = pool.get().await?;

        // Revert all rows from reverted transactions
        if let Some(reverted_chain) = notification.reverted_chain() {
            let events = decode_chain_into_events(&reverted_chain);

            let mut deleted = 0;

            for (_, _, tx, _, _, event) in events {
                match event {
                    ChainEvent::ERC20(ERC20Events::Transfer(TransferEvent { .. })) => {
                        deleted += client.execute(
                            "DELETE FROM index.daimo_transfers WHERE tx_hash = ?;",
                            &[&tx.hash().to_string()],
                        ).await?;
                    },
                    ChainEvent::DaimoNameRegistry(DaimoNameRegistryEvents::Registered(RegisteredEvent { .. })) => {
                        deleted += client.execute(
                            "DELETE FROM index.names WHERE tx_hash = ?;",
                            &[&tx.hash().to_string()],
                        ).await?;
                    }
                    _ => continue,
                }
            }

            info!(block_range = ?reverted_chain.range(), %deleted, "Reverted chain events");
        }

        // Insert new rows from committed transactions
        if let Some(committed_chain) = notification.committed_chain() {
            let events = decode_chain_into_events(&committed_chain);

            let mut n_names = 0;
            let mut  n_transfers = 0;

            for (block, tx_idx, tx, log_idx, log, event) in events {
                match event {
                    // ERC20 Transfer
                    ChainEvent::ERC20(ERC20Events::Transfer(TransferEvent { from, to, value })) => {
                        let inserted = client.execute(
                            r#"
                            INSERT INTO index.daimo_transfers (chain_id, block_number, block_hash, tx_index, tx_hash, log_address, log_index, from, to, amount)
                            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                            "#,
                            &[
                                &(tx.chain_id().unwrap_or_default() as i32),
                                &(block.number as i32),
                                &block.hash().to_string(),
                                &(tx_idx as i32),
                                &tx.hash().to_string(),
                                &log.address.to_string(),
                                &(log_idx as i32),
                                &from.to_string(),
                                &to.to_string(),
                                &value.to_string(),
                            ],
                        ).await?;
                        n_transfers += inserted;
                    },
                    // Registered
                    ChainEvent::DaimoNameRegistry(DaimoNameRegistryEvents::Registered(RegisteredEvent { name, addr })) => {
                        let inserted = client.execute(
                            r#"
                            INSERT INTO index.names (chain_id, block_number, block_hash, tx_index, tx_hash, log_address, log_index, name, addr)
                            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                            "#,
                            &[
                                &(tx.chain_id().unwrap_or_default() as i32),
                                &(block.number as i32),
                                &block.hash().to_string(),
                                &(tx_idx as i32),
                                &tx.hash().to_string(),
                                &log.address.to_string(),
                                &(log_idx as i32),
                                &name.to_string(),
                                &addr.to_string(),
                            ],
                        ).await?;
                        n_names += inserted;
                    },

                    _ => {},
                };



                info!(block_range = ?committed_chain.range(), %n_names, %n_transfers, "Committed chain events");

                // Send a finished height event, signaling the node that we don't need any blocks below
                // this height anymore
                ctx.events.send(ExExEvent::FinishedHeight(committed_chain.tip().number))?;
            }
        }
    }

    Ok(())
}

/// Decode chain of blocks into a flattened list of logs with corresponding txs.
/// TODO: is there an easier way to get tx_idx and log_idx?
fn decode_chain_into_events(
    chain: &Chain,
) -> impl Iterator<Item = (&SealedBlockWithSenders, usize, &TransactionSigned, usize, &Log, ChainEvent)>
{
    let mut num_logs = 0;

    chain
        // Get all blocks and receipts
        .blocks_and_receipts()
        // Get all receipts
        .flat_map(|(block, receipts)| {
            block
                .body
                .iter()
                .zip(receipts.iter().flatten())
                .enumerate()
                .map(move |(tx_idx, (tx, receipt))| (block, tx_idx, tx, receipt))
        })
        // Get all (log_idx, log) from expected contracts
        .flat_map(move |(block, tx_idx, tx, receipt)| {
            receipt
                .logs
                .iter()
                .map(move |log| {
                    num_logs += 1;
                    (num_logs - 1, log)
                })
                .filter(|(_, log)| log.address == NAME_REGISTRY_ADDRESS)
                .map(move |(log_idx, log)| (block, tx_idx, tx, log_idx, log))
        })
        // Decode and filter registry events
        .filter_map(|(block, tx_idx, tx, log_idx, log)| {
            // Map event against each of the possible event types, using topic0 to discriminate
            DaimoNameRegistryEvents::decode_raw_log(log.topics(), &log.data.data, true)
            .map(ChainEvent::DaimoNameRegistry)
            .or_else(|_| ERC20Events::decode_raw_log(log.topics(), &log.data.data, true)
                .map(ChainEvent::ERC20))
            .ok()
            .map(|event| (block, tx_idx, tx, log_idx, log, event))
        })
}

// #[cfg(test)]
// mod tests {
//     use std::pin::pin;

//     use alloy_sol_types::SolEvent;
//     use reth::revm::db::BundleState;
//     use reth_execution_types::{Chain, ExecutionOutcome};
//     use reth_exex_test_utils::{test_exex_context, PollOnce};
//     use reth_primitives::{
//         Address, Block, Header, Log, Receipt, Transaction, TransactionSigned, TxKind, TxLegacy,
//         TxType, U256,
//     };
//     use reth_testing_utils::generators::sign_tx_with_random_key_pair;
//     use tokio_postgres::Connection;

//     use crate::{DaimoNameRegistry, NAME_REGISTRY_ADDRESS};

//     /// Given the address of name registry contract and an event, construct a transaction signed with a
//     /// random private key and a receipt for that transaction.
//     fn construct_tx_and_receipt<E: SolEvent>(
//         to: Address,
//         event: E,
//     ) -> eyre::Result<(TransactionSigned, Receipt)> {
//         let tx = Transaction::Legacy(TxLegacy { to: TxKind::Call(to), ..Default::default() });
//         let log = Log::new(
//             to,
//             event.encode_topics().into_iter().map(|topic| topic.0).collect(),
//             event.encode_data().into(),
//         )
//         .ok_or_else(|| eyre::eyre!("failed to encode event"))?;
//         #[allow(clippy::needless_update)] // side-effect of optimism fields
//         let receipt = Receipt {
//             tx_type: TxType::Legacy,
//             success: true,
//             cumulative_gas_used: 0,
//             logs: vec![log],
//             ..Default::default()
//         };
//         Ok((sign_tx_with_random_key_pair(&mut rand::thread_rng(), tx), receipt))
//     }

//     #[tokio::test]
//     async fn test_exex() -> eyre::Result<()> {
//         // Initialize the test Execution Extension context with all dependencies
//         let (ctx, handle) = test_exex_context().await?;
//         // Create a temporary database file, so we can access it later for assertions
//         let db_file = tempfile::NamedTempFile::new()?;

//         // Initialize the ExEx
//         let mut exex = pin!(super::init(ctx, Connection::open(&db_file)?).await?);

//         // Generate random "from" address for name registry event
//         let from_address = Address::random();

//         // Construct name registry event, transaction and receipt
//         let registration_event = DaimoNameRegistry::Registered {
//             name: U256::from(1337).into(),
//             addr: from_address,
//         };
//         let (registration_tx, registration_tx_receipt) =
//             construct_tx_and_receipt(NAME_REGISTRY_ADDRESS, registration_event.clone())?;
        
//         let chain_id = registration_tx.chain_id();

//         // Construct a block
//         let block = Block {
//             header: Header::default(),
//             body: vec![registration_tx.clone()],
//             ..Default::default()
//         }
//         .seal_slow()
//         .seal_with_senders()
//         .ok_or_else(|| eyre::eyre!("failed to recover senders"))?;

//         // Construct a chain
//         let chain = Chain::new(
//             vec![block.clone()],
//             ExecutionOutcome::new(
//                 BundleState::default(),
//                 vec![registration_tx_receipt].into(),
//                 block.number,
//                 vec![block.requests.clone().unwrap_or_default()],
//             ),
//             None,
//         );

//         // Send a notification that the chain has been committed
//         handle.send_notification_chain_committed(chain.clone()).await?;
//         // Poll the ExEx once, it will process the notification that we just sent
//         exex.poll_once().await?;

//         let connection = Connection::open(&db_file)?;

//         // Assert that the name was parsed correctly and inserted into the database
//         let names = connection
//             .prepare(r#"SELECT chain_id, block_number, block_hash, tx_index, tx_hash, log_address, log_index, name, addr FROM names"#)?
//             .query_map([], |row| {
//                 Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?, row.get(7)?, row.get(8)?))
//             })?
//             .collect::<Result<Vec<_>, _>>()?;
//         assert_eq!(names.len(), 1);
//         assert_eq!(
//             names[0],
//             (
//                 chain_id,
//                 block.number,
//                 block.hash().to_string(),
//                 0,
//                 registration_tx.hash().to_string(),
//                 NAME_REGISTRY_ADDRESS.to_string(),
//                 0,
//                 registration_event.name.to_string(),
//                 registration_event.addr.to_string(),
//             )
//         );

//         // Send a notification that the same chain has been reverted
//         handle.send_notification_chain_reverted(chain).await?;
//         // Poll the ExEx once, it will process the notification that we just sent
//         exex.poll_once().await?;

//         // Assert that the name was removed from the database
//         let names = connection
//             .prepare(r#"SELECT chain_id, block_number, block_hash, tx_index, tx_hash, log_address, log_index, name, addr FROM names"#)?
//             .query_map([], |_| {
//                 Ok(())
//             })?
//             .count();
//         assert_eq!(names, 0);

//         Ok(())
//     }
// }
