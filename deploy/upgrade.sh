#!/usr/bin/env bash
# Relay 在线升级:从 GitHub Releases 拉取最新(或指定)版本的发布包,原地升级本机安装。
# 用法:sudo ./upgrade.sh [版本号]        # 缺省 = 最新 Release;版本号如 0.3.0 或 v0.3.0
# 可用环境变量:INSTALL_DIR=/opt/relay   # 安装目录(与 install.sh 一致)
#              REPO=stoneaigc/Relay    # GitHub 仓库(自 fork 时覆盖)
#              HEALTH_URL=http://127.0.0.1:8080/healthz  # 升级后健康检查地址
# 流程:查询版本 → 下载校验 → 备份当前二进制 → 调 install.sh 原地升级(保留配置与数据) → 健康检查
set -euo pipefail

INSTALL_DIR="${INSTALL_DIR:-/opt/relay}"
REPO="${REPO:-stoneaigc/Relay}"
HEALTH_URL="${HEALTH_URL:-http://127.0.0.1:8080/healthz}"
VER="${1:-}"

if [ "$(id -u)" -ne 0 ]; then
  echo "需要 root 权限。请用 sudo 运行:sudo ./upgrade.sh" >&2
  exit 1
fi

command -v curl >/dev/null 2>&1 || { echo "缺少 curl,请先安装:apt install curl / yum install curl" >&2; exit 1; }

# 目标版本:参数 > 最新 Release
if [ -n "$VER" ]; then
  VER="${VER#v}"
else
  echo "==> 查询最新版本..."
  VER="$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
    | grep -m1 '"tag_name"' | sed 's/.*"tag_name": *"v\{0,1\}\([^"]*\)".*/\1/')" || true
  if [ -z "${VER:-}" ]; then
    echo "无法获取最新版本(离线或 GitHub 不可达)。可手动下载发布包后执行:sudo ./install.sh" >&2
    exit 1
  fi
fi
echo "==> 目标版本: v$VER"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# 当前版本比对(从运行中服务的 /healthz 读取;服务未运行则直接升级)
CUR="$(curl -fsS "$HEALTH_URL" 2>/dev/null | grep -o '"version":"[^"]*"' | cut -d'"' -f4 || true)"
if [ -n "${CUR:-}" ] && [ "v${CUR#v}" = "v$VER" ]; then
  echo "==> 当前已是 v$VER,无需升级。"
  exit 0
fi

# 架构 → CI 包名(relay-v<ver>-linux-<arch>)
ARCH="$(uname -m)"
case "$ARCH" in
  x86_64) A="x86_64" ;;
  aarch64 | arm64) A="arm64" ;;
  *) echo "不支持的架构:$ARCH" >&2; exit 1 ;;
esac
PKG="relay-v${VER}-linux-${A}"
URL="https://github.com/$REPO/releases/download/v${VER}/${PKG}.tar.gz"

echo "==> 下载 $URL"
curl -fSL --retry 3 -o "$TMP/$PKG.tar.gz" "$URL"
echo "==> 解压"
tar -xzf "$TMP/$PKG.tar.gz" -C "$TMP"
[ -x "$TMP/$PKG/install.sh" ] || { echo "发布包缺少 install.sh,中止。" >&2; exit 1; }

# 升级前备份当前二进制(可随时 mv 回去回滚)
if [ -f "$INSTALL_DIR/relay" ]; then
  BAK="$INSTALL_DIR/relay.bak.$(date +%Y%m%d%H%M%S)"
  cp "$INSTALL_DIR/relay" "$BAK"
  echo "==> 已备份当前二进制:$BAK"
fi

echo "==> 执行安装/升级(保留 config 与 relay.env)"
(cd "$TMP/$PKG" && ./install.sh)

echo "==> 健康检查:$HEALTH_URL"
sleep 2
if curl -fsS "$HEALTH_URL" 2>/dev/null | grep -q '"status":"ok"'; then
  echo
  echo "==> 升级完成:$(curl -fsS "$HEALTH_URL")"
else
  echo "警告:健康检查未通过,请查看日志:journalctl -u relay -n 50" >&2
  echo "回滚方法:cp $INSTALL_DIR/relay.bak.* $INSTALL_DIR/relay && systemctl restart relay" >&2
  exit 1
fi
