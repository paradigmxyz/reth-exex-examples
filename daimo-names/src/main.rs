use alloy_sol_types::{sol, SolEventInterface};
use futures::Future;
use reth_execution_types::Chain;
use reth_exex::{ExExContext, ExExEvent};
use reth_node_api::FullNodeComponents;
use reth_node_ethereum::EthereumNode;
use reth_primitives::{address, Address, Log, SealedBlockWithSenders, TransactionSigned};
use reth_tracing::tracing::info;
use rusqlite::Connection;

sol!(DaimoNameRegistry, "daimo_name_registry_abi.json");
use crate::DaimoNameRegistry::{Registered as RegisteredEvent, DaimoNameRegistryEvents};

const NAME_REGISTRY_ADDRESS: Address = address!("f0fc94DCDC04b2400E5EEac6Aba35cC87d1954D0");

/// Initializes the ExEx.
///
/// Opens up a SQLite database and creates the tables (if they don't exist).
async fn init<Node: FullNodeComponents>(
    ctx: ExExContext<Node>,
    mut connection: Connection,
) -> eyre::Result<impl Future<Output = eyre::Result<()>>> {
    create_tables(&mut connection)?;

    Ok(daimo_names_exex(ctx, connection))
}

/// Create SQLite tables if they do not exist.
fn create_tables(connection: &mut Connection) -> rusqlite::Result<()> {
    // Create address <-> name mapping table
    connection.execute(
        r#"
            CREATE TABLE IF NOT EXISTS names (
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
        (),
    )?;

    // todo index tx_hash for efficient revert handling

    info!("Initialized database tables");

    Ok(())
}

/// An example of ExEx that listens to Daimo Name Registry events and stores
/// address <-> name mappings in a SQLite database.
async fn daimo_names_exex<Node: FullNodeComponents>(
    mut ctx: ExExContext<Node>,
    connection: Connection,
) -> eyre::Result<()> {
    // Process all new chain state notifications
    while let Some(notification) = ctx.notifications.recv().await {
        // Revert all rows from reverted transactions
        if let Some(reverted_chain) = notification.reverted_chain() {
            let events = decode_chain_into_events(&reverted_chain);

            let mut deleted = 0;

            for (_, _, tx, _, _, event) in events {
                match event {
                    DaimoNameRegistryEvents::Registered(RegisteredEvent { .. }) => {
                        deleted += connection.execute(
                            "DELETE FROM names WHERE tx_hash = ?;",
                            (tx.hash().to_string(),),
                        )?;
                    }
                    _ => continue,
                }
            }

            info!(block_range = ?reverted_chain.range(), %deleted, "Reverted chain events");
        }

        // Insert new rows from committed transactions
        if let Some(committed_chain) = notification.committed_chain() {
            let events = decode_chain_into_events(&committed_chain);

            let mut names = 0;

            for (block, tx_idx, tx, log_idx, log, event) in events {
                match event {
                    // Registered
                    DaimoNameRegistryEvents::Registered(RegisteredEvent { name, addr}) => {
                        let inserted = connection.execute(
                            r#"
                            INSERT INTO names (chain_id, block_number, block_hash, tx_index, tx_hash, log_address, log_index, name, addr)
                            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                            "#,
                            (
                                tx.chain_id(),
                                block.number,
                                block.hash().to_string(),
                                tx_idx,
                                tx.hash().to_string(),
                                log.address.to_string(),
                                log_idx,
                                name.to_string(),
                                addr.to_string(),
                            ),
                        )?;
                        names += inserted;
                    },
                    _ => continue,
                };
            }

            info!(block_range = ?committed_chain.range(), %names, "Committed chain events");

            // Send a finished height event, signaling the node that we don't need any blocks below
            // this height anymore
            ctx.events.send(ExExEvent::FinishedHeight(committed_chain.tip().number))?;
        }
    }

    Ok(())
}

/// Decode chain of blocks into a flattened list of logs with corresponding txs.
/// TODO: is there an easier way to get tx_idx and log_idx?
fn decode_chain_into_events(
    chain: &Chain,
) -> impl Iterator<Item = (&SealedBlockWithSenders, usize, &TransactionSigned, usize, &Log, DaimoNameRegistryEvents)>
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
            DaimoNameRegistryEvents::decode_raw_log(log.topics(), &log.data.data, true)
                .ok()
                .map(|event| (block, tx_idx, tx, log_idx, log, event))
        })
}

fn main() -> eyre::Result<()> {
    reth::cli::Cli::parse_args().run(|builder, _| async move {
        let handle = builder
            .node(EthereumNode::default())
            .install_exex("DaimoNames", |ctx| async move {
                let connection = Connection::open("daimo_names.db")?;
                init(ctx, connection).await
            })
            .launch()
            .await?;

        handle.wait_for_node_exit().await
    })
}

