#!/bin/bash
set -e

echo "Building for wasm32-unknown-unknown..."
cargo build --target wasm32-unknown-unknown

echo "Wasm files copied to iris-web/pkg/"
