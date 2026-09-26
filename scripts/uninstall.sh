#!/usr/bin/env bash
#
# 卸载 Felix-Workstation
#
#   scripts/uninstall.sh            停止并移除单元，保留数据
#   scripts/uninstall.sh --purge    同时删除数据卷、配置与镜像加速配置
#
set -euo pipefail

PURGE=0
[ "${1:-}" = "--purge" ] && PURGE=1

CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/felix-workstation"
UNIT_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/containers/systemd"
SYSTEMD_USER_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
REGISTRY_DROPIN="${XDG_CONFIG_HOME:-$HOME/.config}/containers/registries.conf.d/100-felix-workstation.conf"
POD_NAME="Felix-Workstation"

SERVICES=(
	felix-workstation-backup.service
	felix-workstation-autoheal.service
	felix-workstation-runner.service
	felix-workstation-homepage.service
	felix-workstation-site.service
	felix-workstation-nextcloud.service
	felix-workstation-caddy.service
	felix-workstation-forgejo.service
	felix-workstation-db.service
	felix-workstation-pod.service
)

VOLUMES=(
	felix-workstation-db-data
	felix-workstation-forgejo-data
	felix-workstation-site-data
	felix-workstation-nextcloud-data
	felix-workstation-runner-data
	felix-workstation-caddy-data
	felix-workstation-caddy-config
)

log() { printf '\033[1;36m[felix]\033[0m %s\n' "$*"; }

log "停止服务"
for svc in "${SERVICES[@]}"; do
	systemctl --user stop "$svc" 2>/dev/null || true
done

log "移除备份定时器与触发单元"
systemctl --user disable --now felix-workstation-backup.timer \
	felix-workstation-backup-sync.timer \
	felix-workstation-backup-request.path \
	felix-workstation-backup-sync-request.path 2>/dev/null || true
rm -f "$SYSTEMD_USER_DIR/felix-workstation-backup.timer" \
	"$SYSTEMD_USER_DIR/felix-workstation-backup-run.service" \
	"$SYSTEMD_USER_DIR/felix-workstation-backup-sync.timer" \
	"$SYSTEMD_USER_DIR/felix-workstation-backup-sync.service" \
	"$SYSTEMD_USER_DIR/felix-workstation-backup-request.path" \
	"$SYSTEMD_USER_DIR/felix-workstation-backup-sync-request.path"

log "移除 Quadlet 软链接"
for f in "$UNIT_DIR"/felix-workstation.*; do
	[ -L "$f" ] && rm -f "$f"
done

systemctl --user daemon-reload

if podman pod exists "$POD_NAME" 2>/dev/null; then
	log "强制移除 Pod $POD_NAME"
	podman pod rm -f "$POD_NAME" >/dev/null 2>&1 || true
fi

	if [ "$PURGE" -eq 1 ]; then
		log "删除数据卷"
		for v in "${VOLUMES[@]}"; do
			podman volume rm -f "$v" >/dev/null 2>&1 || true
		done

		log "删除配置目录 $CONFIG_DIR"
		rm -rf "$CONFIG_DIR"
		rm -f "$REGISTRY_DROPIN"

		log "移除 Podman API 常驻配置"
		rm -f "${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user/podman.service.d/10-felix-workstation.conf"
		systemctl --user daemon-reload
		systemctl --user stop podman.service 2>/dev/null || true
		systemctl --user restart podman.socket
	fi

log "卸载完成"