#[cfg(test)]
mod tests {
    use std::pin::pin;

    use alloy_sol_types::SolEvent;
    use reth::revm::db::BundleState;
    use reth_execution_types::{Chain, ExecutionOutcome};
    use reth_exex_test_utils::{test_exex_context, PollOnce};
    use reth_primitives::{
        Address, Block, Header, Log, Receipt, Transaction, TransactionSigned, TxKind, TxLegacy,
        TxType, U256,
    };
    use reth_testing_utils::generators::sign_tx_with_random_key_pair;
    use rusqlite::Connection;

    use crate::{DaimoNameRegistry, NAME_REGISTRY_ADDRESS};

    /// Given the address of name registry contract and an event, construct a transaction signed with a
    /// random private key and a receipt for that transaction.
    fn construct_tx_and_receipt<E: SolEvent>(
        to: Address,
        event: E,
    ) -> eyre::Result<(TransactionSigned, Receipt)> {
        let tx = Transaction::Legacy(TxLegacy { to: TxKind::Call(to), ..Default::default() });
        let log = Log::new(
            to,
            event.encode_topics().into_iter().map(|topic| topic.0).collect(),
            event.encode_data().into(),
        )
        .ok_or_else(|| eyre::eyre!("failed to encode event"))?;
        #[allow(clippy::needless_update)] // side-effect of optimism fields
        let receipt = Receipt {
            tx_type: TxType::Legacy,
            success: true,
            cumulative_gas_used: 0,
            logs: vec![log],
            ..Default::default()
        };
        Ok((sign_tx_with_random_key_pair(&mut rand::thread_rng(), tx), receipt))
    }

    #[tokio::test]
    async fn test_exex() -> eyre::Result<()> {
        // Initialize the test Execution Extension context with all dependencies
        let (ctx, handle) = test_exex_context().await?;
        // Create a temporary database file, so we can access it later for assertions
        let db_file = tempfile::NamedTempFile::new()?;

        // Initialize the ExEx
        let mut exex = pin!(super::init(ctx, Connection::open(&db_file)?).await?);

        // Generate random "from" address for name registry event
        let from_address = Address::random();

        // Construct name registry event, transaction and receipt
        let registration_event = DaimoNameRegistry::Registered {
            name: U256::from(1337).into(),
            addr: from_address,
        };
        let (registration_tx, registration_tx_receipt) =
            construct_tx_and_receipt(NAME_REGISTRY_ADDRESS, registration_event.clone())?;
        
        let chain_id = registration_tx.chain_id();

        // Construct a block
        let block = Block {
            header: Header::default(),
            body: vec![registration_tx.clone()],
            ..Default::default()
        }
        .seal_slow()
        .seal_with_senders()
        .ok_or_else(|| eyre::eyre!("failed to recover senders"))?;

        // Construct a chain
        let chain = Chain::new(
            vec![block.clone()],
            ExecutionOutcome::new(
                BundleState::default(),
                vec![registration_tx_receipt].into(),
                block.number,
                vec![block.requests.clone().unwrap_or_default()],
            ),
            None,
        );

        // Send a notification that the chain has been committed
        handle.send_notification_chain_committed(chain.clone()).await?;
        // Poll the ExEx once, it will process the notification that we just sent
        exex.poll_once().await?;

        let connection = Connection::open(&db_file)?;

        // Assert that the name was parsed correctly and inserted into the database
        let names = connection
            .prepare(r#"SELECT chain_id, block_number, block_hash, tx_index, tx_hash, log_address, log_index, name, addr FROM names"#)?
            .query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?, row.get(7)?, row.get(8)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(names.len(), 1);
        assert_eq!(
            names[0],
            (
                chain_id,
                block.number,
                block.hash().to_string(),
                0,
                registration_tx.hash().to_string(),
                NAME_REGISTRY_ADDRESS.to_string(),
                0,
                registration_event.name.to_string(),
                registration_event.addr.to_string(),
            )
        );

        // Send a notification that the same chain has been reverted
        handle.send_notification_chain_reverted(chain).await?;
        // Poll the ExEx once, it will process the notification that we just sent
        exex.poll_once().await?;

        // Assert that the name was removed from the database
        let names = connection
            .prepare(r#"SELECT chain_id, block_number, block_hash, tx_index, tx_hash, log_address, log_index, name, addr FROM names"#)?
            .query_map([], |_| {
                Ok(())
            })?
            .count();
        assert_eq!(names, 0);

        Ok(())
    }
}
