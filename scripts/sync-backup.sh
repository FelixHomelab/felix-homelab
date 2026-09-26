#!/usr/bin/env bash
#
# 备份异地同步（在宿主机执行）
#
# 读取 backup.conf 里的渠道（CHANNEL_<n>_*），逐个同步**已启用**的渠道：
#   - rclone：用官方 rclone 容器同步（WebDAV / S3 / R2 / OSS / Drive…）
#   - rsync：用宿主机 rsync（外置盘 / NAS / SSH）
#
# 每个渠道独立开关，可同时启用多个，也可全部关闭。
# 结果写入 backup/sync.status，后台「备份」页会展示。
#
set -euo pipefail

CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/felix-workstation"
BACKUP_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/felix-workstation/backups"
CONF="$CONFIG_DIR/sync/backup.conf"
STATUS="$CONFIG_DIR/backup/sync.status"
RCLONE_DIR="$CONFIG_DIR/rclone"
RCLONE_IMAGE="docker.io/rclone/rclone:latest"

DRY_RUN=""
[ "${1:-}" = "--dry-run" ] && DRY_RUN="--dry-run"

log() { printf '\033[1;36m[felix]\033[0m %s\n' "$*"; }

[ -d "$BACKUP_DIR" ] || { echo "备份目录不存在：$BACKUP_DIR" >&2; exit 1; }

if [ -f "$CONF" ]; then
	# shellcheck disable=SC1090
	. "$CONF"
fi
COUNT="${CHANNEL_COUNT:-0}"

mkdir -p "$(dirname "$STATUS")"
: >"$STATUS"

pick() { local name="CHANNEL_${1}_${2}"; printf '%s' "${!name:-}"; }

ok=0
fail=0
for i in $(seq 1 "$COUNT"); do
	name="$(pick "$i" NAME)"
	kind="$(pick "$i" KIND)"
	target="$(pick "$i" TARGET)"
	enable="$(pick "$i" ENABLE)"
	[ "${enable:-0}" = "1" ] || continue

	ts="$(date '+%F %T')"
	[ -n "$target" ] || {
		printf '%s fail: %s（同步目标为空）\n' "$ts" "$name" >>"$STATUS"
		fail=$((fail + 1))
		continue
	}

	if [ "${kind:-rclone}" = "rsync" ]; then
		if command -v rsync >/dev/null 2>&1; then
			if rsync -a --delete $DRY_RUN "$BACKUP_DIR"/ "$target"/; then
				printf '%s ok: %s（rsync）\n' "$ts" "$name" >>"$STATUS"
				ok=$((ok + 1))
			else
				printf '%s fail: %s（rsync 返回非零）\n' "$ts" "$name" >>"$STATUS"
				fail=$((fail + 1))
			fi
		else
			printf '%s fail: %s（本机没有 rsync）\n' "$ts" "$name" >>"$STATUS"
			fail=$((fail + 1))
		fi
	else
		mkdir -p "$RCLONE_DIR"
		if podman run --rm $DRY_RUN \
			-v "$BACKUP_DIR":/backups:ro \
			-v "$RCLONE_DIR":/config/rclone:ro \
			--security-opt label=disable \
			"$RCLONE_IMAGE" sync /backups "$target" \
			--config /config/rclone/rclone.conf --checksum --transfers 4; then
			printf '%s ok: %s（rclone）\n' "$ts" "$name" >>"$STATUS"
			ok=$((ok + 1))
		else
			printf '%s fail: %s（rclone 返回非零）\n' "$ts" "$name" >>"$STATUS"
			fail=$((fail + 1))
		fi
	fi
done

printf '%s 完成：成功 %d，失败 %d\n' "$(date '+%F %T')" "$ok" "$fail" >>"$STATUS"
log "同步结束：成功 $ok，失败 $fail（详见 $STATUS）"
[ "$fail" -eq 0 ]
