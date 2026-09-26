#!/usr/bin/env bash
# ============================================================================
# Relay 在线安装/升级脚本(参考 1Panel 的快速安装体验)
#
# 一条命令安装:
#   curl -fsSL https://raw.githubusercontent.com/stoneaigc/Relay/main/deploy/install-online.sh -o install-online.sh && sudo bash install-online.sh
#
# 交互引导:安装目录 / 服务端口 / 管理员账号(回车即用默认值)。
# 已安装时自动进入升级模式:保留配置与数据,只替换程序。
#
# 免交互(CI/自动化):所有提问都可用环境变量跳过,例如
#   sudo NONINTERACTIVE=1 RELAY_PORT=9000 RELAY_ADMIN_PASSWORD='S3cret!' bash install-online.sh
#
# 可用环境变量:
#   RELAY_VERSION         指定版本(如 0.2.1),缺省 = 最新 Release
#   INSTALL_DIR           安装目录(默认 /opt/relay)
#   RELAY_PORT            服务端口(默认 8080)
#   RELAY_ADMIN_USERNAME  管理后台用户名(默认 admin,仅首次)
#   RELAY_ADMIN_PASSWORD  管理后台密码(缺省自动随机生成,仅首次)
#   REPO                  GitHub 仓库(默认 stoneaigc/Relay)
#   DL_PREFIX             下载加速前缀(默认 https://github.com,可换镜像)
# ============================================================================
set -euo pipefail

REPO="${REPO:-stoneaigc/Relay}"
INSTALL_DIR="${INSTALL_DIR:-/opt/relay}"
RELAY_PORT="${RELAY_PORT:-8080}"
RELAY_ADMIN_USERNAME="${RELAY_ADMIN_USERNAME:-admin}"
RELAY_ADMIN_PASSWORD="${RELAY_ADMIN_PASSWORD:-}"
RELAY_VERSION="${RELAY_VERSION:-}"
DL_PREFIX="${DL_PREFIX:-https://github.com}"
HEALTH_URL="http://127.0.0.1:${RELAY_PORT}/healthz"

# 交互模式:有终端且未显式声明 NONINTERACTIVE
if [ -t 0 ] && [ -z "${NONINTERACTIVE:-}" ]; then INTERACTIVE=1; else INTERACTIVE=0; fi

# ---- 工具函数 -------------------------------------------------------------
info()  { printf '\033[32m==> %s\033[0m\n' "$*"; }
warn()  { printf '\033[33m[警告] %s\033[0m\n' "$*"; }
abort() { printf '\033[31m[失败] %s\033[0m\n' "$*" >&2; exit 1; }

# 交互提问:ask "提示" "默认值" → 结果写到 REPLY;非交互模式直接用默认值
ask() {
  local prompt="$1" default="$2"
  if [ "$INTERACTIVE" = "1" ]; then
    read -r -p "$(printf '\033[36m%s [%s]: \033[0m' "$prompt" "$default")" REPLY || REPLY="$default"
    REPLY="${REPLY:-$default}"
  else
    REPLY="$default"
  fi
}

rand_str() { LC_ALL=C tr -dc 'A-Za-z0-9' </dev/urandom | head -c "$1" || true; }

# ---- 0. 前置检查 ----------------------------------------------------------
[ "$(id -u)" -eq 0 ] || abort "需要 root 权限,请用 sudo 运行。"
command -v curl >/dev/null 2>&1 || abort "缺少 curl,请先安装:apt install curl / yum install curl"
command -v systemctl >/dev/null 2>&1 || warn "未检测到 systemd,安装脚本将无法注册开机自启(可手动运行 $INSTALL_DIR/relay)"

ARCH="$(uname -m)"
case "$ARCH" in
  x86_64)  PKG_ARCH="x86_64" ;;
  aarch64 | arm64) PKG_ARCH="arm64" ;;
  *) abort "不支持的架构:$ARCH(仅支持 x86_64 / arm64)" ;;
