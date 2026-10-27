#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
case "${1:-}" in ''|--integration) ;; *) printf 'Usage: scripts/check.sh [--integration]\n' >&2; exit 2 ;; esac
python3 scripts/preflight.py
cargo fmt --all -- --check
cargo clippy --locked -p sendline --all-targets -- -D warnings
cargo test --locked -p sendline
./scripts/build.sh
cargo clippy --locked -p sendline-extension --target wasm32-wasip2 -- -D warnings
if [ "${1:-}" = --integration ]; then python3 tests/integration.py; fi
