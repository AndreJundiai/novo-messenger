#!/usr/bin/env sh
# Compila o núcleo Rust para WebAssembly e coloca em web/.
set -eu
cd "$(dirname "$0")"
rustup target add wasm32-unknown-unknown >/dev/null 2>&1 || true
cargo build --profile wasm --target wasm32-unknown-unknown -p nm-wasm
cp target/wasm32-unknown-unknown/wasm/nm_wasm.wasm web/nm_core.wasm
ls -l web/nm_core.wasm
