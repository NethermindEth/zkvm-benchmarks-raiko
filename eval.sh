#!/bin/bash
set -e
echo "Running $1, $2, $3, $4, $5"

# Get program directory name as $1 and append "-$2" to it if $1 is "tendermint"
# or "reth"
if [ "$1" = "tendermint" ] || [ "$1" = "reth" ]; then
    program_directory="${1}-$2"
else
    program_directory="$1"
fi

echo "Building program"

# cd to program directory computed above
cd "benchmarks/$program_directory"

# If the prover is risc0, then build the program.
if [ "$2" == "risc0" ]; then
    echo "Building Risc0"
    # Use the risc0 toolchain.
    CC=gcc CC_riscv32im_risc0_zkvm_elf=~/.risc0/cpp/bin/riscv32-unknown-elf-gcc\
      RUSTFLAGS="-C passes=loweratomic -C link-arg=-Ttext=0x00200800 -C panic=abort"\
      RISC0_FEATURE_bigint2=1\
      RUSTUP_TOOLCHAIN=risc0 \
      CARGO_BUILD_TARGET=riscv32im-risc0-zkvm-elf \
      cargo build --release --ignore-rust-version --features $2

fi
# If the prover is sp1, then build the program.
if [ "$2" == "sp1" ]; then
    # The reason we don't just use `cargo prove build` from the SP1 CLI is we need to pass a --features ...
    # flag to select between sp1 and risc0.
    RUSTFLAGS="-C passes=lower-atomic -C link-arg=-Ttext=0x00200800 -C panic=abort" \
        RUSTUP_TOOLCHAIN=succinct \
        CARGO_BUILD_TARGET=riscv32im-succinct-zkvm-elf \
        cargo build --release --ignore-rust-version --features $2
fi

cd ../../

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
  FEATURES="$2, cuda"
else
  FEATURES="$2"
fi

# Run the benchmark.
RISC0_INFO=1 \
    cargo run \
    -p zkvm-benchmarks-eval \
    --release \
    --no-default-features \
    --features "$FEATURES" \
    -- \
    --program "$1" \
    --prover "$2" \
    --shard-size "$3" \
    --filename "$4" \
     ${5:+$([[ "$1" == "fibonacci" ]] && echo "--fibonacci-input" || echo "--block-number") $5}
    # --hashfn "$3" \
    # --shard-size "$4" \
    # --filename "$5" \
    #  ${6:+$([[ "$1" == "fibonacci" ]] && echo "--fibonacci-input" || echo "--block-number") $6}

  exit $?
