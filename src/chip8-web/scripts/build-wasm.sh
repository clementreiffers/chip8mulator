#!/usr/bin/env sh
set -eu
cd "$(dirname "$0")/.."
mkdir -p wasm-pkg public/wasm
rm -f wasm-pkg/chip8_engine.js wasm-pkg/chip8_engine_bg.wasm wasm-pkg/chip8_engine.d.ts
rm -f public/wasm/chip8_engine.js public/wasm/chip8_engine_bg.wasm
cargo build --manifest-path ../chip8-engine/Cargo.toml --target wasm32-unknown-unknown --features wasm --release
wasm-bindgen ../chip8-engine/target/wasm32-unknown-unknown/release/chip8_engine.wasm --target web --out-dir wasm-pkg
if [ "${CHIP8_WASM_SOURCE:-local}" = "local" ]; then
  mkdir -p public/wasm
  cp wasm-pkg/chip8_engine.js wasm-pkg/chip8_engine_bg.wasm public/wasm/
fi
