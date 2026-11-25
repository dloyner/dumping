#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(
  cd "$(dirname "$0")"
  pwd -P
)"
cd "$SCRIPT_DIR/.."

cargo build -p compute-unit --release --target wasm32-wasip1

if [ -f ./client/compute-unit.wasm ]; then
  echo "removing computing unit in worker"
  rm ./client/compute_unit.wasm
fi

cp target/wasm32-wasip1/release/compute_unit.wasm \
  ./client/compute_unit.wasm
