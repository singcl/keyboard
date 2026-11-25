#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"

CGO_ENABLED=0 go build -ldflags "-w -s -H windowsgui" -o keyboard.exe ./cmd/keyboard

echo "[build] ok: $(pwd)/keyboard.exe"
