#!/usr/bin/env bash
# Compila o núcleo Rust para Android e gera as ligações Kotlin (UniFFI).
# Precisa de: Rust, cargo-ndk (cargo install cargo-ndk) e o NDK do Android (ANDROID_NDK_HOME).
set -euo pipefail
cd "$(dirname "$0")/.."
rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android >/dev/null
cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 -o android/app/src/main/jniLibs build --release -p nm-ffi
cargo build -p nm-ffi
cargo run -p uniffi-bindgen -- generate --library target/debug/libnm_ffi.so --language kotlin --out-dir android/app/src/main/java
ls -R android/app/src/main/jniLibs
