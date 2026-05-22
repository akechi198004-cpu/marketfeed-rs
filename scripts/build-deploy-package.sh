#!/usr/bin/env bash
# 生成 marketfeed-deploy.tar.gz（调用 build-opc.sh 编译 + 打包配置与脚本）
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

DEPLOY_DIR="$ROOT/marketfeed-deploy"
ARCHIVE="$ROOT/marketfeed-deploy.tar.gz"

"$ROOT/scripts/build-opc.sh"

mkdir -p "$DEPLOY_DIR/reports"
cp -f "$ROOT/config.example.toml" "$DEPLOY_DIR/config.toml"
cp -f "$ROOT/scripts/deploy-run-daily.sh" "$DEPLOY_DIR/run-daily.sh"
cp -f "$ROOT/scripts/deploy-README.txt" "$DEPLOY_DIR/README.txt"
chmod +x "$DEPLOY_DIR/run-daily.sh"

tar -czf "$ARCHIVE" -C "$ROOT" marketfeed-deploy
ls -lh "$ARCHIVE"
echo "完成: $ARCHIVE"
echo "注意: 包内 config.toml 为示例，部署到远程后请填写密钥并勿覆盖已有配置"
