#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
rustup target add wasm32-wasip2
cargo build --locked --release -p sendline
cargo build --locked --release -p sendline-extension --target wasm32-wasip2
python3 scripts/generate_tasks.py
mkdir -p bin
# Replace the inode so an already running server keeps its executable intact.
cp target/release/sendline bin/.sendline.new
mv bin/.sendline.new bin/sendline
cp target/wasm32-wasip2/release/sendline_extension.wasm extension.wasm
printf '\nBuilt Sendline. Install with:\npython3 scripts/install.py --enable-buttons\n'
