#!/usr/bin/env bash
# 每日行情更新：拉数 → 信号 → 报告
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT"

CONFIG="${CONFIG:-$ROOT/config.toml}"
BIN="$ROOT/target/release/marketfeed"

export RUST_LOG="${RUST_LOG:-marketfeed=info}"

# 定时任务可设 SKIP_BUILD=1 跳过编译，加快执行
if [[ "${SKIP_BUILD:-0}" != "1" ]] || [[ ! -x "$BIN" ]]; then
  echo "==> 编译 release ..."
  cargo build --release --quiet
else
  echo "==> 使用已有二进制: $BIN"
fi

echo "==> 运行 run-daily (config: $CONFIG)"
exec "$BIN" run-daily --config "$CONFIG"
