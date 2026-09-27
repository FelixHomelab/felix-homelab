#!/usr/bin/env bash
#
# 便捷工具：直接用命令行创建管理员账号（可选）
#
# 用法：scripts/create-admin.sh <用户名> <密码> <邮箱>
# 若未提供参数，则进入交互输入。
#
set -euo pipefail

FORGEJO_CONTAINER="${FORGEJO_CONTAINER:-felix-homelab-forgejo}"
APP_INI="/data/gitea/conf/app.ini"

die() { printf '\033[1;31m[felix]\033[0m %s\n' "$*" >&2; exit 1; }

podman container exists "$FORGEJO_CONTAINER" || die "Forgejo 容器未运行，请先执行 make install"

USERNAME="${1:-}"
PASSWORD="${2:-}"
EMAIL="${3:-}"

if [ -z "$USERNAME" ] || [ -z "$PASSWORD" ] || [ -z "$EMAIL" ]; then
	read -rp "管理员用户名: " USERNAME
	read -rsp "管理员密码  : " PASSWORD
	echo
	read -rp "管理员邮箱  : " EMAIL
fi

podman exec --user 1000 -w /data/gitea "$FORGEJO_CONTAINER" \
	forgejo admin user create --config "$APP_INI" \
	--username "$USERNAME" --password "$PASSWORD" --email "$EMAIL" --admin

echo "管理员 $USERNAME 已创建。"
