#!/usr/bin/env bash
# Builds the web client and runs the server on this machine, without Docker.
# On a laptop this is much faster than a Docker build: it runs natively, uses
# every core, and after the first build compiles only what changed.
#
#   scripts/run-local.sh                 # then open http://localhost:8080/console
#   PORT=9000 scripts/run-local.sh       # another port
#   CARGO_TARGET_DIR=~/.cache/nullnet scripts/run-local.sh
#                                        # build files off a slow external drive
#
# Needs Rust (https://rustup.rs), the wasm32 target and wasm-bindgen-cli at the
# version in Cargo.lock; the script says what to install when one is missing.
# wasm-opt (`brew install binaryen`) is optional and makes the client smaller.
set -euo pipefail
cd "$(dirname "$0")/.."

port="${PORT:-8080}"
db="${NULLNET_DB:-nullnet.db}"
profile="${WEB_PROFILE:-web-lite}"

missing() {
  echo "Missing: $1" >&2
  echo "Install it with:  $2" >&2
  exit 1
}

command -v cargo >/dev/null || missing "Rust" "curl https://sh.rustup.rs -sSf | sh"
targets=$(rustup target list --installed 2>/dev/null || true)
grep -qx wasm32-unknown-unknown <<<"$targets" \
  || missing "the wasm32 target" "rustup target add wasm32-unknown-unknown"
want=$(grep -A1 '^name = "wasm-bindgen"$' Cargo.lock | sed -n 's/^version = "\(.*\)"/\1/p')
have=$(wasm-bindgen --version 2>/dev/null | awk '{print $2}' || true)
[ "$have" = "$want" ] \
  || missing "wasm-bindgen-cli $want (found: ${have:-none})" "cargo install wasm-bindgen-cli --version $want"

scripts/build-web.sh "$profile"
echo "Starting the server: http://localhost:$port/console (Ctrl+C stops it)"
exec cargo run --release -p nullnet-server -- --db "$db" --port "$port" --web dist
