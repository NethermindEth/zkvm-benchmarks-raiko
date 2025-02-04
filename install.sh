#!/usr/bin/env sh

# Exit immediately if a command exits with a non-zero status.
set -e

# Helper function to print an error message and exit.
error_exit() {
    echo "Error: $1 failed with exit code $?. Exiting."
    exit 1
}

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y || error_exit "Installing Rust"
source $HOME/.cargo/env

curl -L https://risczero.com/install | bash || error_exit "Installing Risc Zero toolchain"
export PATH="$PATH:$HOME/.risc0/bin"
rzup install || error_exit "Updating Risc Zero toolchain"
cargo risczero --version || error_exit "Checking cargo risczero version"

# Install the Succinct toolchain
curl -L https://sp1.succinct.xyz | bash || error_exit "Installing Succinct toolchain"
export PATH="$PATH:$HOME/.sp1/bin"
sp1up || error_exit "Updating Succinct toolchain"
cargo prove --version || error_exit "Checking cargo prove version"

# Install Lita toolchain
wget https://github.com/lita-xyz/llvm-valida-releases/releases/download/v0.7.0-alpha/llvm-valida-v0.7.0-alpha-linux-x86_64.tar.xz || error_exit "Downloading lita toolchain"
tar xf llvm-valida-v0.7.0-alpha-linux-x86_64.tar.xz || error_exit "Extracting lita toolchain"
cp install_lita_toolchain.sh valida-toolchain/install.sh
cd valida-toolchain
./install.sh || error_exit "Installing lita toolchain"
cd ../
rm llvm-valida-v0.7.0-alpha-linux-x86_64.tar.xz
rm -rf valida-toolchain

echo "All installations completed successfully."
