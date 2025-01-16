mod rpc;

use std::collections::BTreeSet;

use alloy_primitives::{Bloom, B256};
use alloy_provider::{Provider, ReqwestProvider};
use alloy_rpc_types::{BlockTransactionsKind, EIP1186AccountProofResponse};
use eyre::{eyre, Result};
use reth_chainspec::ChainSpec;
use reth_client::{io::ClientExecutorInput, state::EthereumState};
use reth_ethereum_consensus::validate_block_post_execution;
use reth_evm::execute::{BlockExecutorProvider, ExecutionOutcome, Executor};
use reth_evm_ethereum::execute::EthExecutorProvider;
use reth_primitives::{proofs, Account, Block, BlockBody, BlockExt, Receipts, TransactionSigned};
use reth_trie::{AccountProof, KeccakKeyHasher, StorageProof, EMPTY_ROOT_HASH};
use revm::db::CacheDB;
use url::Url;

use rpc::RpcDb;

pub struct BlockDownloader {}

impl BlockDownloader {
    /// Downloads the block with the given block number.
    pub async fn download(block_number: u64, rpc_url: Url) -> Result<ClientExecutorInput> {
        let provider = ReqwestProvider::new_http(rpc_url);

        tracing::info!("fetching blocks {} and {}", block_number, block_number - 1);

        let current_block = provider
            .get_block_by_number(block_number.into(), BlockTransactionsKind::Full)
            .await?
            .map(into_reth_block)
            .ok_or(eyre!("couldn't fetch block {}", block_number))?;

        let previous_block = provider
            .get_block_by_number((block_number - 1).into(), BlockTransactionsKind::Full)
            .await?
            .map(into_reth_block)
            .ok_or(eyre!("couldn't fetch block {}", block_number))?;

        // Setup the database for the block executor.
        let rpc_db = RpcDb::new(provider.clone(), block_number - 1);
        let cache_db = CacheDB::new(&rpc_db);
        let spec = ChainSpec::default();

        // Execute the block and fetch all the necessary data along the way.
        // Repeat the execution made by `ClientExecutor.execute`
        tracing::info!(
            "executing the block and with rpc db: block_number={}, transaction_count={}",
            block_number,
            current_block.body.transactions.len()
        );

        let executor_block_input = current_block
            .clone()
            .with_recovered_senders()
            .ok_or(eyre!("failed to recover senders"))?;
        let executor_output = EthExecutorProvider::mainnet()
            .executor(cache_db)
            .execute(&executor_block_input)?;

        tracing::info!("validating the block post execution");
        validate_block_post_execution(
            &executor_block_input,
            &spec,
            &executor_output.receipts,
            &executor_output.requests,
        )?;

        // Accumulate the logs bloom.
        tracing::info!("accumulating the logs bloom");
        let mut logs_bloom = Bloom::default();
        executor_output.receipts.iter().for_each(|r| {
            logs_bloom.accrue_bloom(&r.bloom_slow());
        });

        // Convert the output to an execution outcome.
        let executor_outcome = ExecutionOutcome::new(
            executor_output.state,
            Receipts::from(executor_output.receipts),
            current_block.header.number,
            vec![executor_output.requests.into()],
        );

        let state_requests = rpc_db.get_state_requests();
        // For every account we touched, fetch the storage proofs for all the slots we touched.
        tracing::info!("fetching storage proofs");
        let mut before_storage_proofs = Vec::new();
        let mut after_storage_proofs = Vec::new();

        for (address, used_keys) in state_requests.iter() {
            let modified_keys = executor_outcome
                .state()
                .state
                .get(address)
                .map(|account| {
                    account
                        .storage
                        .keys()
                        .map(|key| B256::from(*key))
                        .collect::<BTreeSet<_>>()
                })
                .unwrap_or_default()
                .into_iter()
                .collect::<Vec<_>>();

            let keys = used_keys
                .iter()
                .map(|key| B256::from(*key))
                .chain(modified_keys.clone().into_iter())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();

            let storage_proof = provider
                .get_proof(*address, keys.clone())
                .block_id((block_number - 1).into())
                .await?;
            before_storage_proofs.push(eip1186_proof_to_account_proof(storage_proof));

            let storage_proof = provider
                .get_proof(*address, modified_keys)
                .block_id((block_number).into())
                .await?;
            after_storage_proofs.push(eip1186_proof_to_account_proof(storage_proof));
        }

        let state = EthereumState::from_transition_proofs(
            previous_block.state_root,
            &before_storage_proofs
                .iter()
                .map(|item| (item.address, item.clone()))
                .collect(),
            &after_storage_proofs
                .iter()
                .map(|item| (item.address, item.clone()))
                .collect(),
        )?;

        // Verify the state root.
        tracing::info!("verifying the state root");
        let state_root = {
            let mut mutated_state = state.clone();
            mutated_state.update(&executor_outcome.hash_state_slow::<KeccakKeyHasher>());
            mutated_state.state_root()
        };
        if state_root != current_block.state_root {
            eyre::bail!("mismatched state root");
        }

        // Derive the block header.
        //
        // Note: the receipts root and gas used are verified by `validate_block_post_execution`.
        let mut header = current_block.header.clone();
        header.parent_hash = previous_block.hash_slow();
        header.ommers_hash = proofs::calculate_ommers_root(&current_block.body.ommers);
        header.state_root = current_block.state_root;
        header.transactions_root =
            proofs::calculate_transaction_root(&current_block.body.transactions);
        header.receipts_root = current_block.header.receipts_root;
        header.withdrawals_root = current_block
            .body
            .withdrawals
            .clone()
            .map(|w| proofs::calculate_withdrawals_root(w.into_inner().as_slice()));
        header.logs_bloom = logs_bloom;

        // Assert the derived header is correct.
        assert_eq!(
            header.hash_slow(),
            current_block.header.hash_slow(),
            "header mismatch"
        );

        // Log the result.
        tracing::info!(
            "successfully executed block: block_number={}, block_hash={}, state_root={}",
            current_block.header.number,
            header.hash_slow(),
            state_root
        );

        // Fetch the parent headers needed to constrain the BLOCKHASH opcode.
        let oldest_ancestor = *rpc_db.oldest_ancestor.borrow();
        let mut ancestor_headers = vec![];
        tracing::info!(
            "fetching {} ancestor headers",
            block_number - oldest_ancestor
        );
        for height in (oldest_ancestor..=(block_number - 1)).rev() {
            let block = provider
                .get_block_by_number(height.into(), BlockTransactionsKind::Hashes)
                .await?
                .unwrap();
            ancestor_headers.push(block.header.try_into()?);
        }

        // Create the client input.
        let client_input = ClientExecutorInput {
            current_block: current_block.clone(),
            ancestor_headers,
            parent_state: state,
            state_requests,
            bytecodes: rpc_db.get_bytecodes(),
        };
        tracing::info!("successfully generated client input");

        Ok(client_input)
    }
}

