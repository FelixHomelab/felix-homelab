#!/usr/bin/env bash
#
# Felix-Homelab：OpenCloud 备份（宿主机执行，由 backup-run.service 定时触发）
#
# 为什么在宿主机跑而不是塞进备份容器：
#   · OpenCloud 的 decomposedfs 用 xattr（user.oc.*）存元数据，备份必须保留 xattr，
#     各容器镜像里的 busybox tar 不支持，宿主机 GNU tar 支持；
#   · 卷内文件属主是 rootless 的 subuid，宿主机用户直接读不到（目录 0700），
#     在 podman unshare（与容器同一 user namespace）里以映射 root 读取即可。
#
# 保留天数沿用 sync/backup.conf 的 KEEP_DAYS（与其它备份一致）。
set -euo pipefail

CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/felix-homelab"
BACKUP_DIR="${BACKUP_DIR:-${XDG_DATA_HOME:-$HOME/.local/share}/felix-homelab/backups}"
CONF="${CONFIG_DIR}/sync/backup.conf"
DATA_VOL=felix-homelab-opencloud-data
CFG_VOL=felix-homelab-opencloud-config
PREFIX="felix-homelab-opencloud-"

KEEP_DAYS=7
if [ -f "$CONF" ]; then
	# shellcheck disable=SC1090
	. "$CONF"
fi
KEEP_DAYS="${KEEP_DAYS:-7}"

log() { printf '%s [backup-opencloud] %s\n' "$(date '+%F %T')" "$*"; }

podman volume exists "$DATA_VOL" 2>/dev/null || { log "未安装 OpenCloud，跳过"; exit 0; }
[ -d "$BACKUP_DIR" ] || { log "备份目录不存在：$BACKUP_DIR"; exit 1; }

stamp="$(date +%Y%m%d-%H%M%S)"
target="${BACKUP_DIR}/${PREFIX}${stamp}.tar.gz"
data_mp="$(podman volume inspect -f '{{.Mountpoint}}' "$DATA_VOL")"
cfg_mp="$(podman volume inspect -f '{{.Mountpoint}}' "$CFG_VOL")"

log "开始备份 OpenCloud（配置 + 数据，带 xattr）-> ${target}"
# 配置卷挂载点内容即 /etc/opencloud 的内容，先在 userns 内暂存成 etc-opencloud/ 再入包，
# 与数据卷的 ./ 区分开；数据直接流式打包。tar 退出码 1 = 读取期间文件变化（服务在线），可接受。
set +e
podman unshare sh -c '
	data_mp="$1"; cfg_mp="$2"; target="$3"
	tmp="$(mktemp -d)"
	mkdir -p "$tmp/etc-opencloud"
	cp -a "$cfg_mp/." "$tmp/etc-opencloud/" 2>/dev/null
	tar --xattrs --xattrs-include="user.*" --ignore-failed-read \
		--warning=no-file-changed \
		-czf "$target" -C "$data_mp" . -C "$tmp" etc-opencloud
	rc=$?
	rm -rf "$tmp"
	exit $rc
' sh "$data_mp" "$cfg_mp" "$target"
rc=$?
set -e
if [ "$rc" -gt 1 ] || [ ! -s "$target" ]; then
	log "错误：打包失败（tar rc=$rc）"
	rm -f "$target"
	exit 1
fi
chmod 0644 "$target"

listing="$(tar tzf "$target")"
entries="$(printf '%s\n' "$listing" | wc -l)"
case "$listing" in
*etc-opencloud/opencloud.yaml*) ;;
*)
	log "错误：归档缺少配置，删除无效备份"
	rm -f "$target"
	exit 1
	;;
esac
if [ "$entries" -lt 5 ]; then
	log "错误：归档数据为空，删除无效备份"
	rm -f "$target"
	exit 1
fi
log "OpenCloud 备份完成并校验通过（$(du -h "$target" | cut -f1)）"

find "$BACKUP_DIR" -maxdepth 1 -type f -name "${PREFIX}*.tar.gz" \
	-mtime "+${KEEP_DAYS}" | while read -r old; do
	rm -f "$old"
	log "已清理过期 OpenCloud 备份 $(basename "$old")"
done
