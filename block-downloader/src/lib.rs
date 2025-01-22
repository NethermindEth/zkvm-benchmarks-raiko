mod host;
mod rpc;

use alloy_provider::ReqwestProvider;
use eyre::Result;
use host::HostExecutor;
use reth_client::io::ClientExecutorInput;
use std::{fs, path::PathBuf};
use url::Url;

pub struct BlockDownloader {
    /// The directory where blocks will be saved
    blocks_dir: PathBuf,
    /// The RPC URL to download blocks from
    rpc_url: Url,
}

impl BlockDownloader {
    /// Creates a new BlockDownloader instance
    pub fn new(blocks_dir: PathBuf, rpc_url: Url) -> Result<Self> {
        // Create blocks directory if it doesn't exist
        fs::create_dir_all(&blocks_dir)?;

        Ok(Self {
            blocks_dir,
            rpc_url,
        })
    }

    /// Downloads the block with the given block number.
    async fn download_block(&self, block_number: u64) -> Result<ClientExecutorInput> {
        let provider = ReqwestProvider::new_http(self.rpc_url.clone());
        let executor = HostExecutor::new(provider);

        executor.execute(block_number).await
    }

    /// Downloads and saves a block to disk
    pub async fn download_and_save_block(&self, block_number: u64) -> Result<()> {
        tracing::info!("Downloading block {}", block_number);

        let client_input = self.download_block(block_number).await?;

        // Save to file using bincode
        let file_path = self.blocks_dir.join(format!("{}.bin", block_number));
        let mut block_file = fs::File::create(file_path)?;
        bincode::serialize_into(&mut block_file, &client_input)?;

        tracing::info!("Successfully saved block {}", block_number);
        Ok(())
    }

    /// Downloads and saves multiple blocks to disk
    pub async fn download_and_save_blocks(&self, block_numbers: &[u64]) -> Result<()> {
        for &block_number in block_numbers {
            self.download_and_save_block(block_number).await?;
        }
        Ok(())
    }
}
