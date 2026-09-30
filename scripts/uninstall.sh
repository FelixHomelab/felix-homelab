#!/usr/bin/env bash
#
# 卸载 Felix-Homelab
#
#   scripts/uninstall.sh            停止并移除单元，保留数据
#   scripts/uninstall.sh --purge    同时删除数据卷、配置与镜像加速配置
#
set -euo pipefail

PURGE=0
[ "${1:-}" = "--purge" ] && PURGE=1

CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/felix-homelab"
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/felix-homelab"
UNIT_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/containers/systemd"
SYSTEMD_USER_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
REGISTRY_DROPIN="${XDG_CONFIG_HOME:-$HOME/.config}/containers/registries.conf.d/100-felix-homelab.conf"
POD_NAME="Felix-Homelab"

SERVICES=(
	felix-homelab-backup.service
	felix-homelab-autoheal.service
	felix-homelab-runner.service
	felix-homelab-frpc.service
	felix-homelab-agent-frpc.service
	felix-homelab-agent-gateway.service
	felix-homelab-homepage.service
	felix-homelab-site.service
	felix-homelab-caddy.service
	felix-homelab-opencloud.service
	felix-homelab-kanidm.service
	felix-homelab-forgejo.service
	felix-homelab-db.service
	felix-homelab-pod.service
)

VOLUMES=(
	felix-homelab-db-data
	felix-homelab-forgejo-data
	felix-homelab-site-data
	felix-homelab-runner-data
	felix-homelab-caddy-data
	felix-homelab-caddy-config
	felix-homelab-agent-gateway-data
	felix-homelab-agent-gateway-config
	felix-homelab-opencloud-config
	felix-homelab-opencloud-data
	felix-homelab-kanidm-data
)

log() { printf '\033[1;36m[felix]\033[0m %s\n' "$*"; }

log "停止服务"
for svc in "${SERVICES[@]}"; do
	systemctl --user stop "$svc" 2>/dev/null || true
done

log "移除备份定时器与触发单元"
systemctl --user disable --now felix-homelab-backup.timer \
	felix-homelab-backup-sync.timer \
	felix-homelab-backup-request.path \
	felix-homelab-backup-sync-request.path 2>/dev/null || true
rm -f "$SYSTEMD_USER_DIR/felix-homelab-backup.timer" \
	"$SYSTEMD_USER_DIR/felix-homelab-backup-run.service" \
	"$SYSTEMD_USER_DIR/felix-homelab-backup-sync.timer" \
	"$SYSTEMD_USER_DIR/felix-homelab-backup-sync.service" \
	"$SYSTEMD_USER_DIR/felix-homelab-backup-request.path" \
	"$SYSTEMD_USER_DIR/felix-homelab-backup-sync-request.path"

log "移除 Agent 定时器与请求触发单元"
systemctl --user disable --now felix-homelab-agent.timer \
	felix-homelab-agent-request.path 2>/dev/null || true
rm -f "$SYSTEMD_USER_DIR/felix-homelab-agent.timer" \
	"$SYSTEMD_USER_DIR/felix-homelab-agent-request.path" \
	"$SYSTEMD_USER_DIR/felix-homelab-agent-run.service"

log "移除 Quadlet 软链接"
for f in "$UNIT_DIR"/felix-homelab*; do
	[ -L "$f" ] && rm -f "$f"
done

systemctl --user daemon-reload

# 多租户 Agent 容器不在 Pod 里（各自独立 bridge 网络），必须单独清理；
# 数据卷/工作区保留，--purge 时再删。
if podman ps -a --format '{{.Names}}' 2>/dev/null | grep -q '^felix-agent-'; then
	log "停止并移除多租户 Agent 容器"
	while IFS= read -r c; do
		[ -n "$c" ] && podman rm -f "$c" >/dev/null 2>&1 || true
	done < <(podman ps -a --format '{{.Names}}' 2>/dev/null | grep '^felix-agent-')
fi

if podman pod exists "$POD_NAME" 2>/dev/null; then
	log "强制移除 Pod $POD_NAME"
	podman pod rm -f "$POD_NAME" >/dev/null 2>&1 || true
fi

	if [ "$PURGE" -eq 1 ]; then
		log "删除数据卷"
		for v in "${VOLUMES[@]}"; do
			podman volume rm -f "$v" >/dev/null 2>&1 || true
		done

		log "删除多租户 Agent 数据卷、工作区、网络与镜像"
		while IFS= read -r v; do
			[ -n "$v" ] && podman volume rm -f "$v" >/dev/null 2>&1 || true
		done < <(podman volume ls --format '{{.Name}}' 2>/dev/null | grep '^felix-agent-' || true)
		while IFS= read -r n; do
			[ -n "$n" ] && podman network rm -f "$n" >/dev/null 2>&1 || true
		done < <(podman network ls --format '{{.Name}}' 2>/dev/null | grep '^felix-agent-' || true)
		rm -rf "$DATA_DIR/agents"
		podman image rm -f localhost/felix-agent-opencode:latest >/dev/null 2>&1 || true
		podman image rm -f localhost/felix-agent-dsh:latest >/dev/null 2>&1 || true

		log "删除配置目录 $CONFIG_DIR"
		rm -rf "$CONFIG_DIR"
		rm -f "$REGISTRY_DROPIN"

		log "移除 Podman API 常驻配置"
		rm -f "${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user/podman.service.d/10-felix-homelab.conf"
		systemctl --user daemon-reload
		systemctl --user stop podman.service 2>/dev/null || true
		systemctl --user restart podman.socket
	fi

log "卸载完成"
