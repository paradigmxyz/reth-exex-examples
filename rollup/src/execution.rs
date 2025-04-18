use crate::{db::Database, Zenith, CHAIN_ID, CHAIN_SPEC};
use alloy_consensus::{Blob, SidecarCoder, SimpleCoder, Transaction};
use alloy_eips::{
    eip1559::INITIAL_BASE_FEE, eip2718::Decodable2718, eip4844::kzg_to_versioned_hash,
};
use alloy_primitives::{keccak256, Bytes, B256, U256};
use alloy_rlp::Decodable as _;
use reth::{
    api::Block as _, core::primitives::SignedTransaction, transaction_pool::TransactionPool,
};
use reth_evm::Evm;
use reth_execution_errors::BlockValidationError;
use reth_node_api::ConfigureEvm;
use reth_node_ethereum::{evm::EthEvm, EthEvmConfig};
use reth_primitives::{
    Block, BlockBody, EthereumHardfork, Header, Receipt, Recovered, RecoveredBlock,
    TransactionSigned, TxType,
};
use reth_revm::{
    context::result::{EVMError, ExecutionResult, ResultAndState},
    db::{states::bundle_state::BundleRetention, BundleState, StateBuilder},
    inspector::NoOpInspector,
    DatabaseCommit, State,
};
use reth_tracing::tracing::debug;

/// Execute a rollup block and return (block with recovered senders)[RecoveredBlock], (bundle
/// state)[BundleState] and list of (receipts)[Receipt].
pub async fn execute_block<Pool: TransactionPool>(
    db: &mut Database,
    pool: &Pool,
    tx: &TransactionSigned,
    header: &Zenith::BlockHeader,
    block_data: Bytes,
    block_data_hash: B256,
) -> eyre::Result<(RecoveredBlock<Block>, BundleState, Vec<Receipt>, Vec<ExecutionResult>)> {
    if header.rollupChainId != U256::from(CHAIN_ID) {
        eyre::bail!("Invalid rollup chain ID")
    }

    // Construct header
    let header = construct_header(db, header)?;

    // Decode transactions
    let transactions = decode_transactions(pool, tx, block_data, block_data_hash).await?;

    // Configure EVM
    let evm_config = EthEvmConfig::new(CHAIN_SPEC.clone());
    let mut evm = evm_config
        .evm_for_block(StateBuilder::new_with_database(db).with_bundle_update().build(), &header);

    // Execute transactions
    let (executed_txs, receipts, results) = execute_transactions(&mut evm, &header, transactions)?;

    // Construct block and recover senders
    let block =
        Block { header, body: BlockBody { transactions: executed_txs, ..Default::default() } }
            .try_into_recovered()?;

    let bundle = evm.db_mut().take_bundle();

    Ok((block, bundle, receipts, results))
}

/// Construct header from the given rollup header.
fn construct_header(db: &Database, header: &Zenith::BlockHeader) -> eyre::Result<Header> {
    let parent_block = if !header.sequence.is_zero() {
        db.get_block(header.sequence - U256::from(1))?
    } else {
        None
    };

    let block_number = u64::try_from(header.sequence)?;

    // Calculate base fee per gas for EIP-1559 transactions
    let base_fee_per_gas =
        if CHAIN_SPEC.fork(EthereumHardfork::London).transitions_at_block(block_number) {
            INITIAL_BASE_FEE
        } else {
            parent_block
                .as_ref()
                .ok_or(eyre::eyre!("parent block not found"))?
                .header()
                .next_block_base_fee(CHAIN_SPEC.base_fee_params_at_block(block_number))
                .ok_or(eyre::eyre!("failed to calculate base fee"))?
        };

    // Construct header
    Ok(Header {
        parent_hash: parent_block.map(|block| block.hash()).unwrap_or_default(),
        number: block_number,
        gas_limit: u64::try_from(header.gasLimit)?,
        timestamp: u64::try_from(header.confirmBy)?,
        base_fee_per_gas: Some(base_fee_per_gas),
        ..Default::default()
    })
}

