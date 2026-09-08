#!/usr/bin/env bash
# Relay 发布包构建:源码 → relay-<版本>-<架构>.tar.gz(二进制 + 前端产物 + config + 安装脚本)。
# 在 Linux 服务器(或 CI)的仓库根目录执行:./deploy/build.sh
# 产物:deploy/dist/relay-<version>-<arch>.tar.gz
# 部署:上传解包后 sudo ./install.sh(默认装到 /opt/relay,详见 README「部署」)。
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

VERSION="$(grep -m1 '^version' Cargo.toml | sed 's/.*"\(.*\)".*/\1/')"
ARCH="$(uname -m)" # x86_64 / aarch64(命名与 CI release 包对齐:relay-<版本>-linux-<架构>)
OUT="$ROOT/deploy/dist"
PKG="relay-${VERSION}-linux-${ARCH}"
STAGE="$OUT/$PKG"

echo "==> [1/4] 构建 admin / portal 前端"
(cd frontend/admin  && npm ci --no-audit --no-fund && npm run build)
(cd frontend/portal && npm ci --no-audit --no-fund && npm run build)

echo "==> [2/4] cargo build --release"
cargo build --release --bin relay

echo "==> [3/4] 组装发布包 $PKG"
rm -rf "$STAGE"
mkdir -p "$STAGE/config" "$STAGE/frontend/admin" "$STAGE/frontend/portal"
cp target/release/relay "$STAGE/relay"
cp -r frontend/admin/dist  "$STAGE/frontend/admin/dist"
cp -r frontend/portal/dist "$STAGE/frontend/portal/dist"
cp config/default.toml "$STAGE/config/"
cp deploy/install.sh deploy/relay.service "$STAGE/"
cp README.md "$STAGE/"

echo "==> [4/4] 打包 tar.gz"
mkdir -p "$OUT"
tar -czf "$OUT/$PKG.tar.gz" -C "$OUT" "$PKG"
rm -rf "$STAGE"

echo
echo "==> 完成:$OUT/$PKG.tar.gz"
echo "    上传到目标服务器后:"
echo "      tar xzf $PKG.tar.gz && cd $PKG"
echo "      sudo ./install.sh          # 安装/升级(默认 /opt/relay,可用 INSTALL_DIR 覆盖)"
