mod interfaces;
mod preflight;
mod provider;

use std::{
    fs::{self, File},
    path::PathBuf,
};
use std::collections::HashMap;
use std::io::Write;
use alloy_primitives::Address;
use alloy_rpc_types::EIP1186AccountProofResponse;
use clap::Parser;
use serde::Serialize;
use serde_json::ser::{Formatter, PrettyFormatter};
use serde_json::Serializer;
use raiko_lib::consts::{ChainSpec, SupportedChainSpecs};
use raiko_lib::input::{BlobProofType, GuestInput, TaikoProverData};
use raiko_lib::primitives::B256;
use crate::interfaces::ProofRequest;
use crate::preflight::{preflight, PreflightData};
use crate::provider::BlockDataProvider;
use crate::provider::rpc::RpcBlockDataProvider;

pub type RaikoResult<T> = Result<T, anyhow::Error>;

pub type MerkleProof = HashMap<Address, EIP1186AccountProofResponse>;

#[derive(Parser)]
#[command(about = "Download blocks and save them to disk")]
struct Args {
    /// List of block numbers to download
    #[arg(required = true)]
    block_numbers: Vec<u64>,

    #[arg(required = true)]
    taiko_network: String,

    #[arg(required = true)]
    l1_network: String,
}

#[tokio::main]
async fn main() -> RaikoResult<()> {
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

    let taiko_network = args.taiko_network;
    let l1_network = args.l1_network;
    let taiko_chain_spec = SupportedChainSpecs::default()
        .get_chain_spec(&taiko_network)
        .unwrap();
    let l1_chain_spec = SupportedChainSpecs::default()
        .get_chain_spec(&l1_network)
        .unwrap();

    // Create blocks directory in eval if it doesn't exist
    let blocks_dir = PathBuf::from(format!("eval/taiko-blocks-{taiko_network}"));
    fs::create_dir_all(&blocks_dir)?;

    for block_number in args.block_numbers {
        let provider =
            RpcBlockDataProvider::new(&taiko_chain_spec.rpc, block_number - 1)
                .expect("Could not create RpcBlockDataProvider");

        let proof_request = ProofRequest {
            block_number,
            l1_inclusion_block_number: 0,
            network: taiko_network.clone(),
            graffiti: B256::ZERO,
            prover: Address::ZERO,
            l1_network: l1_network.clone(),
            blob_proof_type: BlobProofType::ProofOfEquivalence,
        };
        tracing::info!("Downloading block {}", block_number);
        let raiko = Raiko::new(l1_chain_spec.clone(), taiko_chain_spec.clone(), proof_request.clone());
        let input = raiko
            .generate_input(provider)
            .await
            .expect("input generation failed");
        let input_json = {
            let mut buf = Vec::with_capacity(65536);
            let fmt = CuteFormatter::new();
            let mut serializer = Serializer::with_formatter(&mut buf, fmt);
            input.serialize(&mut serializer).expect("Failed to serialize GuestInput");
            String::from_utf8(buf).expect("Non-UTF8 serialization of GuestInput")
        };

        let file_path = blocks_dir.join(format!("{}.json", block_number));
        let mut file = File::create(file_path)?;

        file.write_all(input_json.as_bytes())?;

        tracing::info!("Successfully saved block {}", block_number);
    }

    Ok(())
}


pub struct Raiko {
    l1_chain_spec: ChainSpec,
    taiko_chain_spec: ChainSpec,
    request: ProofRequest,
}

impl Raiko {
    pub fn new(
        l1_chain_spec: ChainSpec,
        taiko_chain_spec: ChainSpec,
        request: ProofRequest,
    ) -> Self {
        Self {
            l1_chain_spec,
            taiko_chain_spec,
            request,
        }
    }

    pub async fn generate_input<BDP: BlockDataProvider>(
        &self,
        provider: BDP,
    ) -> RaikoResult<GuestInput> {
        let preflight_data = PreflightData::new(
            self.request.block_number,
            self.request.l1_inclusion_block_number,
            self.l1_chain_spec.to_owned(),
            self.taiko_chain_spec.to_owned(),
            TaikoProverData {
                graffiti: self.request.graffiti,
                prover: self.request.prover,
            },
            self.request.blob_proof_type.clone(),
        );
        Ok(preflight(provider, preflight_data).await?)
    }
}

/// Like [PrettyFormatter], but places array element on the same line.
/// Does not try to be particularly efficient.
struct CuteFormatter<'a> {
    pretty_formatter: PrettyFormatter<'a>
}

impl<'a> CuteFormatter<'a> {
    pub fn new() -> Self {
        CuteFormatter {
            pretty_formatter: PrettyFormatter::new(),
        }
    }
}

impl<'a> Formatter for CuteFormatter<'a> {
    #[inline]
    fn begin_array<W>(&mut self, writer: &mut W) -> std::io::Result<()>
    where
        W: ?Sized + Write,
    {
        self.pretty_formatter.begin_array(writer)
    }

    #[inline]
    fn end_array<W>(&mut self, writer: &mut W) -> std::io::Result<()>
    where
        W: ?Sized + Write,
    {
        self.pretty_formatter.end_array(writer)
    }

    #[inline]
    fn begin_array_value<W>(&mut self, writer: &mut W, first: bool) -> std::io::Result<()>
    where
        W: ?Sized + Write,
    {
        if first {
            self.pretty_formatter.begin_array_value(writer, first)
        } else {
            writer.write_all(b", ")
        }
    }

    #[inline]
    fn end_array_value<W>(&mut self, writer: &mut W) -> std::io::Result<()>
    where
        W: ?Sized + Write,
    {
        self.pretty_formatter.end_array_value(writer)
    }

    #[inline]
    fn begin_object<W>(&mut self, writer: &mut W) -> std::io::Result<()>
    where
        W: ?Sized + Write,
    {
        self.pretty_formatter.begin_object(writer)
    }

    #[inline]
    fn end_object<W>(&mut self, writer: &mut W) -> std::io::Result<()>
    where
        W: ?Sized + Write,
    {
        self.pretty_formatter.end_object(writer)
    }

    #[inline]
    fn begin_object_key<W>(&mut self, writer: &mut W, first: bool) -> std::io::Result<()>
    where
        W: ?Sized + Write,
    {
        self.pretty_formatter.begin_object_key(writer, first)
    }

    #[inline]
    fn begin_object_value<W>(&mut self, writer: &mut W) -> std::io::Result<()>
    where
        W: ?Sized + Write,
    {
        self.pretty_formatter.begin_object_value(writer)
    }

    #[inline]
    fn end_object_value<W>(&mut self, writer: &mut W) -> std::io::Result<()>
    where
        W: ?Sized + Write,
    {
        self.pretty_formatter.end_object_value(writer)
    }
}