/// Decode transactions from the block data and recover senders.
/// - If the transaction is a blob-carrying one, decode the blobs either using the local transaction
///   pool, or querying Blobscan.
/// - If the transaction is a regular one, decode the block data directly.
async fn decode_transactions<Pool: TransactionPool>(
    pool: &Pool,
    tx: &TransactionSigned,
    block_data: Bytes,
    block_data_hash: B256,
) -> eyre::Result<Vec<Recovered<TransactionSigned>>> {
    // Get raw transactions either from the blobs, or directly from the block data
    let raw_transactions = if matches!(tx.tx_type(), TxType::Eip4844) {
        let blobs: Vec<_> = if let Some(sidecar) = pool.get_blob(*tx.hash())? {
            // Try to get blobs from the transaction pool
            sidecar.blobs.clone().into_iter().zip(sidecar.commitments.clone()).collect()
        } else {
            // If transaction is not found in the pool, try to get blobs from Blobscan
            let blobscan_client = foundry_blob_explorers::Client::holesky();
            let sidecar = blobscan_client.transaction(tx.hash().0.into()).await?.blob_sidecar();
            sidecar
                .blobs
                .into_iter()
                .map(|blob| (*blob).into())
                .zip(sidecar.commitments.into_iter().map(|commitment| (*commitment).into()))
                .collect()
        };

        // Decode blob hashes from block data
        let blob_hashes = Vec::<B256>::decode(&mut block_data.as_ref())?;

        // Filter blobs that are present in the block data
        let blobs = blobs
            .into_iter()
            // Convert blob KZG commitments to versioned hashes
            .map(|(blob, commitment)| (blob, kzg_to_versioned_hash(commitment.as_slice())))
            // Filter only blobs that are present in the block data
            .filter(|(_, hash)| blob_hashes.contains(hash))
            .map(|(blob, _)| Blob::from(*blob))
            .collect::<Vec<_>>();
        if blobs.len() != blob_hashes.len() {
            eyre::bail!("some blobs not found")
        }

        // Decode blobs and concatenate them to get the raw transactions
        let data = SimpleCoder::default()
            .decode_all(&blobs)
            .ok_or(eyre::eyre!("failed to decode blobs"))?
            .concat();

        data.into()
    } else {
        block_data
    };

    let raw_transaction_hash = keccak256(&raw_transactions);
    if raw_transaction_hash != block_data_hash {
        eyre::bail!("block data hash mismatch")
    }

    // Decode block data, filter only transactions with the correct chain ID and recover senders
    let raw_transactions: Vec<Vec<u8>> = alloy_rlp::decode_exact(raw_transactions.as_ref())?;
    let mut transactions = Vec::with_capacity(raw_transactions.len());
    for raw_transaction in raw_transactions {
        let tx = TransactionSigned::decode_2718(&mut &raw_transaction[..])?;
        if tx.chain_id() == Some(CHAIN_ID) {
            let sender = tx.recover_signer()?;
            transactions.push(tx.with_signer(sender));
        }
    }

    Ok(transactions)
}

/// Execute transactions and return the list of executed transactions, receipts and
/// execution results.
fn execute_transactions<DB: reth_evm::Database>(
    evm: &mut EthEvm<State<DB>, NoOpInspector>,
    header: &Header,
    transactions: Vec<Recovered<TransactionSigned>>,
) -> eyre::Result<(Vec<TransactionSigned>, Vec<Receipt>, Vec<ExecutionResult>)>
where
    DB::Error: Send,
{
    let mut receipts = Vec::with_capacity(transactions.len());
    let mut executed_txs = Vec::with_capacity(transactions.len());
    let mut results = Vec::with_capacity(transactions.len());
    if !transactions.is_empty() {
        let mut cumulative_gas_used = 0;
        for transaction in transactions {
            // The sum of the transaction’s gas limit, Tg, and the gas utilized in this block prior,
            // must be no greater than the block’s gasLimit.
            let block_available_gas = header.gas_limit - cumulative_gas_used;
            if transaction.gas_limit() > block_available_gas {
                return Err(BlockValidationError::TransactionGasLimitMoreThanAvailableBlockGas {
                    transaction_gas_limit: transaction.gas_limit(),
                    block_available_gas,
                }
                .into());
            }
            // Execute transaction.
            let ResultAndState { result, state } = match evm.transact(&transaction) {
                Ok(result) => result,
                Err(err) => {
                    match err {
                        EVMError::Transaction(err) => {
                            // if the transaction is invalid, we can skip it
                            debug!(%err, ?transaction, "Skipping invalid transaction");
                            continue;
                        }
                        _ => {
                            // this is an error that we should treat as fatal for this attempt
                            eyre::bail!("db error")
                        }
                    }
                }
            };

            debug!(?transaction, ?result, ?state, "Executed transaction");

            evm.db_mut().commit(state);

            // append gas used
            cumulative_gas_used += result.gas_used();

            // Push transaction changeset and calculate header bloom filter for receipt.
            #[allow(clippy::needless_update)] // side-effect of optimism fields
            receipts.push(Receipt {
                tx_type: transaction.tx_type(),
                success: result.is_success(),
                cumulative_gas_used,
                logs: result.logs().to_vec(),
                ..Default::default()
            });

            // append transaction to the list of executed transactions
            executed_txs.push(transaction.into_inner());
            results.push(result);
        }

        evm.db_mut().merge_transitions(BundleRetention::Reverts);
    }

    Ok((executed_txs, receipts, results))
}