fn eip1186_proof_to_account_proof(proof: EIP1186AccountProofResponse) -> AccountProof {
    let address = proof.address;
    let balance = proof.balance;
    let code_hash = proof.code_hash;
    let storage_root = proof.storage_hash;
    let account_proof = proof.account_proof;
    let storage_proofs = proof
        .storage_proof
        .into_iter()
        .map(|storage_proof| {
            let key = storage_proof.key;
            let value = storage_proof.value;
            let proof = storage_proof.proof;
            let mut sp = StorageProof::new(key.as_b256());
            sp.value = value;
            sp.proof = proof;
            sp
        })
        .collect();

    let (storage_root, info) =
        if proof.nonce == 0 && balance.is_zero() && storage_root.is_zero() && code_hash.is_zero() {
            // Account does not exist in state. Return `None` here to prevent proof verification.
            (EMPTY_ROOT_HASH, None)
        } else {
            (
                storage_root,
                Some(Account {
                    nonce: proof.nonce,
                    balance,
                    bytecode_hash: code_hash.into(),
                }),
            )
        };

    AccountProof {
        address,
        info,
        proof: account_proof,
        storage_root,
        storage_proofs,
    }
}

fn into_reth_block(block: alloy_rpc_types::Block) -> Block {
    let block_consensus = block.into_consensus();
    let transactions: Vec<TransactionSigned> = block_consensus
        .body
        .transactions
        .iter()
        .map(|txn| txn.inner.clone().into())
        .collect();

    reth_primitives::Block {
        header: block_consensus.header,
        body: BlockBody {
            transactions,
            ommers: block_consensus.body.ommers,
            withdrawals: block_consensus.body.withdrawals,
        },
    }
}
