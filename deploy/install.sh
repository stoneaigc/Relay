#!/usr/bin/env bash
# Relay 安装/升级脚本。在解压后的包目录里执行:sudo ./install.sh
# 可用环境变量覆盖安装目录:INSTALL_DIR=/srv/relay sudo -E ./install.sh
set -euo pipefail

INSTALL_DIR="${INSTALL_DIR:-/opt/relay}"
SERVICE="relay"
SRC="$(cd "$(dirname "$0")" && pwd)" # 包根目录(脚本所在目录)

if [ "$(id -u)" -ne 0 ]; then
  echo "需要 root 权限(安装到 $INSTALL_DIR 并注册 systemd)。请用 sudo 运行。" >&2
  exit 1
fi

echo "==> 安装目录:$INSTALL_DIR"
mkdir -p "$INSTALL_DIR"

# 专用系统用户:服务不 root 运行(relay.service 的 User=relay 与此对应)
NOLOGIN="$(command -v nologin || echo /usr/sbin/nologin)"
if ! id -u relay >/dev/null 2>&1; then
  useradd --system --no-create-home --shell "$NOLOGIN" relay
  echo "==> 已创建系统用户 relay(shell=$NOLOGIN)"
fi

# 若在安装目录里(源=目标)执行,文件已就位,跳过自我拷贝,直接走 systemd。
if [ "$SRC" = "$(cd "$INSTALL_DIR" && pwd)" ]; then
  echo "==> 检测到在安装目录内运行,文件已就位,跳过拷贝"
else
  # 二进制 + 前端:每次都更新
  echo "==> 更新二进制与前端资源"
  install -m 0755 "$SRC/relay" "$INSTALL_DIR/relay"
  rm -rf "$INSTALL_DIR/frontend"
  cp -r "$SRC/frontend" "$INSTALL_DIR/frontend"
fi

# ---- 部署脚本:安装/升级脚本落到安装目录,后续可就地升级 -------------------
install -m 0755 "$SRC/install.sh" "$INSTALL_DIR/install.sh"
if [ -f "$SRC/upgrade.sh" ]; then
  install -m 0755 "$SRC/upgrade.sh" "$INSTALL_DIR/upgrade.sh"
fi

# 配置:仅首次安装时写入,避免覆盖线上已改过的配置
mkdir -p "$INSTALL_DIR/config"
if [ ! -f "$INSTALL_DIR/config/default.toml" ]; then
  cp "$SRC/config/default.toml" "$INSTALL_DIR/config/default.toml"
  echo "==> 已写入默认配置:$INSTALL_DIR/config/default.toml(请按需修改)"
else
  echo "==> 保留已存在的 config/default.toml(未覆盖)"
fi

# 敏感配置文件模板(首次创建,空)
if [ ! -f "$INSTALL_DIR/relay.env" ]; then
  cat > "$INSTALL_DIR/relay.env" <<'EOF'
# systemd 注入的环境变量,每行 KEY=VALUE。敏感项建议放这里而非 default.toml。
# 例:
# RELAY_EMAIL__PASSWORD=你的邮箱授权码
EOF
  chmod 600 "$INSTALL_DIR/relay.env"
  echo "==> 已创建 $INSTALL_DIR/relay.env(可写入授权码等敏感变量)"
fi

# 属主交给服务用户:SQLite 库写在安装目录(User=relay 需要写权限)
chown -R relay:relay "$INSTALL_DIR"

# systemd 单元:把占位符替换成实际安装目录;重启服务以加载新程序(升级路径的关键一步)
if command -v systemctl >/dev/null 2>&1; then
  echo "==> 安装 systemd 服务:$SERVICE"
  sed "s#@INSTALL_DIR@#${INSTALL_DIR}#g" "$SRC/relay.service" \
    > "/etc/systemd/system/${SERVICE}.service"
  systemctl daemon-reload
  systemctl enable "$SERVICE" >/dev/null
  systemctl restart "$SERVICE"
else
  # 无 systemd(如 Alpine/OpenRC):跳过注册,由调用方/人工重启加载新程序
  echo "==> 未检测到 systemd,跳过服务注册与自动重启。"
  echo "    如旧服务仍在运行,请手动重启以加载新程序,例如:"
  echo "    pkill -f '$INSTALL_DIR/relay' && nohup $INSTALL_DIR/relay >> $INSTALL_DIR/relay.log 2>&1 &"
fi

echo
echo "==> 完成。状态/日志:"
echo "    systemctl status $SERVICE"
echo "    journalctl -u $SERVICE -f"
echo "    监听 :8080 → 门户 /portal/ ,后台 /admin/"
