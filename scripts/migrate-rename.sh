#!/usr/bin/env bash
#
# Felix-Workstation → Felix-Homelab 命名迁移（一次性）
#
# 旧版部署使用 felix-workstation 作为 pod / 单元 / 卷 / 配置目录名；本脚本把它们
# 迁移到 felix-homelab 命名，数据不丢：
#   1. 停止旧服务、禁用旧备份 timer/path，移除旧 Quadlet 软链接与 drop-in
#   2. 配置目录 ~/.config/felix-workstation → ~/.config/felix-homelab
#   3. 数据目录 ~/.local/share/felix-workstation → ~/.local/share/felix-homelab（含备份归档）
#   4. 复制数据卷 felix-workstation-* → felix-homelab-*（旧卷默认保留，确认可删后用 --prune-old）
#   5. 移除旧 Pod Felix-Workstation
#
# 迁移完成后执行：
#   make install     # 以新命名启动（会创建新 Pod）
#   make register    # 重新注册 Runner（名字由 felix-workstation 变为 felix-homelab）
# 并在 Forgejo 后台「站点管理 → Actions → Runner」删除旧的离线 Runner。
#
# 用法：
#   scripts/migrate-rename.sh [--yes] [--prune-old]
#     --yes        不交互确认
#     --prune-old  复制完成后删除旧数据卷（建议确认新服务正常后再用）
#
set -euo pipefail

ASSUME_YES=0
PRUNE_OLD=0
for arg in "$@"; do
	case "$arg" in
		--yes | -y) ASSUME_YES=1 ;;
		--prune-old) PRUNE_OLD=1 ;;
		-h | --help)
			sed -n '2,22p' "$0" | sed 's/^# \{0,1\}//'
			exit 0
			;;
		*) echo "未知参数: $arg" >&2; exit 2 ;;
	esac
done

CONFIG_HOME="${XDG_CONFIG_HOME:-$HOME/.config}"
DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
UNIT_DIR="$CONFIG_HOME/containers/systemd"
SYSTEMD_USER_DIR="$CONFIG_HOME/systemd/user"

OLD_NAME="felix-workstation"
NEW_NAME="felix-homelab"
OLD_POD="Felix-Workstation"

OLD_CONFIG_DIR="$CONFIG_HOME/$OLD_NAME"
NEW_CONFIG_DIR="$CONFIG_HOME/$NEW_NAME"
OLD_DATA_DIR="$DATA_HOME/$OLD_NAME"
NEW_DATA_DIR="$DATA_HOME/$NEW_NAME"

OLD_SERVICES=(
	felix-workstation-backup.service
	felix-workstation-autoheal.service
	felix-workstation-runner.service
	felix-workstation-frpc.service
	felix-workstation-homepage.service
	felix-workstation-site.service
	felix-workstation-caddy.service
	felix-workstation-forgejo.service
	felix-workstation-db.service
	felix-workstation-pod.service
)

OLD_VOLUMES=(
	felix-workstation-db-data
	felix-workstation-forgejo-data
	felix-workstation-site-data
	felix-workstation-runner-data
	felix-workstation-caddy-data
	felix-workstation-caddy-config
)

log()  { printf '\033[1;36m[migrate]\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m[migrate]\033[0m %s\n' "$*" >&2; }
die()  { printf '\033[1;31m[migrate]\033[0m %s\n' "$*" >&2; exit 1; }

# 旧部署的迹象：旧配置目录 / 旧卷 / 旧单元 / 旧 Pod 任一存在即需要迁移
need_migrate=0
[ -d "$OLD_CONFIG_DIR" ] && need_migrate=1
for v in "${OLD_VOLUMES[@]}"; do
	podman volume exists "$v" 2>/dev/null && need_migrate=1
done
podman pod exists "$OLD_POD" 2>/dev/null && need_migrate=1
[ -L "$UNIT_DIR/felix-workstation.pod" ] && need_migrate=1

if [ "$need_migrate" -eq 0 ]; then
	log "未发现 Felix-Workstation 旧命名部署，无需迁移。"
	exit 0
fi

log "以下内容将迁移到 Felix-Homelab 命名："
log "  配置目录 : $OLD_CONFIG_DIR → $NEW_CONFIG_DIR"
log "  数据目录 : $OLD_DATA_DIR → $NEW_DATA_DIR"
log "  数据卷   : felix-workstation-* → felix-homelab-*（旧卷保留）"
log "  旧 Pod   : $OLD_POD 将被移除（数据卷已复制，不会丢数据）"

if [ "$ASSUME_YES" -ne 1 ]; then
	printf '继续？输入 yes 确认: '
	read -r answer
	[ "$answer" = "yes" ] || { echo "已取消"; exit 0; }
fi

# ---------------------------------------------------------------------------
# 1. 停旧服务、禁用旧定时器
# ---------------------------------------------------------------------------
log "停止旧服务"
for svc in "${OLD_SERVICES[@]}"; do
	systemctl --user stop "$svc" 2>/dev/null || true
