#!/usr/bin/env python3

import argparse
import subprocess


DEFAULT_BLOCKS = [17106222, 19409768]


def download_block(rpc_url, block_number):
    """Download a single block using block-downloader."""
    print(f"Downloading block {block_number}...")
    try:
        subprocess.run(
            [
                "cargo",
                "run",
                "--release",
                "--",
                "--rpc-url",
                rpc_url,
                str(block_number),
            ],
            cwd="block-downloader",
            check=True,
        )
        print(f"Successfully downloaded block {block_number}")
        return True
    except subprocess.CalledProcessError as e:
        print(f"Error downloading block {block_number}:")
        return False


def main():
    parser = argparse.ArgumentParser(
        description="Download Ethereum blocks using block-downloader"
    )
    parser.add_argument("--rpc-url", required=True, help="Ethereum RPC URL")
    parser.add_argument(
        "--blocks",
        nargs="+",
        type=int,
        default=DEFAULT_BLOCKS,
        help=f"List of block numbers to download (default: {DEFAULT_BLOCKS})",
    )

    args = parser.parse_args()

    # Download each block
    success = True
    for block in args.blocks:
        if not download_block(args.rpc_url, block):
            success = False

    if success:
        print("\nAll blocks downloaded successfully!")
    else:
        print("\nSome blocks failed to download. Please check the errors above.")


if __name__ == "__main__":
    main()
