#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"

CGO_ENABLED=0 go build -ldflags "-w -s -H windowsgui" -o keyboard-plus.exe ./cmd/keyboard
CGO_ENABLED=0 go build -tags noupdate -ldflags "-w -s -H windowsgui" -o keyboard-plus-slim.exe ./cmd/keyboard

echo "[build] ok: $(pwd)/keyboard-plus.exe"
echo "[build] ok: $(pwd)/keyboard-plus-slim.exe"
