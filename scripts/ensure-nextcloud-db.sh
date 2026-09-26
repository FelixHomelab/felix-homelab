#!/usr/bin/env bash
#
# 确保 PostgreSQL 中存在 Nextcloud 的库与角色（幂等）
#
# Nextcloud 首次启动前必须已有数据库，否则容器会报错退出。
# 这里复用 Felix-Workstation 的 PostgreSQL 容器，创建独立的 nextcloud 库/角色。
#
set -euo pipefail

CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/felix-workstation"
ENV_FILE="$CONFIG_DIR/nextcloud.env"
DB_CONTAINER="${DB_CONTAINER:-felix-workstation-db}"
NC_DB="nextcloud"
NC_USER="nextcloud"

die() { printf '\033[1;31m[felix]\033[0m %s\n' "$*" >&2; exit 1; }

[ -f "$ENV_FILE" ] || die "缺少 $ENV_FILE（先执行 make install 生成）"
PW="$(grep '^POSTGRES_PASSWORD=' "$ENV_FILE" | head -1 | cut -d= -f2-)"
[ -n "$PW" ] || die "nextcloud.env 缺少 POSTGRES_PASSWORD"
podman container exists "$DB_CONTAINER" || die "数据库容器未运行：$DB_CONTAINER"

# 角色：不存在则建，存在则同步密码
podman exec "$DB_CONTAINER" psql -U forgejo -d postgres -v ON_ERROR_STOP=1 -c \
	"DO \$\$ BEGIN IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname='$NC_USER') THEN CREATE ROLE $NC_USER LOGIN PASSWORD '$PW'; ELSE ALTER ROLE $NC_USER WITH LOGIN PASSWORD '$PW'; END IF; END \$\$;" >/dev/null

# 库：不存在则建
if ! podman exec "$DB_CONTAINER" psql -U forgejo -d postgres -tAc \
	"SELECT 1 FROM pg_database WHERE datname='$NC_DB'" | grep -q 1; then
	podman exec "$DB_CONTAINER" psql -U forgejo -d postgres -v ON_ERROR_STOP=1 -c \
		"CREATE DATABASE $NC_DB OWNER $NC_USER;" >/dev/null
fi

echo "Nextcloud 数据库就绪：$NC_DB（owner $NC_USER）"
