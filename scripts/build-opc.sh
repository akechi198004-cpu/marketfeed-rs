#!/usr/bin/env bash
# 本地 musl 静态编译，输出到 marketfeed-deploy/marketfeed，可直接 scp 到 opc 运行
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

BINARY="$ROOT/marketfeed-deploy/marketfeed"
MUSL_BIN="$ROOT/target/x86_64-unknown-linux-musl/release/marketfeed"

if ! command -v musl-gcc >/dev/null 2>&1; then
  echo "错误: 未找到 musl-gcc，请安装: sudo apt install musl-tools" >&2
  exit 1
fi

echo "==> musl 静态编译 (opc / 旧 glibc) ..."
rustup target add x86_64-unknown-linux-musl >/dev/null 2>&1 || true
cargo build --release --target x86_64-unknown-linux-musl -q

mkdir -p "$ROOT/marketfeed-deploy"
cp -f "$MUSL_BIN" "$BINARY"
chmod +x "$BINARY"

echo ""
echo "==> 本地产物（复制这个，不要用 target/release/marketfeed）"
echo "    $BINARY"
echo ""
file "$BINARY"
if ldd "$BINARY" 2>/dev/null | grep -q 'libc\.so'; then
  echo "错误: 仍依赖 glibc" >&2
  exit 1
fi
ldd "$BINARY" 2>&1 || true
sha256sum "$BINARY"
echo ""
echo "==> 上传到 opc（改主机名）"
echo "    scp $BINARY opc@你的主机:~/marketfeed-deploy/"
echo "    ssh opc@你的主机 'chmod +x ~/marketfeed-deploy/marketfeed && file ~/marketfeed-deploy/marketfeed && ldd ~/marketfeed-deploy/marketfeed'"
