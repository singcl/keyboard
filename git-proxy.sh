#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"

DEFAULT_PROXY="http://127.0.0.1:10808"
ACTION="${1:-set}"
PROXY="${2:-$DEFAULT_PROXY}"

case "$ACTION" in
    set|on)
        git config --local http.proxy "$PROXY"
        git config --local https.proxy "$PROXY"

        echo "[git-proxy] configured:"
        echo "  http.proxy  $PROXY"
        echo "  https.proxy $PROXY"
        ;;
    unset|off|remove)
        git config --local --unset-all http.proxy 2>/dev/null || true
        git config --local --unset-all https.proxy 2>/dev/null || true

        echo "[git-proxy] proxy configuration removed for this repository"
        ;;
    *)
        echo "Usage:"
        echo "  ./git-proxy.sh set [proxy]       Configure the repository proxy"
        echo "  ./git-proxy.sh unset             Remove the repository proxy"
        echo
        echo "Default proxy: $DEFAULT_PROXY"
        exit 2
        ;;
esac
