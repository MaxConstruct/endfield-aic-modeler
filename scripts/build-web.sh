#!/usr/bin/env bash
# Builds the web version into target/web/: index.html, the .wasm and its JS glue, and a copy of
# data/, which the app fetches at startup. Serve that folder from any static web server.
#
#   scripts/build-web.sh          # release build into target/web/
#   scripts/build-web.sh --serve  # debug build, served at http://127.0.0.1:8080 with live reload
#
# Needs `rustup target add wasm32-unknown-unknown`, `cargo install --locked trunk` and
# `cargo install --locked wasm-bindgen-cli --version 0.2.130` (the version in Cargo.lock).
set -euo pipefail

cd "$(dirname "$0")/../crates/aic-app"

case "${1:-}" in
    --serve) exec trunk serve ;;
    "") exec trunk build --release ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
esac