esac

echo
echo "============================================================"
echo "  Relay 在线安装(参考 1Panel 快速安装体验)"
echo "  仓库: https://github.com/$REPO"
echo "  架构: $ARCH"
echo "============================================================"
echo

# ---- 1. 目标版本 ----------------------------------------------------------
if [ -n "$RELAY_VERSION" ]; then
  RELAY_VERSION="${RELAY_VERSION#v}"
else
  info "查询最新版本..."
  RELAY_VERSION="$(curl -fsSL -m 15 "https://api.github.com/repos/$REPO/releases/latest" 2>/dev/null \
    | grep -m1 '"tag_name"' | sed 's/.*"tag_name": *"v\{0,1\}\([^"]*\)".*/\1/')" || true
  [ -n "${RELAY_VERSION:-}" ] || abort "无法获取最新版本(离线或 GitHub 不可达)。可改用离线安装:手动下载发布包后执行 sudo ./install.sh"
fi
echo "==> 目标版本: v$RELAY_VERSION"

# ---- 2. 已安装检测(升级模式) ---------------------------------------------
UPGRADE=0
if [ -x "$INSTALL_DIR/relay" ]; then
  UPGRADE=1
  CUR="$(curl -fsS -m 5 "http://127.0.0.1:${RELAY_PORT}/healthz" 2>/dev/null \
    | grep -o '"version":"[^"]*"' | cut -d'"' -f4 || true)"
  echo "==> 检测到已安装:${INSTALL_DIR}/relay(当前 ${CUR:-未知版本})"
  echo "==> 将进入升级模式:保留 config / relay.env / 数据库,只替换程序文件。"
  if [ "$INTERACTIVE" = "1" ]; then
    read -r -p "确认升级到 v$RELAY_VERSION? [Y/n]: " yn || yn="Y"
    case "$yn" in [nN]*) abort "已取消。";; esac
  fi
fi

# ---- 3. 交互引导 ----------------------------------------------------------
if [ "$INTERACTIVE" = "1" ]; then
  echo
  echo "---- 安装配置(直接回车即用 [默认值]) ----"
  ask "设置安装目录" "$INSTALL_DIR";              INSTALL_DIR="$REPLY"
  ask "设置服务端口" "$RELAY_PORT";               RELAY_PORT="$REPLY"
  HEALTH_URL="http://127.0.0.1:${RELAY_PORT}/healthz"
  if [ "$UPGRADE" = "0" ]; then
    ask "设置管理后台用户名" "$RELAY_ADMIN_USERNAME"; RELAY_ADMIN_USERNAME="$REPLY"
    if [ -n "$RELAY_ADMIN_PASSWORD" ]; then
      echo "==> 使用环境变量提供的管理员密码。"
    else
      GENERATED_PWD="$(rand_str 16)"
      ask "设置管理后台密码(回车=自动生成强密码)" "$GENERATED_PWD"; RELAY_ADMIN_PASSWORD="$REPLY"
    fi
  fi
fi
HEALTH_URL="http://127.0.0.1:${RELAY_PORT}/healthz"

# ---- 4. 下载发布包 --------------------------------------------------------
PKG="relay-v${RELAY_VERSION}-linux-${PKG_ARCH}"
URL="${DL_PREFIX}/$REPO/releases/download/v${RELAY_VERSION}/${PKG}.tar.gz"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

info "下载 ${PKG}.tar.gz"
curl -fSL --retry 3 --retry-delay 2 -o "$TMP/$PKG.tar.gz" "$URL" \
  || abort "下载失败。可设置 DL_PREFIX 换镜像源,或离线安装:https://github.com/$REPO/releases"
info "解压"
tar -xzf "$TMP/$PKG.tar.gz" -C "$TMP"
[ -x "$TMP/$PKG/install.sh" ] || abort "发布包缺少 install.sh,下载不完整,请重试。"

