#!/usr/bin/env sh
set -eu
cd "$(dirname "$0")/.."
cargo build --manifest-path ../chip8-engine/Cargo.toml --target wasm32-unknown-unknown --features wasm --release
wasm-bindgen ../chip8-engine/target/wasm32-unknown-unknown/release/chip8_engine.wasm --target web --out-dir wasm-pkg
