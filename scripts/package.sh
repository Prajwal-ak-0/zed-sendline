#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
./scripts/build.sh
python3 scripts/install.py --package-only
