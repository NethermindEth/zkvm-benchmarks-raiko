use clap::Parser;
use eyre::Result;
use std::path::PathBuf;
use url::Url;

use block_downloader::BlockDownloader;

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

    // Create downloader instance
    let downloader = BlockDownloader::new(blocks_dir, args.rpc_url)?;

    // Download all requested blocks
    downloader
        .download_and_save_blocks(&args.block_numbers)
        .await?;

    Ok(())
}
