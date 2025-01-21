mod host;
mod rpc;

use alloy_provider::ReqwestProvider;
use eyre::Result;
use host::HostExecutor;
use reth_client::io::ClientExecutorInput;
use url::Url;

pub struct BlockDownloader {}

impl BlockDownloader {
    /// Downloads the block with the given block number.
    pub async fn download(block_number: u64, rpc_url: Url) -> Result<ClientExecutorInput> {
        let provider = ReqwestProvider::new_http(rpc_url);
        let executor = HostExecutor::new(provider);

        executor.execute(block_number).await
    }
}
