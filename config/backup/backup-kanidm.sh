#!/usr/bin/env bash
#
# Felix-Homelab：Kanidm 备份（宿主机执行，加入每日/手动备份）
#
# 做法：在 podman unshare 里用 SQLite 的在线 .backup 导出数据库（WAL 模式下也一致），
# 再带上 TLS 证书，一起打成 tar.gz；保留天数沿用 sync/backup.conf 的 KEEP_DAYS。
set -euo pipefail

CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/felix-homelab"
BACKUP_DIR="${BACKUP_DIR:-${XDG_DATA_HOME:-$HOME/.local/share}/felix-homelab/backups}"
CONF="${CONFIG_DIR}/sync/backup.conf"
VOL=felix-homelab-kanidm-data
PREFIX="felix-homelab-kanidm-"

KEEP_DAYS=7
if [ -f "$CONF" ]; then
	# shellcheck disable=SC1090
	. "$CONF"
fi
KEEP_DAYS="${KEEP_DAYS:-7}"

log() { printf '%s [backup-kanidm] %s\n' "$(date '+%F %T')" "$*"; }

podman volume exists "$VOL" 2>/dev/null || { log "未安装 Kanidm，跳过"; exit 0; }
[ -d "$BACKUP_DIR" ] || { log "备份目录不存在：$BACKUP_DIR"; exit 1; }

mp="$(podman volume inspect -f '{{.Mountpoint}}' "$VOL")"
stamp="$(date +%Y%m%d-%H%M%S)"
target="${BACKUP_DIR}/${PREFIX}${stamp}.tar.gz"
tmp="${BACKUP_DIR}/.kanidm-tmp.$$"

cleanup() { rm -rf "$tmp"; }
trap cleanup EXIT

mkdir -p "$tmp"
log "开始备份 Kanidm（SQLite 在线导出 + 证书）-> ${target}"
podman unshare python3 - "$mp" "$tmp" <<'PYEOF'
import os, shutil, sqlite3, sys

mp, tmp = sys.argv[1], sys.argv[2]
db = os.path.join(mp, "kanidm.db")
if os.path.exists(db):
    src = sqlite3.connect(db)
    dst = sqlite3.connect(os.path.join(tmp, "kanidm.db"))
    src.backup(dst)
    dst.close()
    src.close()
for name in ("chain.pem", "key.pem", "ca.pem", "cert.pem"):
    path = os.path.join(mp, name)
    if os.path.exists(path):
        shutil.copy2(path, os.path.join(tmp, name))
PYEOF

tar -czf "$target" -C "$tmp" .
chmod 0644 "$target"

listing="$(tar tzf "$target")"
case "$listing" in
*kanidm.db*) ;;
*)
	log "错误：归档缺少 kanidm.db，删除无效备份"
	rm -f "$target"
	exit 1
	;;
esac
log "Kanidm 备份完成并校验通过（$(du -h "$target" | cut -f1)）"

find "$BACKUP_DIR" -maxdepth 1 -type f -name "${PREFIX}*.tar.gz" \
	-mtime "+${KEEP_DAYS}" | while read -r old; do
	rm -f "$old"
	log "已清理过期 Kanidm 备份 $(basename "$old")"
done