#[cfg(test)]
mod tests {
    use crate::{
        db::Database, execute_block, Zenith::BlockHeader, CHAIN_ID, CHAIN_SPEC,
        ROLLUP_SUBMITTER_ADDRESS,
    };
    use alloy_consensus::{constants::ETH_TO_WEI, SidecarBuilder, SimpleCoder, TxEip2930};
    use alloy_eips::eip2718::Encodable2718;
    use alloy_primitives::{bytes, keccak256, BlockNumber, TxKind, U256};
    use alloy_sol_types::{sol, SolCall};
    use reth_evm::{ConfigureEvm, Evm};
    use reth_node_ethereum::EthEvmConfig;
    use reth_primitives::{public_key_to_address, Block, Receipt, RecoveredBlock, Transaction};
    use reth_revm::{
        context::{
            result::{ExecutionResult, Output},
            TxEnv,
        },
        state::AccountInfo,
    };
    use reth_testing_utils::generators::{self, sign_tx_with_key_pair};
    use reth_transaction_pool::{
        test_utils::{testing_pool, MockTransaction},
        TransactionOrigin, TransactionPool,
    };
    use rusqlite::Connection;
    use secp256k1::Keypair;
    use std::time::{SystemTime, UNIX_EPOCH};

    sol!(
        WETH,
        r#"
[
   {
      "constant":true,
      "inputs":[
         {
            "name":"",
            "type":"address"
         }
      ],
      "name":"balanceOf",
      "outputs":[
         {
            "name":"",
            "type":"uint256"
         }
      ],
      "payable":false,
      "stateMutability":"view",
      "type":"function"
   }
]
        "#
    );

