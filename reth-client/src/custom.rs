use std::sync::Arc;

use alloy_primitives::{Address, Bytes};
use reth_chainspec::{
    once_cell_set, BaseFeeParams, BaseFeeParamsKind, Chain, ChainHardforks, ChainSpec,
    DepositContract, EthereumHardfork, ForkCondition,
};
use reth_evm::{ConfigureEvm, ConfigureEvmEnv, NextBlockEnvAttributes};
use reth_evm_ethereum::EthEvmConfig;
use reth_primitives::{
    constants::ETHEREUM_BLOCK_GAS_LIMIT,
    revm_primitives::{CfgEnvWithHandlerCfg, TxEnv},
    Header, TransactionSigned, MAINNET_GENESIS_HASH,
};
use reth_revm::{
    handler::register::EvmHandler, precompile::PrecompileSpecId, primitives::Env,
    ContextPrecompiles, Database, Evm, EvmBuilder,
};
use revm::precompile::{
    bn128, kzg_point_evaluation, secp256k1, Precompile, PrecompileResult, PrecompileWithAddress,
};
use revm_primitives::{address, b256, BlockEnv, U256};

/// Returns the [ChainSpec] for Ethereum mainnet.
pub fn mainnet() -> ChainSpec {
    // Spec extracted from:
    //
    // https://github.com/paradigmxyz/reth/blob/c228fe15808c3acbf18dc3af1a03ef5cbdcda07a/crates/chainspec/src/spec.rs#L35-L60
    let mut spec = ChainSpec {
        chain: Chain::mainnet(),
        // We don't need the genesis state. Using default to save cycles.
        genesis: Default::default(),
        genesis_header: Default::default(),
        genesis_hash: once_cell_set(MAINNET_GENESIS_HASH),
        paris_block_and_final_difficulty: Some((0, U256::ZERO)),
        // For some reasons a state root mismatch error arises if we don't force activate everything
        // before and including Shanghai.
        hardforks: ChainHardforks::new(vec![
            (EthereumHardfork::Frontier.boxed(), ForkCondition::Block(0)),
            (EthereumHardfork::Homestead.boxed(), ForkCondition::Block(0)),
            (EthereumHardfork::Dao.boxed(), ForkCondition::Block(0)),
            (EthereumHardfork::Tangerine.boxed(), ForkCondition::Block(0)),
            (
                EthereumHardfork::SpuriousDragon.boxed(),
                ForkCondition::Block(0),
            ),
            (EthereumHardfork::Byzantium.boxed(), ForkCondition::Block(0)),
            (
                EthereumHardfork::Constantinople.boxed(),
                ForkCondition::Block(0),
            ),
            (
                EthereumHardfork::Petersburg.boxed(),
                ForkCondition::Block(0),
            ),
            (EthereumHardfork::Istanbul.boxed(), ForkCondition::Block(0)),
            (
                EthereumHardfork::MuirGlacier.boxed(),
                ForkCondition::Block(0),
            ),
            (EthereumHardfork::Berlin.boxed(), ForkCondition::Block(0)),
            (EthereumHardfork::London.boxed(), ForkCondition::Block(0)),
            (
                EthereumHardfork::ArrowGlacier.boxed(),
                ForkCondition::Block(0),
            ),
            (
                EthereumHardfork::GrayGlacier.boxed(),
                ForkCondition::Block(0),
            ),
            (
                EthereumHardfork::Paris.boxed(),
                ForkCondition::TTD {
                    fork_block: Some(0),
                    total_difficulty: U256::ZERO,
                },
            ),
            (
                EthereumHardfork::Shanghai.boxed(),
                ForkCondition::Timestamp(0),
            ),
            (
                EthereumHardfork::Cancun.boxed(),
                ForkCondition::Timestamp(1710338135),
            ),
        ]),
        deposit_contract: Some(DepositContract::new(
            address!("00000000219ab540356cbb839cbe05303d7705fa"),
            11052984,
            b256!("649bbc62d0e31342afea4e5cd82d4049e7e1ee912fc0889aa790803be39038c5"),
        )),
        base_fee_params: BaseFeeParamsKind::Constant(BaseFeeParams::ethereum()),
        max_gas_limit: ETHEREUM_BLOCK_GAS_LIMIT,
        prune_delete_limit: 20000,
    };
    spec.genesis.config.dao_fork_support = true;
    spec
}

/// Create an annotated precompile that tracks the cycle count of a precompile.
/// This is useful for tracking how many cycles in total are consumed by calls to a given
/// precompile.
macro_rules! create_annotated_precompile {
    ($precompile:expr, $name:expr) => {
        PrecompileWithAddress(
            $precompile.0,
            Precompile::Standard(|input: &Bytes, gas_limit: u64| -> PrecompileResult {
                let precompile = $precompile.precompile();
                match precompile {
                    Precompile::Standard(precompile) => precompile(input, gas_limit),
                    _ => panic!("Annotated precompile must be a standard precompile."),
                }
            }),
        )
    };
}

