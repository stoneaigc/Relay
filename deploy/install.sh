#!/usr/bin/env bash
# RunAPI 安装/升级脚本。在解压后的包目录里执行:sudo ./install.sh
# 可用环境变量覆盖安装目录:INSTALL_DIR=/srv/runapi sudo -E ./install.sh
set -euo pipefail

INSTALL_DIR="${INSTALL_DIR:-/opt/runapi}"
SERVICE="runapi"
SRC="$(cd "$(dirname "$0")" && pwd)" # 包根目录(脚本所在目录)

if [ "$(id -u)" -ne 0 ]; then
  echo "需要 root 权限(安装到 $INSTALL_DIR 并注册 systemd)。请用 sudo 运行。" >&2
  exit 1
fi

echo "==> 安装目录:$INSTALL_DIR"
mkdir -p "$INSTALL_DIR"

# 二进制 + 前端:每次都更新
echo "==> 更新二进制与前端资源"
install -m 0755 "$SRC/runapi" "$INSTALL_DIR/runapi"
rm -rf "$INSTALL_DIR/frontend"
cp -r "$SRC/frontend" "$INSTALL_DIR/frontend"

# 配置:仅首次安装时写入,避免覆盖线上已改过的配置
mkdir -p "$INSTALL_DIR/config"
if [ ! -f "$INSTALL_DIR/config/default.toml" ]; then
  cp "$SRC/config/default.toml" "$INSTALL_DIR/config/default.toml"
  echo "==> 已写入默认配置:$INSTALL_DIR/config/default.toml(请按需修改)"
else
  echo "==> 保留已存在的 config/default.toml(未覆盖)"
fi

# 敏感配置文件模板(首次创建,空)
if [ ! -f "$INSTALL_DIR/runapi.env" ]; then
  cat > "$INSTALL_DIR/runapi.env" <<'EOF'
# systemd 注入的环境变量,每行 KEY=VALUE。敏感项建议放这里而非 default.toml。
# 例:
# RUNAPI_EMAIL__PASSWORD=你的邮箱授权码
EOF
  chmod 600 "$INSTALL_DIR/runapi.env"
  echo "==> 已创建 $INSTALL_DIR/runapi.env(可写入授权码等敏感变量)"
fi

# systemd 单元:把占位符替换成实际安装目录
echo "==> 安装 systemd 服务:$SERVICE"
sed "s#@INSTALL_DIR@#${INSTALL_DIR}#g" "$SRC/runapi.service" \
  > "/etc/systemd/system/${SERVICE}.service"
systemctl daemon-reload
systemctl enable "$SERVICE" >/dev/null
systemctl restart "$SERVICE"

echo
echo "==> 完成。状态/日志:"
echo "    systemctl status $SERVICE"
echo "    journalctl -u $SERVICE -f"
echo "    监听 :8080 → 门户 /portal/ ,后台 /admin/"