# ---- 5. 预写敏感配置 relay.env(install.sh 不覆盖已存在文件) -------------
# 首次安装:写入端口 / 管理员凭据 / 随机 JWT 密钥(比默认 dev-secret 更安全);
# 升级安装:已存在则原样保留,绝不覆盖用户已改配置。
ENV_FILE="$INSTALL_DIR/relay.env"
if [ "$UPGRADE" = "0" ] && [ ! -f "$ENV_FILE" ]; then
  [ -n "$RELAY_ADMIN_PASSWORD" ] || RELAY_ADMIN_PASSWORD="$(rand_str 16)"
  JWT_SECRET="$(rand_str 48)"
  cat > "$ENV_FILE" <<EOF
# Relay 敏感配置(systemd EnvironmentFile 自动注入;本文件由在线安装脚本生成)
RELAY_SERVER__BIND=0.0.0.0:${RELAY_PORT}
RELAY_AUTH__JWT_SECRET=${JWT_SECRET}
RELAY_ADMIN__USERNAME=${RELAY_ADMIN_USERNAME}
RELAY_ADMIN__PASSWORD=${RELAY_ADMIN_PASSWORD}
EOF
  chmod 600 "$ENV_FILE"
  info "已生成 $ENV_FILE(端口 / JWT 密钥 / 管理员凭据)"
elif [ ! -f "$ENV_FILE" ]; then
  # 升级但从未有过 relay.env(旧版本装的):至少把端口固化下来
  echo "RELAY_SERVER__BIND=0.0.0.0:${RELAY_PORT}" > "$ENV_FILE"
  chmod 600 "$ENV_FILE"
fi

# ---- 6. 执行安装(install.sh 幂等:装程序/注册 systemd/保配置) ------------
info "开始安装到 $INSTALL_DIR"
(cd "$TMP/$PKG" && INSTALL_DIR="$INSTALL_DIR" ./install.sh)

# ---- 7. 健康检查 ----------------------------------------------------------
info "健康检查:$HEALTH_URL"
OK=""
for _ in $(seq 1 15); do
  if curl -fsS -m 3 "$HEALTH_URL" 2>/dev/null | grep -q '"status":"ok"'; then OK=1; break; fi
  sleep 2
done
[ -n "$OK" ] || {
  warn "健康检查未通过,请查看日志:journalctl -u relay -n 50"
  warn "常见原因:端口被占用(RELAY_SERVER__BIND 换端口)、配置语法错误。"
  exit 1
}
VER="$(curl -fsS -m 5 "$HEALTH_URL" 2>/dev/null | grep -o '"version":"[^"]*"' | cut -d'"' -f4 || true)"

# ---- 8. 完成横幅 ----------------------------------------------------------
IP="$(hostname -I 2>/dev/null | awk '{print $1}')"
IP="${IP:-127.0.0.1}"
echo
echo "============================================================"
echo "  ✔ Relay ${VER:+v$VER} ${UPGRADE:+升级}${UPGRADE:-安装}成功!"
echo "============================================================"
if [ "$UPGRADE" = "0" ]; then
  echo "  管理后台 : http://${IP}:${RELAY_PORT}/admin/"
  echo "  用户门户 : http://${IP}:${RELAY_PORT}/portal/"
  echo "  管理账号 : ${RELAY_ADMIN_USERNAME}"
  echo "  管理密码 : ${RELAY_ADMIN_PASSWORD}  (已写入 ${ENV_FILE},请妥善保管)"
  echo "  首次登录后请立即在「设置」中修改强密码。"
else
  echo "  升级完成,配置与数据均已保留。"
fi
echo
echo "  常用命令:"
echo "    systemctl status relay                    # 服务状态"
echo "    journalctl -u relay -f                    # 实时日志"
echo "    sudo bash $INSTALL_DIR/upgrade.sh         # 后续一键在线升级"
echo "============================================================"
