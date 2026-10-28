#!/usr/bin/env bash
# Relay 运维命令行:/usr/local/bin/relay(install.sh 由模板渲染安装)。
# 用法: relay <command> [args];不带 root 时控制类命令自动加 sudo。
#   relay status              服务状态
#   relay start|stop|restart  启停/重启
#   relay logs [-f] [-n N]    日志(默认最近 100 行;-f 跟随)
#   relay health              探活(healthz,含版本)
#   relay port                查看当前端口
#   relay port <n>            修改端口并重启(写 relay.env)
#   relay info                版本/路径/端口/数据库一览
#   relay backup [目录]       SQLite 在线热备(默认 /var/backups/relay)
#   relay upgrade [版本]      升级(调用安装目录内 upgrade.sh)
set -uo pipefail

INSTALL_DIR="@INSTALL_DIR@"
SERVICE="@SERVICE@"
ENV_FILE="$INSTALL_DIR/relay.env"
CONFIG_FILE="$INSTALL_DIR/config/default.toml"
BACKUP_DIR="/var/backups/relay"

# 非 root 时自动提权(控制类/日志类命令需要)
run() {
  if [ "$(id -u)" -ne 0 ] && command -v sudo >/dev/null 2>&1; then
    sudo "$@"
  else
    "$@"
  fi
}

# 去掉值两侧的引号与空白
clean() { sed -e 's/^[[:space:]"]*//' -e 's/[[:space:]"]*$//'; }

# 当前生效端口:relay.env 的 RELAY_SERVER__BIND → config 的 bind → 默认 9000
detect_port() {
  local p=""
  if [ -f "$ENV_FILE" ]; then
    p="$(grep -E '^[[:space:]]*RELAY_SERVER__BIND=' "$ENV_FILE" 2>/dev/null | tail -n1 | cut -d= -f2- | clean | awk -F: '{print $NF}')"
  fi
  if [ -z "$p" ] && [ -f "$CONFIG_FILE" ]; then
    p="$(grep -E '^[[:space:]]*bind[[:space:]]*=' "$CONFIG_FILE" 2>/dev/null | head -n1 | cut -d= -f2- | clean | awk -F: '{print $NF}')"
  fi
  echo "${p:-9000}"
}

healthz() {
  curl -s --max-time 3 "http://127.0.0.1:$(detect_port)/healthz"
}

cmd_status() { run systemctl status "$SERVICE" --no-pager; }
cmd_start()   { run systemctl start "$SERVICE";   echo "==> 已启动:$(healthz || echo '探活失败,relay logs 查看')"; }
cmd_stop()    { run systemctl stop "$SERVICE";    echo "==> 已停止。"; }
cmd_restart() { run systemctl restart "$SERVICE"; sleep 1; echo "==> 已重启:$(healthz || echo '探活失败,relay logs 查看')"; }

cmd_logs() {
  run journalctl -u "$SERVICE" "$@"
}

cmd_health() {
  local out; out="$(healthz)"
  if [ -z "$out" ]; then
    echo "✗ 服务未响应(端口 $(detect_port))。用 relay status / relay logs 排查。"
    return 1
  fi
  echo "✓ $out"
}

