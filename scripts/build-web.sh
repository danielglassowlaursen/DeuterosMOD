#!/usr/bin/env bash
# Builds the browser client into dist/.
#
# Requires: rustup target add wasm32-unknown-unknown
#           cargo install wasm-bindgen-cli --version <version of wasm-bindgen in Cargo.lock>
# Optional: wasm-opt (binaryen) for a smaller download.
set -euo pipefail
cd "$(dirname "$0")/.."

profile="${1:-web}"
cargo build -p nullnet-client --target wasm32-unknown-unknown --profile "$profile"

target_dir="target/wasm32-unknown-unknown/$profile"
[ "$profile" = dev ] && target_dir="target/wasm32-unknown-unknown/debug"

rm -rf dist
mkdir -p dist
wasm-bindgen --target web --no-typescript --out-dir dist --out-name nullnet-client \
  "$target_dir/nullnet-client.wasm"
if command -v wasm-opt >/dev/null; then
  # The features rustc enables by default for wasm32-unknown-unknown. An old
  # binaryen that cannot read the module just leaves it unoptimised.
  wasm-opt -Os --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext \
    --enable-reference-types --enable-multivalue --enable-mutable-globals \
    -o dist/nullnet-client_bg.opt.wasm dist/nullnet-client_bg.wasm \
    && mv dist/nullnet-client_bg.opt.wasm dist/nullnet-client_bg.wasm \
    || echo "wasm-opt failed; keeping the unoptimised module"
fi
cp web/index.html dist/
# The narrator's clips, one per page of the story.
cp -r web/voice dist/
echo "Built dist/ ($(du -h dist/nullnet-client_bg.wasm | cut -f1) wasm). Serve it with e.g.: python3 -m http.server -d dist 8080"
