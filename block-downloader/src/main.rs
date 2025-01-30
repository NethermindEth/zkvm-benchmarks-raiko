use alloy_provider::ReqwestProvider;
use clap::Parser;
use eyre::Result;
use std::{
    fs::{self, File},
    io::{BufWriter, Write},
    path::PathBuf,
};
use url::Url;

use block_downloader::HostExecutor;

#[derive(Parser)]
#[command(about = "Download blocks and save them to disk")]
struct Args {
    /// List of block numbers to download
    #[arg(required = true)]
    block_numbers: Vec<u64>,

    /// RPC URL to download blocks from
    #[arg(long, default_value = "http://localhost:8545")]
    rpc_url: Url,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing subscriber
    tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .compact()
        .init();

    let args = Args::parse();

    // Create blocks directory in eval if it doesn't exist
    let blocks_dir = PathBuf::from("../eval/blocks");
    fs::create_dir_all(&blocks_dir)?;

    let provider = ReqwestProvider::new_http(args.rpc_url);
    let executor = HostExecutor::new(provider);

    for block_number in args.block_numbers {
        tracing::info!("Downloading block {}", block_number);
        let client_input = executor.execute(block_number).await?;

        let file_path = blocks_dir.join(format!("{}.json", block_number));
        let file = File::create(file_path)?;
        let mut writer = BufWriter::new(file);

        serde_json::to_writer(&mut writer, &client_input)?;
        writer.flush()?;

        tracing::info!("Successfully saved block {}", block_number);
    }

    Ok(())
}