done

log "禁用旧备份定时器与触发单元"
systemctl --user disable --now felix-workstation-backup.timer \
	felix-workstation-backup-sync.timer \
	felix-workstation-backup-request.path \
	felix-workstation-backup-sync-request.path 2>/dev/null || true

log "移除旧单元与 drop-in"
for f in "$UNIT_DIR"/felix-workstation*; do
	[ -L "$f" ] && rm -f "$f"
done
rm -f "$SYSTEMD_USER_DIR"/felix-workstation-backup.timer \
	"$SYSTEMD_USER_DIR"/felix-workstation-backup-run.service \
	"$SYSTEMD_USER_DIR"/felix-workstation-backup-sync.timer \
	"$SYSTEMD_USER_DIR"/felix-workstation-backup-sync.service \
	"$SYSTEMD_USER_DIR"/felix-workstation-backup-request.path \
	"$SYSTEMD_USER_DIR"/felix-workstation-backup-sync-request.path \
	"$SYSTEMD_USER_DIR"/podman.service.d/10-felix-workstation.conf
rm -f "$CONFIG_HOME/containers/registries.conf.d/100-felix-workstation.conf"
systemctl --user daemon-reload

# ---------------------------------------------------------------------------
# 2. 移除旧 Pod（容器由旧单元创建，此时已完成使命）
# ---------------------------------------------------------------------------
if podman pod exists "$OLD_POD" 2>/dev/null; then
	log "移除旧 Pod $OLD_POD"
	podman pod rm -f "$OLD_POD" >/dev/null 2>&1 || true
fi

# ---------------------------------------------------------------------------
# 3. 复制数据卷
# ---------------------------------------------------------------------------
copied=0
for old in "${OLD_VOLUMES[@]}"; do
	new="${old/felix-workstation-/felix-homelab-}"
	podman volume exists "$old" 2>/dev/null || continue
	if podman volume exists "$new" 2>/dev/null; then
		warn "目标卷已存在，跳过：$new"
		continue
	fi
	if [ "$copied" -eq 0 ]; then
		log "拉取复制用镜像 alpine"
		podman pull docker.io/library/alpine:3.20
		copied=1
	fi
	log "复制卷 $old → $new"
	podman volume create "$new" >/dev/null
	# 刻意不用 :Z：:Z 会给卷内容打上该复制容器的 category 标签，
	# 正式容器（另一套 category）会被 SELinux 拒绝 setattr。
	# 关闭标签隔离执行复制，新文件继承卷根目录的通用 container_file_t 标签。
	podman run --rm --security-opt label=disable \
		-v "$old":/from -v "$new":/to \
		docker.io/library/alpine:3.20 \
		sh -c 'cd /from && tar cf - . | tar xf - -C /to'
done

# ---------------------------------------------------------------------------
# 4. 迁移目录
# ---------------------------------------------------------------------------
if [ -d "$OLD_CONFIG_DIR" ]; then
	if [ -e "$NEW_CONFIG_DIR" ]; then
		warn "$NEW_CONFIG_DIR 已存在，跳过目录迁移；请手工核对旧目录内容"
	else
		log "迁移配置目录"
		mv "$OLD_CONFIG_DIR" "$NEW_CONFIG_DIR"
	fi
fi

if [ -d "$OLD_DATA_DIR" ] && [ "$OLD_DATA_DIR" != "$NEW_DATA_DIR" ]; then
	mkdir -p "$(dirname "$NEW_DATA_DIR")"
	if [ -e "$NEW_DATA_DIR" ]; then
		warn "$NEW_DATA_DIR 已存在，跳过数据目录迁移；请手工合并 $OLD_DATA_DIR"
	else
		log "迁移数据目录（含备份归档）"
		mv "$OLD_DATA_DIR" "$NEW_DATA_DIR"
	fi
fi

# ---------------------------------------------------------------------------
# 5. 可选：删除旧卷
# ---------------------------------------------------------------------------
if [ "$PRUNE_OLD" -eq 1 ]; then
	log "删除旧数据卷"
	for old in "${OLD_VOLUMES[@]}"; do
		podman volume exists "$old" 2>/dev/null && podman volume rm -f "$old" >/dev/null 2>&1 || true
	done
fi

# ---------------------------------------------------------------------------
# 6. 后续步骤
# ---------------------------------------------------------------------------
cat <<EOF

$(printf '\033[1;32m迁移完成（数据已复制，尚未启动新服务）。\033[0m')

下一步：
  make install     # 以 Felix-Homelab 命名创建 Pod 并启动
  make register    # 重新注册 Runner（Forgejo 里旧 Runner 会显示离线，可删除）

说明：
  · 旧数据卷默认保留；确认新服务一切正常后可执行：
      podman volume rm felix-workstation-db-data ...（或重跑本脚本 --prune-old）
  · 旧备份归档 felix-ws-*.tar.gz 仍在数据目录中，恢复脚本可继续识别。
EOF
