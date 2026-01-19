#!/bin/bash

echo "Building for wasm32-unknown-unknown..."
cargo build --release --target wasm32-unknown-unknown

echo "Wasm files copied to iris-web/pkg/"