    #[tokio::test]
    async fn test_execute_block() -> eyre::Result<()> {
        reth_tracing::init_test_tracing();

        let mut database = Database::new(Connection::open_in_memory()?)?;
        let evm_config = EthEvmConfig::new(CHAIN_SPEC.clone());

        // Create key pair
        let key_pair = generators::generate_key(&mut generators::rng());
        let sender_address = public_key_to_address(key_pair.public_key());

        // Deposit some ETH to the sender and insert it into database
        database.upsert_account(sender_address, |_| {
            Ok(AccountInfo { balance: U256::from(ETH_TO_WEI), nonce: 1, ..Default::default() })
        })?;

        // WETH deployment transaction sent using calldata
        let (_, _, results) = execute_transaction(
            &mut database,
            key_pair,
            0,
            Transaction::Eip2930(TxEip2930 {
                chain_id: CHAIN_ID,
                nonce: 1,
                gas_limit: 1_500_000,
                gas_price: 1_500_000_000,
                to: TxKind::Create,
                // WETH9 bytecode
                input: bytes!("60606040526040805190810160405280600d81526020017f57726170706564204574686572000000000000000000000000000000000000008152506000908051906020019061004f9291906100c8565b506040805190810160405280600481526020017f57455448000000000000000000000000000000000000000000000000000000008152506001908051906020019061009b9291906100c8565b506012600260006101000a81548160ff021916908360ff16021790555034156100c357600080fd5b61016d565b828054600181600116156101000203166002900490600052602060002090601f016020900481019282601f1061010957805160ff1916838001178555610137565b82800160010185558215610137579182015b8281111561013657825182559160200191906001019061011b565b5b5090506101449190610148565b5090565b61016a91905b8082111561016657600081600090555060010161014e565b5090565b90565b610c348061017c6000396000f3006060604052600436106100af576000357c0100000000000000000000000000000000000000000000000000000000900463ffffffff16806306fdde03146100b9578063095ea7b31461014757806318160ddd146101a157806323b872dd146101ca5780632e1a7d4d14610243578063313ce5671461026657806370a082311461029557806395d89b41146102e2578063a9059cbb14610370578063d0e30db0146103ca578063dd62ed3e146103d4575b6100b7610440565b005b34156100c457600080fd5b6100cc6104dd565b6040518080602001828103825283818151815260200191508051906020019080838360005b8381101561010c5780820151818401526020810190506100f1565b50505050905090810190601f1680156101395780820380516001836020036101000a031916815260200191505b509250505060405180910390f35b341561015257600080fd5b610187600480803573ffffffffffffffffffffffffffffffffffffffff1690602001909190803590602001909190505061057b565b604051808215151515815260200191505060405180910390f35b34156101ac57600080fd5b6101b461066d565b6040518082815260200191505060405180910390f35b34156101d557600080fd5b610229600480803573ffffffffffffffffffffffffffffffffffffffff1690602001909190803573ffffffffffffffffffffffffffffffffffffffff1690602001909190803590602001909190505061068c565b604051808215151515815260200191505060405180910390f35b341561024e57600080fd5b61026460048080359060200190919050506109d9565b005b341561027157600080fd5b610279610b05565b604051808260ff1660ff16815260200191505060405180910390f35b34156102a057600080fd5b6102cc600480803573ffffffffffffffffffffffffffffffffffffffff16906020019091905050610b18565b6040518082815260200191505060405180910390f35b34156102ed57600080fd5b6102f5610b30565b6040518080602001828103825283818151815260200191508051906020019080838360005b8381101561033557808201518184015260208101905061031a565b50505050905090810190601f1680156103625780820380516001836020036101000a031916815260200191505b509250505060405180910390f35b341561037b57600080fd5b6103b0600480803573ffffffffffffffffffffffffffffffffffffffff16906020019091908035906020019091905050610bce565b604051808215151515815260200191505060405180910390f35b6103d2610440565b005b34156103df57600080fd5b61042a600480803573ffffffffffffffffffffffffffffffffffffffff1690602001909190803573ffffffffffffffffffffffffffffffffffffffff16906020019091905050610be3565b6040518082815260200191505060405180910390f35b34600360003373ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff168152602001908152602001600020600082825401925050819055503373ffffffffffffffffffffffffffffffffffffffff167fe1fffcc4923d04b559f4d29a8bfc6cda04eb5b0d3c460751c2402c5c5cc9109c346040518082815260200191505060405180910390a2565b60008054600181600116156101000203166002900480601f0160208091040260200160405190810160405280929190818152602001828054600181600116156101000203166002900480156105735780601f1061054857610100808354040283529160200191610573565b820191906000526020600020905b81548152906001019060200180831161055657829003601f168201915b505050505081565b600081600460003373ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200190815260200160002060008573ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff168152602001908152602001600020819055508273ffffffffffffffffffffffffffffffffffffffff163373ffffffffffffffffffffffffffffffffffffffff167f8c5be1e5ebec7d5bd14f71427d1e84f3dd0314c0f7b2291e5b200ac8c7c3b925846040518082815260200191505060405180910390a36001905092915050565b60003073ffffffffffffffffffffffffffffffffffffffff1631905090565b600081600360008673ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200190815260200160002054101515156106dc57600080fd5b3373ffffffffffffffffffffffffffffffffffffffff168473ffffffffffffffffffffffffffffffffffffffff16141580156107b457507fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff600460008673ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200190815260200160002060003373ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff1681526020019081526020016000205414155b156108cf5781600460008673ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200190815260200160002060003373ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff168152602001908152602001600020541015151561084457600080fd5b81600460008673ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200190815260200160002060003373ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff168152602001908152602001600020600082825403925050819055505b81600360008673ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff1681526020019081526020016000206000828254039250508190555081600360008573ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff168152602001908152602001600020600082825401925050819055508273ffffffffffffffffffffffffffffffffffffffff168473ffffffffffffffffffffffffffffffffffffffff167fddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef846040518082815260200191505060405180910390a3600190509392505050565b80600360003373ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff1681526020019081526020016000205410151515610a2757600080fd5b80600360003373ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff168152602001908152602001600020600082825403925050819055503373ffffffffffffffffffffffffffffffffffffffff166108fc829081150290604051600060405180830381858888f193505050501515610ab457600080fd5b3373ffffffffffffffffffffffffffffffffffffffff167f7fcf532c15f0a6db0bd6d0e038bea71d30d808c7d98cb3bf7268a95bf5081b65826040518082815260200191505060405180910390a250565b600260009054906101000a900460ff1681565b60036020528060005260406000206000915090505481565b60018054600181600116156101000203166002900480601f016020809104026020016040519081016040528092919081815260200182805460018160011615610100020316600290048015610bc65780601f10610b9b57610100808354040283529160200191610bc6565b820191906000526020600020905b815481529060010190602001808311610ba957829003601f168201915b505050505081565b6000610bdb33848461068c565b905092915050565b60046020528160005260406000206020528060005260406000206000915091505054815600a165627a7a72305820deb4c2ccab3c2fdca32ab3f46728389c2fe2c165d5fafa07661e4e004f6c344a0029"),
                ..Default::default()
            }),
            BlockDataSource::Calldata
        ).await?;

        let weth_address = match results.first() {
            Some(ExecutionResult::Success { output: Output::Create(_, Some(address)), .. }) => {
                *address
            }
            _ => eyre::bail!("WETH contract address not found"),
        };

        // WETH deposit transaction sent using blobs
        execute_transaction(
            &mut database,
            key_pair,
            1,
            Transaction::Eip2930(TxEip2930 {
                chain_id: CHAIN_ID,
                nonce: 2,
                gas_limit: 50000,
                gas_price: 1_500_000_000,
                to: TxKind::Call(weth_address),
                value: U256::from(0.5 * ETH_TO_WEI as f64),
                input: bytes!("d0e30db0"),
                ..Default::default()
            }),
            BlockDataSource::Blobs,
        )
        .await?;

        // Verify WETH balance
        let mut evm = evm_config.evm_with_env(&mut database, Default::default());
        let result = evm
            .transact(TxEnv {
                caller: sender_address,
                gas_limit: 50_000_000,
                kind: TxKind::Call(weth_address),
                data: WETH::balanceOfCall::new((sender_address,)).abi_encode().into(),
                nonce: 3,
                ..Default::default()
            })
            .map_err(|err| eyre::eyre!(err))?
            .result;
        assert_eq!(
            result.output(),
            Some(&U256::from(0.5 * ETH_TO_WEI as f64).to_be_bytes_vec().into())
        );
        drop(evm);

        // Verify nonce
        let account = database.get_account(sender_address)?.unwrap();
        assert_eq!(account.nonce, 3);

        // Revert block with WETH deposit transaction
        database.revert_tip_block(U256::from(1))?;

        // Verify WETH balance after revert
        let mut evm = evm_config.evm_with_env(&mut database, Default::default());
        let result = evm
            .transact(TxEnv {
                caller: sender_address,
                gas_limit: 50_000_000,
                kind: TxKind::Call(weth_address),
                data: WETH::balanceOfCall::new((sender_address,)).abi_encode().into(),
                nonce: 2,
                ..Default::default()
            })
            .map_err(|err| eyre::eyre!(err))?
            .result;

        assert_eq!(result.output(), Some(&U256::ZERO.to_be_bytes_vec().into()));
        drop(evm);

        // Verify nonce after revert
        let account = database.get_account(sender_address)?.unwrap();
        assert_eq!(account.nonce, 2);

        Ok(())
    }

