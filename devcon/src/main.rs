use std::{
    str::FromStr,
    sync::{Arc, Mutex},
};

use alloy_primitives::{address, Address, U256};
use alloy_sol_macro::sol;
use alloy_sol_types::SolEvent;
use async_trait::async_trait;
use futures::TryStreamExt;
use jsonrpsee::{
    core::RpcResult,
    proc_macros::rpc,
    types::{error::INTERNAL_ERROR_CODE, ErrorObject},
};
use reth_exex::{ExExContext, ExExEvent};
use reth_node_api::FullNodeComponents;
use reth_node_ethereum::EthereumNode;

const DEPOSIT_CONTRACT_ADDRESS: Address = address!("00000000219ab540356cBB839Cbe05303d7705Fa");

sol! {
    event DepositEvent(
        bytes pubkey,
        bytes withdrawal_credentials,
        bytes amount,
        bytes signature,
        bytes index
    );
}

async fn exex<Node: FullNodeComponents>(
    mut ctx: ExExContext<Node>,
    connection: Arc<Mutex<rusqlite::Connection>>,
) -> eyre::Result<()> {
    while let Some(notification) = ctx.notifications.try_next().await? {
        let connection = connection.lock().unwrap();

        if let Some(reverted_chain) = notification.reverted_chain() {
            // Iterate over blocks
            for block in reverted_chain.blocks_iter() {
                // Iterate over transactions
                for tx in block.transactions() {
                    // Delete the deposit from the database by tx hash
                    connection.execute(
                        "DELETE FROM deposits WHERE tx_hash = ?1",
                        rusqlite::params![tx.hash().to_string()],
                    )?;
                }
            }
        }

        if let Some(committed_chain) = notification.committed_chain() {
            // Iterate over blocks and receipts
            for (block, receipts) in committed_chain.blocks_and_receipts() {
                // Iterate over transactions and receipts
                for (tx, receipt) in block.transactions().zip(receipts) {
                    // Filter only transactions to the deposit contract
                    if tx.to() == Some(DEPOSIT_CONTRACT_ADDRESS) {
                        // Iterate over logs
                        for log in &receipt.as_ref().unwrap().logs {
                            // Filter those logs that decode to the `DepositEvent`
                            if let Ok(deposit) =
                                DepositEvent::decode_raw_log(log.topics(), &log.data.data, true)
                            {
                                // Insert the deposit event into the database
                                let args = rusqlite::params![
                                    tx.hash().to_string(),
                                    tx.recover_signer().unwrap().to_string(),
                                    deposit.amount.to_string()
                                ];
                                connection.execute(
                                r#"INSERT INTO deposits (tx_hash, "from", amount) VALUES (?1, ?2, ?3)"#,
                                args,
                            )?;
                            }
                        }
                    }
                }
            }

            ctx.events.send(ExExEvent::FinishedHeight(committed_chain.tip().num_hash()))?;
        }
    }

    Ok(())
}

#[rpc(server, namespace = "devcon")]
trait DevconRpcExtApi {
    #[method(name = "depositors")]
    async fn depositors(&self, count: usize) -> RpcResult<Vec<(Address, U256)>>;
}

pub struct DevconRpcExt {
    connection: Arc<Mutex<rusqlite::Connection>>,
}

#[async_trait]
impl DevconRpcExtApiServer for DevconRpcExt {
    async fn depositors(&self, count: usize) -> RpcResult<Vec<(Address, U256)>> {
        let connection = self.connection.lock().unwrap();

        // Query depositors from highest to lowest by total amount
        let mut stmt = connection
            .prepare(
                r#"SELECT "from", SUM(amount) amount FROM deposits GROUP BY 1 ORDER BY 2 DESC LIMIT ?"#,
            )
            .map_err(|err| {
                ErrorObject::owned(
                    INTERNAL_ERROR_CODE,
                    format!("failed to prepare query: {err:?}"),
                    None::<()>,
                )
            })?;

        // Decode the depositor addresses and amounts
        let depositors = stmt
            .query_map([count], |row| {
                let from: String = row.get(0)?;
                let amount: String = row.get(1)?;

                Ok((Address::from_str(&from).unwrap(), U256::from_str(&amount).unwrap()))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        Ok(depositors)
    }
}

fn main() -> eyre::Result<()> {
    reth::cli::Cli::parse_args().run(|builder, _| async move {
        let connection = Arc::new(Mutex::new(rusqlite::Connection::open("devcon.db")?));

        connection.lock().unwrap().execute(
            r#"
            CREATE TABLE IF NOT EXISTS deposits (
                id               INTEGER PRIMARY KEY,
                block_number     INTEGER NOT NULL,
                tx_hash          TEXT NOT NULL UNIQUE,
                "from"           TEXT NOT NULL,
                amount           TEXT NOT NULL
            );
            "#,
            (),
        )?;

        let exex_connection = connection.clone();
        let handle = builder
            .node(EthereumNode::default())
            .install_exex("Devcon", move |ctx| async { Ok(exex(ctx, exex_connection)) })
            .extend_rpc_modules(move |ctx| {
                ctx.modules.merge_configured(DevconRpcExt { connection }.into_rpc())?;
                Ok(())
            })
            .launch()
            .await?;

        handle.wait_for_node_exit().await
    })
}
