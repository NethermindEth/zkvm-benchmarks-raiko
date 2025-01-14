pub mod db;
pub mod io;
pub mod mpt;
pub mod state;

use alloy_primitives::Bloom;
use reth_chainspec::ChainSpec;
use reth_ethereum_consensus::validate_block_post_execution;
use reth_evm::execute::{BlockExecutorProvider, ExecutionOutcome, Executor};
use reth_evm_ethereum::execute::EthExecutorProvider;
use reth_primitives::{proofs, BlockExt, Header, Receipts};
use reth_trie::KeccakKeyHasher;
use revm::db::CacheDB;

use crate::io::ClientExecutorInput;

#[derive(Debug, Clone, Default)]
pub struct ClientExecutor;

impl ClientExecutor {
    pub fn execute(&self, mut input: ClientExecutorInput) -> eyre::Result<Header> {
        let witness_db = input.witness_db()?;
        let cache_db = CacheDB::new(&witness_db);
        let spec = ChainSpec::default();

        // Execute the block.
        let executor_block_input = input
            .current_block
            .clone()
            .with_recovered_senders()
            .ok_or("failed to recover senders")
            .unwrap();

        let executor_output = EthExecutorProvider::mainnet()
            .executor(cache_db)
            .execute(&executor_block_input)?;

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

        // Verify the state root
        input
            .parent_state
            .update(&executor_outcome.hash_state_slow::<KeccakKeyHasher>());
        let state_root = input.parent_state.state_root();

        if state_root != input.current_block.state_root {
            eyre::bail!("mismatched state root");
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
            .clone()
            .map(|w| proofs::calculate_withdrawals_root(w.into_inner().as_slice()));
        header.logs_bloom = logs_bloom;

        Ok(header)
    }
}
