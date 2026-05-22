#!/usr/bin/env bash
# 本机开发：每日 run-daily（需已配置 config.toml）
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT"

CONFIG="${CONFIG:-$ROOT/config.toml}"
export RUST_LOG="${RUST_LOG:-marketfeed=info}"

echo "==> run-daily (config: $CONFIG)"
exec cargo run --release --quiet -- run-daily --config "$CONFIG"