// An annotated version of the KZG point evaluation precompile. Because this is a stateful
// precompile we cannot use the `create_annotated_precompile` macro
pub(crate) const ANNOTATED_KZG_PROOF: PrecompileWithAddress = PrecompileWithAddress(
    kzg_point_evaluation::POINT_EVALUATION.0,
    Precompile::Env(
        |input: &Bytes, gas_limit: u64, env: &Env| -> PrecompileResult {
            let precompile = kzg_point_evaluation::POINT_EVALUATION.precompile();
            match precompile {
                Precompile::Env(precompile) => precompile(input, gas_limit, env),
                _ => panic!("Annotated precompile must be a env precompile."),
            }
        },
    ),
);

pub(crate) const ANNOTATED_ECRECOVER: PrecompileWithAddress =
    create_annotated_precompile!(secp256k1::ECRECOVER, "ecrecover");
pub(crate) const ANNOTATED_BN_ADD: PrecompileWithAddress =
    create_annotated_precompile!(bn128::add::ISTANBUL, "bn-add");
pub(crate) const ANNOTATED_BN_MUL: PrecompileWithAddress =
    create_annotated_precompile!(bn128::mul::ISTANBUL, "bn-mul");
pub(crate) const ANNOTATED_BN_PAIR: PrecompileWithAddress =
    create_annotated_precompile!(bn128::pair::ISTANBUL, "bn-pair");

/// Custom EVM configuration
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct CustomEvmConfig;

impl CustomEvmConfig {
    /// Sets the precompiles to the EVM handler
    ///
    /// This will be invoked when the EVM is created via [ConfigureEvm::evm] or
    /// [ConfigureEvm::evm_with_inspector]
    ///
    /// This will use the default mainnet precompiles and add additional precompiles.
    fn set_precompiles<EXT, DB>(handler: &mut EvmHandler<'_, EXT, DB>)
    where
        DB: Database,
    {
        // first we need the evm spec id, which determines the precompiles
        let spec_id = handler.cfg.spec_id;
        // install the precompiles
        handler.pre_execution.load_precompiles = Arc::new(move || {
            let mut loaded_precompiles: ContextPrecompiles<DB> =
                ContextPrecompiles::new(PrecompileSpecId::from_spec_id(spec_id));
            loaded_precompiles.extend(vec![
                ANNOTATED_ECRECOVER,
                ANNOTATED_BN_ADD,
                ANNOTATED_BN_MUL,
                ANNOTATED_BN_PAIR,
                ANNOTATED_KZG_PROOF,
            ]);

            loaded_precompiles
        });
    }

    pub fn from_variant() -> Self {
        Self {}
    }
}

impl ConfigureEvm for CustomEvmConfig {
    type DefaultExternalContext<'a> = ();

    fn evm<DB: Database>(&self, db: DB) -> Evm<'_, Self::DefaultExternalContext<'_>, DB> {
        EvmBuilder::default()
            .with_db(db)
            // add additional precompiles
            .append_handler_register(Self::set_precompiles)
            .build()
    }

    fn default_external_context<'a>(&self) -> Self::DefaultExternalContext<'a> {}
}

impl ConfigureEvmEnv for CustomEvmConfig {
    type Header = Header;

    fn fill_tx_env(&self, tx_env: &mut TxEnv, transaction: &TransactionSigned, sender: Address) {
        EthEvmConfig::new(Arc::new(mainnet())).fill_tx_env(tx_env, transaction, sender)
    }

    fn fill_cfg_env(
        &self,
        cfg_env: &mut CfgEnvWithHandlerCfg,
        header: &Header,
        total_difficulty: U256,
    ) {
        EthEvmConfig::new(Arc::new(mainnet())).fill_cfg_env(cfg_env, header, total_difficulty)
    }

    fn fill_tx_env_system_contract_call(
        &self,
        env: &mut Env,
        caller: Address,
        contract: Address,
        data: Bytes,
    ) {
        EthEvmConfig::new(Arc::new(mainnet()))
            .fill_tx_env_system_contract_call(env, caller, contract, data)
    }

    fn next_cfg_and_block_env(
        &self,
        parent: &Self::Header,
        attributes: NextBlockEnvAttributes,
    ) -> (CfgEnvWithHandlerCfg, BlockEnv) {
        EthEvmConfig::new(Arc::new(mainnet())).next_cfg_and_block_env(parent, attributes)
    }
}
