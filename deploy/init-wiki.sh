#!/usr/bin/env bash
# 将 docs/wiki/ 同步到 GitHub Wiki(需要仓库 Wiki 已启用且已创建过第一页)。
# 用法:在仓库根目录执行  bash deploy/init-wiki.sh
set -euo pipefail

REPO="${REPO:-stoneaigc/Relay}"
REMOTE="git@github.com:${REPO}.wiki.git"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SRC="$ROOT/docs/wiki"
WT="$(mktemp -d)"

trap 'rm -rf "$WT"' EXIT

# wiki 仓库已存在则 clone,否则本地 init 后直接 push(GitHub 允许时即完成初始化)
if git ls-remote "$REMOTE" >/dev/null 2>&1; then
  git clone "$REMOTE" "$WT"
else
  git init -b main "$WT"
fi

cp "$SRC"/*.md "$WT/"
cd "$WT"
git add -A
if git diff --cached --quiet; then
  echo "==> Wiki 已是最新,无需更新。"
else
  git -c user.name="stoneaigc" -c user.email="stoneaigc@users.noreply.github.com" \
    commit -m "docs: sync wiki from docs/wiki"
  git push -u origin HEAD:main
  echo "==> Wiki 已推送:https://github.com/$REPO/wiki"
fi
