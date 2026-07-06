#!/usr/bin/env bash
# Serve the demo: http://127.0.0.1:5189/wasm/
cd "$(dirname "$0")/../site"
exec python3 -m http.server "${1:-5189}" --bind 127.0.0.1
