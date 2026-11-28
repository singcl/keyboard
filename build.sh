#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"

cargo build --release
cp -f target/release/keyboard-plus.exe keyboard-plus.exe

cargo build --release --no-default-features
cp -f target/release/keyboard-plus.exe keyboard-plus-slim.exe

# 恢复 target/ 里的标准版，方便后续调试
cargo build --release >/dev/null

echo "[build] ok: $(pwd)/keyboard-plus.exe"
echo "[build] ok: $(pwd)/keyboard-plus-slim.exe"
