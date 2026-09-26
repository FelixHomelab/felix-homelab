#!/usr/bin/env bash
#
# Nextcloud 备份（在宿主机执行）
#
# 备份容器里没有 pg_dump、也不便直接读取 Nextcloud 数据卷，因此这一步放在宿主机：
#   1. 用 db 容器的 pg_dump 导出一致性快照（自定义格式）
#   2. 用一次性容器把 Nextcloud 数据卷打包
#   3. 合并为一个归档 felix-ws-nextcloud-<时间戳>.tar.gz
#
# 是否执行由 backup.conf 的 BACKUP_NEXTCLOUD=1 决定。
#
set -euo pipefail

CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/felix-workstation"
BACKUP_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/felix-workstation/backups"
CONF="$CONFIG_DIR/sync/backup.conf"
DB_CONTAINER="${DB_CONTAINER:-felix-workstation-db}"
NC_VOLUME="${NC_VOLUME:-felix-workstation-nextcloud-data}"
HELPER_IMAGE="docker.io/library/alpine:3.20"

KEEP_DAYS=7
if [ -f "$CONF" ]; then
	# shellcheck disable=SC1090
	. "$CONF"
fi
KEEP_DAYS="${KEEP_DAYS:-7}"

log() { printf '%s [nextcloud] %s\n' "$(date '+%F %T')" "$*"; }

[ "${BACKUP_NEXTCLOUD:-0}" = "1" ] || { log "备份源未启用：Nextcloud（跳过）"; exit 0; }

podman container exists "$DB_CONTAINER" || { log "数据库容器未运行，跳过"; exit 0; }

stamp="$(date '+%F_%H%M%S')"
target="$BACKUP_DIR/felix-ws-nextcloud-${stamp}.tar.gz"
mkdir -p "$BACKUP_DIR"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

log "导出 Nextcloud 数据库..."
podman exec "$DB_CONTAINER" pg_dump -U forgejo -Fc nextcloud >"$tmp/nextcloud-db.dump"

log "打包 Nextcloud 数据卷（可能较大）..."
if podman volume exists "$NC_VOLUME"; then
	podman run --rm \
		-v "$NC_VOLUME":/data:ro \
		-v "$tmp":/out:Z \
		"$HELPER_IMAGE" tar -czf /out/nextcloud-files.tar.gz -C /data . >/dev/null
else
	log "数据卷不存在，仅备份数据库"
fi

tar -czf "$target" -C "$tmp" .
chmod 0644 "$target"

if ! tar tzf "$target" | grep -q 'nextcloud-db.dump'; then
	log "错误：归档缺少数据库，判定无效并删除"
	rm -f "$target"
	exit 1
fi
log "完成并校验通过：$target"

find "$BACKUP_DIR" -maxdepth 1 -name 'felix-ws-nextcloud-*.tar.gz' -type f \
	-mtime "+${KEEP_DAYS}" | while read -r old; do
	rm -f "$old"
	log "已清理过期 Nextcloud 备份 $(basename "$old")"
done
