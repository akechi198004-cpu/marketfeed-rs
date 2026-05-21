#!/usr/bin/env bash
# 远程部署版：与 marketfeed 二进制同目录，无需 Rust/cargo
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT"

CONFIG="${CONFIG:-$ROOT/config.toml}"
BIN="$ROOT/marketfeed"

export RUST_LOG="${RUST_LOG:-marketfeed=info}"

if [[ ! -x "$BIN" ]]; then
  echo "错误: 未找到可执行文件 $BIN" >&2
  exit 1
fi

echo "==> run-daily (config: $CONFIG)"
exec "$BIN" run-daily --config "$CONFIG"