cmd_port() {
  if [ $# -eq 0 ]; then
    echo "当前端口:$(detect_port)"
    return 0
  fi
  local port="$1"
  if ! [[ "$port" =~ ^[0-9]+$ ]] || [ "$port" -lt 1 ] || [ "$port" -gt 65535 ]; then
    echo "端口需为 1-65535 的数字: $port"
    return 1
  fi
  run touch "$ENV_FILE"
  if run grep -qE '^[[:space:]]*RELAY_SERVER__BIND=' "$ENV_FILE"; then
    run sed -i "s#^[[:space:]]*RELAY_SERVER__BIND=.*#RELAY_SERVER__BIND=0.0.0.0:${port}#" "$ENV_FILE"
  else
    echo "RELAY_SERVER__BIND=0.0.0.0:${port}" | run tee -a "$ENV_FILE" >/dev/null
  fi
  run chown relay:relay "$ENV_FILE" 2>/dev/null || true
  cmd_restart
  echo "==> 新端口监听日志:"
  run journalctl -u "$SERVICE" -n 20 --no-pager | grep "listening on" | tail -n1 \
    || echo "    (未见 listening 行,relay logs 查看启动是否成功)"
  echo "==> 别忘了放行防火墙/安全组的 ${port} 端口。"
}

cmd_info() {
  local port; port="$(detect_port)"
  local ver; ver="$(healthz | grep -o '"version":"[^"]*"' | cut -d'"' -f4)"
  local state; state="$(run systemctl is-active "$SERVICE" 2>/dev/null || echo unknown)"
  local db; db="$(grep -E '^[[:space:]]*url[[:space:]]*=' "$CONFIG_FILE" 2>/dev/null | head -n1 | cut -d= -f2- | clean)"
  echo "Relay 运维信息"
  echo "  服务状态 : $state  (systemd: $SERVICE)"
  echo "  版本     : ${ver:-未知(服务未响应)}"
  echo "  端口     : ${port}  → http://127.0.0.1:${port}/healthz"
  echo "  安装目录 : $INSTALL_DIR"
  echo "  环境变量 : $ENV_FILE"
  echo "  配置文件 : $CONFIG_FILE"
  echo "  数据库   : ${db:-sqlite(默认)}"
  echo "  日志     : relay logs [-f]"
}

cmd_backup() {
  local out_dir="${1:-$BACKUP_DIR}"
  local db_type
  db_type="$(grep -E '^[[:space:]]*type[[:space:]]*=' "$CONFIG_FILE" 2>/dev/null | head -n1 | cut -d= -f2- | clean)"
  db_type="${db_type:-sqlite}"
  if [ "$db_type" != "sqlite" ]; then
    echo "当前数据库为 $db_type,请用对应工具备份,例如:"
    echo "  pg_dump -h 127.0.0.1 -U relay -d relay -Fc -f /backup/relay-\$(date +%F).dump"
    return 1
  fi
  local url db
  url="$(grep -E '^[[:space:]]*url[[:space:]]*=' "$CONFIG_FILE" 2>/dev/null | head -n1 | cut -d= -f2- | clean)"
  db="${url#sqlite://}"
  case "$db" in /*) ;; *) db="$INSTALL_DIR/$db" ;; esac
  if [ ! -f "$db" ]; then
    echo "未找到 SQLite 库文件: $db(relay info 查看数据库配置)"
    return 1
  fi
  if ! command -v sqlite3 >/dev/null 2>&1; then
    echo "未安装 sqlite3,无法在线热备。两种选择:"
    echo "  1) 安装后重试:  apt install -y sqlite3  (或 yum install -y sqlite)"
    echo "  2) 停服冷备:    relay stop && cp $db 备份路径 && relay start"
    return 1
  fi
  local ts out
  ts="$(date +%F_%H%M%S)"
  out="$out_dir/relay-$ts.db"
  run mkdir -p "$out_dir"
  run sqlite3 "$db" ".backup '$out'" || { echo "备份失败"; return 1; }
  echo "✓ 已备份: $out ($(du -h "$out" | cut -f1))"
  echo "  恢复: relay stop → 用备份覆盖 $db → relay start"
}

cmd_upgrade() { run env INSTALL_DIR="$INSTALL_DIR" bash "$INSTALL_DIR/upgrade.sh" "$@"; }

# 卸载:停服务 → 删单元 → 自动备份 SQLite/relay.env → 删安装目录 → 自删除 CLI。
# 需交互确认;--yes/-y 跳过(脚本场景)。
cmd_uninstall() {
  local confirm="${1:-}"
  if ! command -v systemctl >/dev/null 2>&1; then
    echo "未检测到 systemd(Docker 等环境请用对应方式卸载,如 docker compose down)。"
    return 1
  fi
  echo "即将卸载 Relay,将删除:"
  echo "  - systemd 服务 : $SERVICE(disable --now + 单元文件)"
  echo "  - 安装目录     : $INSTALL_DIR(含数据库/配置/前端)"
  echo "  - 运维命令     : /usr/local/bin/relay"
  echo
  # 尽力自动备份:数据库(SQLite)与 relay.env,落在 $BACKUP_DIR
  local ts; ts="$(date +%F_%H%M%S)"
  local db_url db
  db_url="$(grep -E '^[[:space:]]*url[[:space:]]*=' "$CONFIG_FILE" 2>/dev/null | head -n1 | cut -d= -f2- | clean)"
  db="${db_url#sqlite://}"
  case "$db" in /*) ;; *) db="$INSTALL_DIR/$db" ;; esac
  if [ -f "$db" ] && command -v sqlite3 >/dev/null 2>&1; then
    run mkdir -p "$BACKUP_DIR"
    if run sqlite3 "$db" ".backup '$BACKUP_DIR/relay-uninstall-$ts.db'"; then
      echo "  ✓ 数据库已自动备份: $BACKUP_DIR/relay-uninstall-$ts.db"
    fi
  fi
  if [ -f "$ENV_FILE" ]; then
    run mkdir -p "$BACKUP_DIR"
    if run cp "$ENV_FILE" "$BACKUP_DIR/relay-env-uninstall-$ts.bak"; then
      echo "  ✓ relay.env 已自动备份: $BACKUP_DIR/relay-env-uninstall-$ts.bak"
    fi
  fi
  if [ "$confirm" != "--yes" ] && [ "$confirm" != "-y" ]; then
    local ans
    read -r -p "确认卸载?数据库/账号/配置将随安装目录删除(已有上方备份)。输入 yes 继续: " ans
    [ "$ans" = "yes" ] || { echo "已取消。"; return 1; }
  fi
  run systemctl disable --now "$SERVICE" 2>/dev/null || true
  run rm -f "/etc/systemd/system/$SERVICE.service"
  run systemctl daemon-reload 2>/dev/null || true
  run rm -rf "$INSTALL_DIR"
  echo "==> 服务与安装目录已删除。"
  echo "如需删除专用系统用户: sudo userdel relay(可选,不影响系统)"
  echo "==> 卸载完成,本命令(/usr/local/bin/relay)一并移除,再见。"
  # 自删除:exec 确保脚本文件被删后不再有后续读取
  if [ "$(id -u)" -ne 0 ] && command -v sudo >/dev/null 2>&1; then
    exec sudo rm -f /usr/local/bin/relay
  else
    exec rm -f /usr/local/bin/relay
  fi
}

usage() {
  cat <<'EOF'
Relay 运维命令
  relay status               服务状态
  relay start | stop | restart
  relay logs [-f] [-n N]     日志(默认最近 100 行;-f 实时跟随)
  relay health               探活(含版本)
  relay port                 查看端口
  relay port <n>             修改端口并重启(自动写 relay.env)
  relay info                 版本/路径/端口/数据库一览
  relay backup [目录]        SQLite 在线热备(默认 /var/backups/relay)
  relay upgrade [版本]       升级(默认拉最新 Release)
  relay uninstall [--yes]    卸载(自动备份数据库与 relay.env;--yes 跳过确认)
EOF
}

case "${1:-help}" in
  status)  shift; cmd_status "$@" ;;
  start)   cmd_start ;;
  stop)    cmd_stop ;;
  restart) cmd_restart ;;
  logs)    shift; if [ $# -eq 0 ]; then cmd_logs -n 100 --no-pager; else cmd_logs "$@"; fi ;;
  health)  cmd_health ;;
  port)    shift; cmd_port "$@" ;;
  info)    cmd_info ;;
  backup)  shift; cmd_backup "$@" ;;
  upgrade) shift; cmd_upgrade "$@" ;;
  uninstall) shift; cmd_uninstall "$@" ;;
  help|--help|-h|*) usage ;;
esac
