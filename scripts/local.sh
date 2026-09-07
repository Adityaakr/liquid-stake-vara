#!/usr/bin/env bash
# One command local end-to-end environment:
#   scripts/local.sh            start a dev node, deploy seeded programs, write .env.local, start the app
#   scripts/local.sh stop       stop the node and the app
# Needs the programs built (see README) and the gear binary (default ~/.local/bin/gear, override with GEAR_BIN).
set -euo pipefail
cd "$(dirname "$0")/.."
GEAR_BIN="${GEAR_BIN:-$HOME/.local/bin/gear}"
RPC=ws://127.0.0.1:9944
LOG_DIR=.local
mkdir -p "$LOG_DIR"

if [ "${1:-}" = "stop" ]; then
  pkill -f "gear --dev --tmp --rpc-port 9944" 2>/dev/null && echo "node stopped" || echo "node was not running"
  pkill -f "vite.js" 2>/dev/null && echo "app stopped" || true
  exit 0
fi

if ! pgrep -f "gear --dev --tmp --rpc-port 9944" >/dev/null; then
  echo "starting local gear node (log: $LOG_DIR/node.log)"
  nohup "$GEAR_BIN" --dev --tmp --rpc-port 9944 --rpc-cors all > "$LOG_DIR/node.log" 2>&1 &
  for _ in $(seq 1 30); do
    if curl -s -H 'Content-Type: application/json' -d '{"id":1,"jsonrpc":"2.0","method":"system_health","params":[]}' http://127.0.0.1:9944 | grep -q result; then break; fi
    sleep 1
  done
else
  echo "local node already running"
fi

echo "deploying seeded programs (unbond ${UNBOND:-120}s, vesting ${VESTING:-604800}s, faucet cooldown ${COOLDOWN:-60}s, seed ${SEED:-250000} tokens per vault, ${STAKE:-918270000} VARA in the kVARA pool, rewards sized for the published APYs)"
DEPLOYER_SEED='//Alice' node node_modules/tsx/dist/cli.mjs --tsconfig tsconfig.app.json scripts/deploy.ts \
  --rpc "$RPC" --unbond "${UNBOND:-120}" --vesting "${VESTING:-604800}" --cooldown "${COOLDOWN:-60}" --fund 10 --seed "${SEED:-250000}" \
  --stake "${STAKE:-918270000}" ${REWARDS:+--rewards "$REWARDS"} --env .env.local | tail -20

echo
echo "starting the app on http://localhost:5173/app/vaults (log: $LOG_DIR/app.log)"
pkill -f "vite.js" 2>/dev/null || true   # an older dev server would keep stale env values
nohup node node_modules/vite/bin/vite.js --port 5173 --strictPort > "$LOG_DIR/app.log" 2>&1 &
sleep 2
echo "ready. Connect a Polkadot.js or SubWallet account, press 'Get 100 VARA for fees', then stake VARA or use the faucet and deposit."
