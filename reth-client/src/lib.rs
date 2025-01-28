/// Client program input data types.
pub mod custom;
pub mod error;
pub mod io;
pub mod mpt;
pub mod state;

use std::fmt::Display;

use alloy_primitives::Bloom;
use io::ClientExecutorInput;
use reth_errors::ProviderError;
use reth_ethereum_consensus::validate_block_post_execution;
use reth_evm::execute::{
    BlockExecutionError, BlockExecutionOutput, BlockExecutorProvider, Executor,
};
use reth_evm_ethereum::execute::EthExecutorProvider;
use reth_execution_types::ExecutionOutcome;
use reth_primitives::{proofs, BlockWithSenders, Header, Receipt, Receipts};
use revm::{db::WrapDatabaseRef, Database};
use revm_primitives::U256;

use custom::{mainnet, CustomEvmConfig};
use error::ClientError;

/// An executor that executes a block inside a zkVM.
#[derive(Debug, Clone, Default)]
pub struct ClientExecutor;

impl ClientExecutor {
    pub fn execute(&self, mut input: ClientExecutorInput) -> Result<Header, ClientError> {
        // Initialize the witnessed database with verified storage proofs.
        let wrap_ref = WrapDatabaseRef(input.witness_db().unwrap());
        let spec = mainnet();

        // Execute the block.
        let executor_block_input = input
            .current_block
            .clone()
            .with_recovered_senders()
            .ok_or(ClientError::SignatureRecoveryFailed)?;
        let executor_difficulty = input.current_block.header.difficulty;
        let executor_output =
            execute_provider(&executor_block_input, executor_difficulty, wrap_ref)?;

        // Validate the block post execution.
        validate_block_post_execution(
            &executor_block_input,
            &spec,
            &executor_output.receipts,
            &executor_output.requests,
        )?;

        // Accumulate the logs bloom.
        let mut logs_bloom = Bloom::default();
        executor_output.receipts.iter().for_each(|r| {
            logs_bloom.accrue_bloom(&r.bloom_slow());
        });

        // Convert the output to an execution outcome.
        let executor_outcome = ExecutionOutcome::new(
            executor_output.state,
            Receipts::from(executor_output.receipts),
            input.current_block.header.number,
            vec![executor_output.requests.into()],
        );

        // Verify the state root.
        input
            .parent_state
            .update(&executor_outcome.hash_state_slow());
        let state_root = input.parent_state.state_root();

        if state_root != input.current_block.state_root {
            return Err(ClientError::MismatchedStateRoot);
        }

        // Derive the block header.
        //
        // Note: the receipts root and gas used are verified by `validate_block_post_execution`.
        let mut header = input.current_block.header.clone();
        header.parent_hash = input.parent_header().hash_slow();
        header.ommers_hash = proofs::calculate_ommers_root(&input.current_block.body.ommers);
        header.state_root = input.current_block.state_root;
        header.transactions_root =
            proofs::calculate_transaction_root(&input.current_block.body.transactions);
        header.receipts_root = input.current_block.header.receipts_root;
        header.withdrawals_root = input
            .current_block
            .body
            .withdrawals
            .take()
            .map(|w| proofs::calculate_withdrawals_root(w.into_inner().as_slice()));
        header.logs_bloom = logs_bloom;
        header.requests_root = input
            .current_block
            .body
            .requests
            .as_ref()
            .map(|r| proofs::calculate_requests_root(&r.0));

        Ok(header)
    }
}

fn execute_provider<DB>(
    executor_block_input: &BlockWithSenders,
    executor_difficulty: U256,
    cache_db: DB,
) -> Result<BlockExecutionOutput<Receipt>, BlockExecutionError>
where
    DB: Database<Error: Into<ProviderError> + Display>,
{
    EthExecutorProvider::new(mainnet().into(), CustomEvmConfig::from_variant())
        .executor(cache_db)
        .execute((executor_block_input, executor_difficulty).into())
}
