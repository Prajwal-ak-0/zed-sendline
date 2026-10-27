#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
./scripts/build.sh
revision=db8b4398de90b6d0b6c780aba96aaa2cd8e9202c
if [ ! -d grammars/http/.git ]; then
  git clone https://github.com/rest-nvim/tree-sitter-http.git grammars/http
fi
git -C grammars/http checkout "$revision"
mkdir -p .build
if [ -z "${WASI_SDK_PATH:-}" ]; then
  case "$(uname -s)-$(uname -m)" in
    Darwin-arm64) platform=arm64-macos ;;
    Darwin-x86_64) platform=x86_64-macos ;;
    Linux-aarch64) platform=arm64-linux ;;
    Linux-x86_64) platform=x86_64-linux ;;
    *) printf 'Set WASI_SDK_PATH to your WASI SDK directory.\n' >&2; exit 1 ;;
  esac
  WASI_SDK_PATH="$PWD/.build/wasi-sdk-34.0-$platform"
  if [ ! -x "$WASI_SDK_PATH/bin/clang" ]; then
    curl --connect-timeout 15 --max-time 300 --fail --location --silent --show-error "https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-34/wasi-sdk-34.0-$platform.tar.gz" -o .build/wasi-sdk.tar.gz
    tar -xzf .build/wasi-sdk.tar.gz -C .build
  fi
fi
"$WASI_SDK_PATH/bin/clang" -fPIC -shared -Os -Wl,--export=tree_sitter_http -o grammars/http.wasm -I grammars/http/src grammars/http/src/parser.c
python3 scripts/install.py --package-only
