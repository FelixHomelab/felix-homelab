#!/usr/bin/env bash
#
# Felix-Homelab：Kanidm 开户（建号 → 入组 → 生成中文 onboarding 链接）
#
# 用法：
#   scripts/kanidm-adduser.sh <用户名> "<显示名>" [邮箱] [附加组...]
# 例：
#   scripts/kanidm-adduser.sh zhangsan "张三" zhangsan@example.com
#   scripts/kanidm-adduser.sh lisi "李四" -- admins
#
# 完成后把打印的 onboarding 链接私发给对方（微信/QQ 均可），
# 对方在浏览器里设置密码即完成（普通用户无需验证器；密码需 4/4 强度，建议 4 个词用 - 连接）。
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/felix-homelab"
ID_BASE="${ID_BASE:-https://id.wraindrock.com}"

if [ $# -lt 2 ]; then
	echo "用法: $0 <用户名> \"<显示名>\" [邮箱] [附加组...]" >&2
	exit 1
fi
USER="$1"; shift
DISPLAY="$1"; shift
MAIL=""
if [ $# -gt 0 ] && [[ "$1" == *@* ]]; then
	MAIL="$1"; shift
fi
EXTRA_GROUPS=("$@")

PW="$(grep -E '^KANIDM_ADMIN_PASSWORD=' "$CONFIG_DIR/.env" | cut -d= -f2- || true)"
[ -n "$PW" ] || { echo "错误：$CONFIG_DIR/.env 缺少 KANIDM_ADMIN_PASSWORD" >&2; exit 1; }

run() { PW="$PW" "$HERE/kanidm-cli.py" "$*"; }

echo "== 创建 $USER（$DISPLAY）"
run "kanidm person create '$USER' '$DISPLAY'"

if [ -n "$MAIL" ]; then
	run "kanidm person update '$USER' --mail '$MAIL'"
fi

echo "== 加入基础组 felix-users"
run "kanidm group add-members felix-users '$USER' --name idm_admin"
for g in "${EXTRA_GROUPS[@]:-}"; do
	[ -n "$g" ] || continue
	echo "== 加入组 $g"
	run "kanidm group add-members '$g' '$USER' --name idm_admin"
done

echo "== 生成 onboarding 链接（7 天有效）"
OUT="$(run "kanidm person credential create-reset-token '$USER' --ttl 604800")"
TOKEN="$(printf '%s\n' "$OUT" | grep -oE 'use-reset-token [A-Za-z0-9-]+' | awk '{print $2}' | head -1)"
if [ -z "$TOKEN" ]; then
	echo "错误：未能解析 reset token，原始输出：" >&2
	printf '%s\n' "$OUT" >&2
	exit 1
fi

echo
echo "================ 发给用户的内容 ================"
echo "1. 打开：$ID_BASE/ui/reset?token=$TOKEN"
echo "2. 设置密码（建议 4 个词用 - 连接，如 blue-cat-happy-river）"
echo "3. 完成，之后可用该账号登录已接入的服务（Forgejo 等）"
echo "================================================"