    enum BlockDataSource {
        Calldata,
        Blobs,
    }

    async fn execute_transaction(
        database: &mut Database,
        key_pair: Keypair,
        sequence: BlockNumber,
        tx: Transaction,
        block_data_source: BlockDataSource,
    ) -> eyre::Result<(RecoveredBlock<Block>, Vec<Receipt>, Vec<ExecutionResult>)> {
        // Construct block header
        let block_header = BlockHeader {
            rollupChainId: U256::from(CHAIN_ID),
            sequence: U256::from(sequence),
            confirmBy: U256::from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs()),
            gasLimit: U256::from(30_000_000),
            rewardAddress: ROLLUP_SUBMITTER_ADDRESS,
        };
        let encoded_transactions =
            alloy_rlp::encode(vec![sign_tx_with_key_pair(key_pair, tx).encoded_2718()]);
        let block_data_hash = keccak256(&encoded_transactions);

        let pool = testing_pool();

        let (block_data, l1_transaction) = match block_data_source {
            BlockDataSource::Calldata => (
                encoded_transactions,
                sign_tx_with_key_pair(key_pair, Transaction::Eip2930(TxEip2930::default())),
            ),
            BlockDataSource::Blobs => {
                let sidecar =
                    SidecarBuilder::<SimpleCoder>::from_slice(&encoded_transactions).build()?;
                let blob_hashes = alloy_rlp::encode(sidecar.versioned_hashes().collect::<Vec<_>>());

                let mut mock_transaction = MockTransaction::eip4844_with_sidecar(sidecar);
                let transaction =
                    sign_tx_with_key_pair(key_pair, Transaction::from(mock_transaction.clone()));
                mock_transaction.set_hash(*transaction.hash());
                pool.add_transaction(TransactionOrigin::Local, mock_transaction).await?;
                (blob_hashes, transaction)
            }
        };

        // Execute block and insert into database
        let (block, bundle, receipts, results) = execute_block(
            database,
            &pool,
            &l1_transaction,
            &block_header,
            block_data.into(),
            block_data_hash,
        )
        .await?;
        database.insert_block_with_bundle(&block, bundle)?;

        Ok((block, receipts, results))
    }
}
