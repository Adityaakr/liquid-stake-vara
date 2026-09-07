#!/usr/bin/env bash
# Regenerate the frontend's IDL snapshots and TypeScript clients from the built programs.
# Usage: scripts/gen-clients.sh   (after `cargo build --release` in programs/demo-token, programs/vault and programs/vara-pool)
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p src/chain/idl
cp programs/demo-token/target/wasm32-gear/release/demo_token.idl src/chain/idl/demo_token.idl
cp programs/vault/target/wasm32-gear/release/vault.idl src/chain/idl/vault.idl
cp programs/vara-pool/target/wasm32-gear/release/vara_pool.idl src/chain/idl/vara_pool.idl
cargo sails client-js src/chain/idl/demo_token.idl src/chain/idl/demo_token.ts >/dev/null
cargo sails client-js src/chain/idl/vault.idl src/chain/idl/vault.ts >/dev/null
cargo sails client-js src/chain/idl/vara_pool.idl src/chain/idl/vara_pool.ts >/dev/null
# sails-cli 1.0.1 emits package names that do not exist on npm; sails-js 1.0.0 exposes the same
# symbols under subpath exports.
for f in src/chain/idl/demo_token.ts src/chain/idl/vault.ts src/chain/idl/vara_pool.ts; do
  sed -i '' -e 's|from "sails-js-parser-idl-v2"|from "sails-js/parser"|' -e 's|from "sails-js-types"|from "sails-js/types"|' "$f"
  node scripts/tidy-client.mjs "$f"
done
echo "clients regenerated"
