#!/bin/bash
set -e
echo "Running $1, $2, $3, $4, $5, $6"

PROGRAM=$1;
PROVER=$2;
SHARD_SIZE=$3;
FILENAME=$4;
ADDED_ARGS=$5;
BLOCKS_DIR_SUFFIX=$6;

# Function to check rust version and determine correct parameter name
check_rust_version() {
    local toolchain=$PROGRAM
    local version_output

    if [ -z "$toolchain" ]; then
        version_output=$(rustc --version)
    else
        version_output=$(rustc +$toolchain --version)
    fi

    # Extract version number
    local version=$(echo "$version_output" | sed -E 's/rustc ([0-9]+\.[0-9]+\.[0-9]+).*/\1/')
    local major=$(echo "$version" | cut -d. -f1)
    local minor=$(echo "$version" | cut -d. -f2)

    # Compare version with 1.81
    if [ "$major" -gt 1 ] || ([ "$major" -eq 1 ] && [ "$minor" -gt 81 ]); then
        echo "lower-atomic"  # New parameter name for Rust >= 1.81
    else
        echo "loweratomic"   # Old parameter name for Rust < 1.81
    fi
}

# If $PROVER == jolt, append precompiles to Cargo.toml
if [ "$PROVER" = "jolt" ]; then
    cp Cargo.toml Cargo.toml.bak
    cat patches/jolt.txt >> Cargo.toml
fi

echo "Building program"

if [ "$PROGRAM" == "raiko" ]; then
    echo "Building Raiko for prover $PROVER"

    # Values from Raiko build script
    TOOLCHAIN_RISC0=nightly-2024-09-05
    TOOLCHAIN_SP1=nightly-2024-09-05

    # Run a builder inherited from Raiko itself
    if [ "$PROVER" == "sp1" ]; then
        RUSTUP_TOOLCHAIN=$TOOLCHAIN_SP1 \
            cargo run --bin raiko-sp1-builder
    elif [ "$PROVER" == "risc0" ]; then
        RUSTUP_TOOLCHAIN=$TOOLCHAIN_RISC0 \
            cargo run --bin raiko-risc0-builder
    else
        echo "Prover $PROVER is not supported for Raiko benchmark!"
        exit
    fi
else
    # Get program directory name as $PROGRAM and append "-$PROVER" to it if $PROGRAM is "tendermint"
    # or "reth"
    if [ "$PROGRAM" = "tendermint" ] || [ "$PROGRAM" = "reth" ]; then
        program_directory="${1}-$PROVER"
    else
        program_directory="$PROGRAM"
    fi

    # cd to program directory computed above
    cd "benchmarks/$program_directory"

    # If the prover is risc0, then build the program.
    if [ "$PROVER" == "risc0" ]; then
        echo "Building Risc0"
        # Use the risc0 toolchain.
        ATOMIC_PARAM=$(check_rust_version "risc0")
        CC=gcc CC_riscv32im_risc0_zkvm_elf=~/.risc0/cpp/bin/riscv32-unknown-elf-gcc\
          RUSTFLAGS="-C passes=$ATOMIC_PARAM -C link-arg=-Ttext=0x00200800 -C panic=abort"\
          RISC0_FEATURE_bigint2=1\
          RUSTUP_TOOLCHAIN=risc0 \
          CARGO_BUILD_TARGET=riscv32im-risc0-zkvm-elf \
          cargo build --release --ignore-rust-version --features $PROVER

    fi

    # If the prover is sp1, then build the program.
    if [ "$PROVER" == "sp1" ]; then
        # The reason we don't just use `cargo prove build` from the SP1 CLI is we need to pass a --features ...
        # flag to select between sp1 and risc0.
        ATOMIC_PARAM=$(check_rust_version "succinct")
        RUSTFLAGS="-C passes=$ATOMIC_PARAM -C link-arg=-Ttext=0x00200800 -C panic=abort" \
            RUSTUP_TOOLCHAIN=succinct \
            CARGO_BUILD_TARGET=riscv32im-succinct-zkvm-elf \
            cargo build --release --ignore-rust-version --features $PROVER
    fi

    if [ "$PROVER" == "lita" ]; then
      echo "Building Lita"
      # Use the lita toolchain.
      CC_valida_unknown_baremetal_gnu="/valida-toolchain/bin/clang" \
        CFLAGS_valida_unknown_baremetal_gnu="--sysroot=/valida-toolchain -isystem /valida-toolchain/include" \
        RUSTUP_TOOLCHAIN=valida \
        CARGO_BUILD_TARGET=valida-unknown-baremetal-gnu \
        cargo build --release --ignore-rust-version --features $PROVER

      # Lita does not have any hardware acceleration. Also it does not have an SDK
      # or a crate to be used on rust. We need to benchmark it without rust
      cd ../../
      ./eval_lita.sh $PROGRAM $PROVER $SHARD_SIZE $program_directory $6
      exit
    fi

    if [ "$PROVER" == "nexus" ]; then
      echo "Building Nexus"
      # Hardcode the memlimit to 8 MB
      RUSTFLAGS="-C link-arg=--defsym=MEMORY_LIMIT=0x80000 -C link-arg=-T../../nova.x" \
        CARGO_BUILD_TARGET=riscv32i-unknown-none-elf \
        RUSTUP_TOOLCHAIN=1.77.0 \
        cargo build --release --ignore-rust-version --features $PROVER
    fi

    cd ../../
fi

echo "Running eval script"


# Check for AVX-512 support
if lscpu | grep -q avx512; then
  # If AVX-512 is supported, add the specific features to RUSTFLAGS
  export RUSTFLAGS="-C target-cpu=native -C target-feature=+avx512ifma,+avx512vl"
else
  # If AVX-512 is not supported, just set target-cpu=native
  export RUSTFLAGS="-C target-cpu=native"
fi

# Set the logging level.
export RUST_LOG=info

# Detect whether we're on an instance with a GPU.
if nvidia-smi > /dev/null 2>&1; then
  FEATURES="$PROVER, cuda"
  if [ "$2" = "jolt" ]; then
    export ICICLE_BACKEND_INSTALL_DIR=$(pwd)/target/release/deps/icicle/lib/backend
  fi
else
  FEATURES="$PROVER"
fi

if [ "$PROVER" = "jolt" ]; then
  export RUSTFLAGS=""
  export RUSTUP_TOOLCHAIN="nightly-2024-09-30"
fi

set -x # echo on

# Run the benchmark.
RISC0_INFO=1 \
  RUST_BACKTRACE=1 \
    cargo run \
    -p zkvm-benchmarks-eval \
    --release \
    --no-default-features \
    --features "$FEATURES" \
    -- \
    --program "$PROGRAM" \
    --prover "$PROVER" \
    --shard-size "$SHARD_SIZE" \
    --filename "$FILENAME" \
    ${ADDED_ARGS:+$(
      [[ "$PROGRAM" == "fibonacci" ]] && echo "--fibonacci-input" || echo "--block-name"
    ) $ADDED_ARGS} \
    --taiko-blocks-dir-suffix "$BLOCKS_DIR_SUFFIX"
    # --hashfn "$HASHFN"

# Revert Cargo.toml as the last step
if [ "$PROVER" = "jolt" ]; then
    mv Cargo.toml.bak Cargo.toml
fi

exit $?
