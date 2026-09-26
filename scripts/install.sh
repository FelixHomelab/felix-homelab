#!/usr/bin/env bash
#
# Felix-Workstation 安装脚本（rootless Podman + Quadlet）
#
# 作用：
#   1. 生成 ~/.config/felix-workstation/{.env,Caddyfile,runner-config.yml}
#   2. 安装 docker.io 镜像加速配置
#   3. 将 quadlet 单元软链接到 ~/.config/containers/systemd/
#   4. 拉取镜像、启动 Pod 及核心容器
#   5. 自动注册并启动 Forgejo Actions Runner
#
set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/felix-workstation"
UNIT_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/containers/systemd"
REGISTRY_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/containers/registries.conf.d"
SYSTEMD_USER_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
BACKUP_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/felix-workstation/backups"

CORE_SERVICES=(
	felix-workstation-db.service
	felix-workstation-forgejo.service
	felix-workstation-site.service
	felix-workstation-nextcloud.service
	felix-workstation-caddy.service
	felix-workstation-homepage.service
	felix-workstation-autoheal.service
	felix-workstation-backup.service
)

log()  { printf '\033[1;36m[felix]\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m[felix]\033[0m %s\n' "$*" >&2; }
die()  { printf '\033[1;31m[felix]\033[0m %s\n' "$*" >&2; exit 1; }

gen_secret() {
	if command -v openssl >/dev/null 2>&1; then
		openssl rand -hex 16
	else
		head -c 32 /dev/urandom | od -An -tx1 | tr -d ' \n'
	fi
}

log "仓库目录: $REPO_DIR"

# ---------------------------------------------------------------------------
# 1. 目录
# ---------------------------------------------------------------------------
mkdir -p "$CONFIG_DIR" "$UNIT_DIR" "$REGISTRY_DIR" "$SYSTEMD_USER_DIR" "$BACKUP_DIR"

# ---------------------------------------------------------------------------
# 2. docker.io 镜像加速（可选，已存在则保留用户配置）
# ---------------------------------------------------------------------------
if [ ! -f "$REGISTRY_DIR/100-felix-workstation.conf" ]; then
	log "安装 docker.io 镜像加速配置"
	install -m 0644 "$REPO_DIR/config/registries.conf" \
		"$REGISTRY_DIR/100-felix-workstation.conf"
fi

# ---------------------------------------------------------------------------
# 2b. 让 Podman API 服务常驻
#
# 默认 `podman system service` 空闲 5 秒即退出，socket 文件会被删除，
# 而 Runner / Homepage 以 bind mount 方式挂载了该 socket，
# 一旦重建 inode 就会失效。这里关闭超时，保证 socket 稳定。
# ---------------------------------------------------------------------------
PODMAN_DROPIN_DIR="$SYSTEMD_USER_DIR/podman.service.d"
mkdir -p "$PODMAN_DROPIN_DIR"
cat > "$PODMAN_DROPIN_DIR/10-felix-workstation.conf" <<'EOF'
[Service]
ExecStart=
ExecStart=/usr/bin/podman $LOGGING system service --time=0
EOF
systemctl --user daemon-reload
systemctl --user stop podman.service 2>/dev/null || true
systemctl --user restart podman.socket
systemctl --user start podman.service

# ---------------------------------------------------------------------------
# 3. 环境变量（.env）
# ---------------------------------------------------------------------------
if [ ! -f "$CONFIG_DIR/.env" ]; then
	log "生成 $CONFIG_DIR/.env"
	install -m 0600 "$REPO_DIR/.env.example" "$CONFIG_DIR/.env"
	PW="$(gen_secret)"
	sed -i "s/^POSTGRES_PASSWORD=.*/POSTGRES_PASSWORD=$PW/" "$CONFIG_DIR/.env"
	sed -i "s/^FORGEJO__database__PASSWD=.*/FORGEJO__database__PASSWD=$PW/" "$CONFIG_DIR/.env"
	APW="$(gen_secret)"
	sed -i "s/^ADMIN_PASSWORD=.*/ADMIN_PASSWORD=$APW/" "$CONFIG_DIR/.env"
	log "个人主页站长密码（ADMIN_PASSWORD）: $APW"
else
	log "保留已存在的 .env"
fi

# 确保关键配置存在（便于旧版本升级）
grep -q '^FORGEJO__security__INSTALL_LOCK=' "$CONFIG_DIR/.env" \
	|| echo 'FORGEJO__security__INSTALL_LOCK=true' >> "$CONFIG_DIR/.env"

# Nextcloud 环境变量（首次生成随机密码）
if [ ! -f "$CONFIG_DIR/nextcloud.env" ]; then
	log "生成 $CONFIG_DIR/nextcloud.env"
	install -m 0600 "$REPO_DIR/config/nextcloud.env.example" "$CONFIG_DIR/nextcloud.env"
	NPW="$(gen_secret)"
	sed -i "s/^POSTGRES_PASSWORD=.*/POSTGRES_PASSWORD=$NPW/" "$CONFIG_DIR/nextcloud.env"
	sed -i "s/^NEXTCLOUD_ADMIN_PASSWORD=.*/NEXTCLOUD_ADMIN_PASSWORD=$NPW/" "$CONFIG_DIR/nextcloud.env"
	log "Nextcloud 管理员密码（NEXTCLOUD_ADMIN_PASSWORD）: $NPW"
fi

# 从旧版端口方案（8080/3000/2222）迁移到 5729 起
sed -i \
	-e 's|^FORGEJO__server__ROOT_URL=http://localhost:8080/$|FORGEJO__server__ROOT_URL=http://localhost:5730/|' \
	-e 's|^FORGEJO__server__SSH_PORT=2222$|FORGEJO__server__SSH_PORT=5731|' \
	"$CONFIG_DIR/.env"

# ---------------------------------------------------------------------------
# 4. 其它配置
# ---------------------------------------------------------------------------
log "安装 Caddyfile"
install -m 0644 "$REPO_DIR/config/Caddyfile" "$CONFIG_DIR/Caddyfile"

# Homepage 配置仅在首次安装时植入，之后保留用户自定义
if [ ! -d "$CONFIG_DIR/homepage" ]; then
	log "植入 Homepage 配置"
	cp -r "$REPO_DIR/config/homepage" "$CONFIG_DIR/homepage"
fi

# 图标资源属于静态资源，随仓库同步更新
if [ -d "$REPO_DIR/config/homepage/icons" ]; then
	mkdir -p "$CONFIG_DIR/homepage/icons"
	cp -f "$REPO_DIR"/config/homepage/icons/* "$CONFIG_DIR/homepage/icons/" 2>/dev/null || true
fi

# Runner 标签定义仅在首次安装时植入
if [ ! -f "$CONFIG_DIR/runner-labels.txt" ]; then
	install -m 0644 "$REPO_DIR/config/runner-labels.txt" "$CONFIG_DIR/runner-labels.txt"
fi

# 备份脚本随仓库同步（项目托管，非用户自定义）
mkdir -p "$CONFIG_DIR/backup" "$CONFIG_DIR/rclone" "$CONFIG_DIR/sync"
for script in backup.sh backup-nextcloud.sh; do
	install -m 0755 "$REPO_DIR/config/backup/$script" "$CONFIG_DIR/backup/$script"
done
install -m 0755 "$REPO_DIR/scripts/sync-backup.sh" "$CONFIG_DIR/backup/sync-backup.sh"

# 备份配置仅在首次安装时植入，之后保留用户配置
# （sync/ 整目录挂进容器；整个目录挂载可避免宿主机替换文件后容器读到旧 inode）
if [ ! -f "$CONFIG_DIR/sync/backup.conf" ]; then
	install -m 0644 "$REPO_DIR/config/backup/backup.conf.example" "$CONFIG_DIR/sync/backup.conf"
fi

# 后台触发用的请求文件（Path 单元监视；必须先存在，否则会被建成目录）
: >"$CONFIG_DIR/sync/backup-request"
: >"$CONFIG_DIR/sync/sync-request"

# --- 备份调度与触发（宿主机 systemd user 单元）---
# 定时：每天 03:00 备份（Persistent 错过后补跑），04:00 同步到各渠道
# 手动：后台写 backup-request / sync-request，由 .path 单元触发
cat >"$SYSTEMD_USER_DIR/felix-workstation-backup-run.service" <<'EOF'
[Unit]
Description=Felix-Workstation: 执行一次备份（Forgejo/主站 + Nextcloud）
After=felix-workstation-backup.service felix-workstation-forgejo.service felix-workstation-db.service
Requires=felix-workstation-backup.service

[Service]
Type=oneshot
ExecStart=/usr/bin/podman exec felix-workstation-backup sh /usr/local/bin/backup.sh once
ExecStart=%h/.config/felix-workstation/backup/backup-nextcloud.sh
EOF

cat >"$SYSTEMD_USER_DIR/felix-workstation-backup.timer" <<'EOF'
[Unit]
Description=Felix-Workstation: 每日备份

[Timer]
OnCalendar=*-*-* 03:00:00
Persistent=true
RandomizedDelaySec=5m
Unit=felix-workstation-backup-run.service

[Install]
WantedBy=timers.target
EOF

cat >"$SYSTEMD_USER_DIR/felix-workstation-backup-request.path" <<'EOF'
[Unit]
Description=Felix-Workstation: 后台请求备份

[Path]
PathChanged=%h/.config/felix-workstation/sync/backup-request
Unit=felix-workstation-backup-run.service

[Install]
WantedBy=paths.target
EOF

cat >"$SYSTEMD_USER_DIR/felix-workstation-backup-sync.service" <<'EOF'
[Unit]
Description=Felix-Workstation: 同步备份到各渠道
[Service]
Type=oneshot
ExecStart=%h/.config/felix-workstation/backup/sync-backup.sh
EOF

cat >"$SYSTEMD_USER_DIR/felix-workstation-backup-sync.timer" <<'EOF'
[Unit]
Description=Felix-Workstation: 每日同步备份

[Timer]
OnCalendar=*-*-* 04:00:00
Persistent=true
RandomizedDelaySec=10m
Unit=felix-workstation-backup-sync.service

[Install]
WantedBy=timers.target
EOF

cat >"$SYSTEMD_USER_DIR/felix-workstation-backup-sync-request.path" <<'EOF'
[Unit]
Description=Felix-Workstation: 后台请求同步备份

[Path]
PathChanged=%h/.config/felix-workstation/sync/sync-request
Unit=felix-workstation-backup-sync.service

[Install]
WantedBy=paths.target
EOF

systemctl --user daemon-reload
systemctl --user enable --now felix-workstation-backup.timer >/dev/null
systemctl --user enable --now felix-workstation-backup-sync.timer >/dev/null
systemctl --user enable --now felix-workstation-backup-request.path >/dev/null
systemctl --user enable --now felix-workstation-backup-sync-request.path >/dev/null

# runner 配置在注册前先放一个空文件，保证挂载目标存在
[ -f "$CONFIG_DIR/runner-config.yml" ] || : > "$CONFIG_DIR/runner-config.yml"

# ---------------------------------------------------------------------------
# 5. Quadlet 单元软链接
# ---------------------------------------------------------------------------
log "链接 Quadlet 单元到 $UNIT_DIR"
# runner 单元在注册时（register-runner.sh）才链接并启动，
# 避免 pod 在 runner 尚未注册时反复拉起它。
for f in "$REPO_DIR"/quadlet/*; do
	case "$(basename "$f")" in
		felix-workstation-runner.container) continue ;;
	esac
	ln -sfn "$f" "$UNIT_DIR/$(basename "$f")"
done

# ---------------------------------------------------------------------------
# 6. 预拉取镜像（走镜像加速）
# ---------------------------------------------------------------------------
log "预拉取镜像（可能需要几分钟）"
podman pull docker.io/library/postgres:17-alpine
podman pull codeberg.org/forgejo/forgejo:16
podman pull docker.io/library/caddy:2
podman pull data.forgejo.org/forgejo/runner:13
podman pull ghcr.io/gethomepage/homepage:v2.4.0
podman pull docker.io/willfarrell/autoheal:latest
podman pull docker.io/library/nextcloud:apache

# 个人主页镜像：本地构建（Leptos 首次编译较慢，仅在缺失时构建）
if ! podman image exists localhost/felix-homepage:latest; then
	log "构建个人主页镜像（首次需要编译 Rust，可能较久）"
	"$REPO_DIR/scripts/build-site.sh"
fi

# ---------------------------------------------------------------------------
# 7. 启动
# ---------------------------------------------------------------------------
log "reload systemd --user"
systemctl --user daemon-reload

log "启动数据库并准备 Nextcloud 库"
systemctl --user start felix-workstation-db.service
/usr/bin/podman wait --condition=healthy felix-workstation-db
"$REPO_DIR/scripts/ensure-nextcloud-db.sh"

log "启动核心服务"
# Quadlet 会把带 [Install] 的单元自动挂到 default.target.wants，
# 因此无需 systemctl enable，直接 start 即可，开机也会自启。
systemctl --user start "${CORE_SERVICES[@]}"

log "等待 Forgejo 就绪..."
"$REPO_DIR/scripts/wait-for-forgejo.sh"

# Homepage 挂载了 podman.sock；升级安装时重启它以刷新 socket inode
systemctl --user try-restart felix-workstation-homepage.service 2>/dev/null || true

# ---------------------------------------------------------------------------
# 8. 注册 Actions Runner
# ---------------------------------------------------------------------------
log "注册 Forgejo Actions Runner"
"$REPO_DIR/scripts/register-runner.sh"

printf '\n\033[1;32mFelix-Workstation 部署完成！\033[0m\n'
cat <<EOF
  首页 Homepage : http://localhost:5729/   (直连 http://localhost:5732/)
  Forgejo Web   : http://localhost:5730/   (Caddy: http://forgejo.localhost:5729/)
  Forgejo SSH   : ssh -p 5731 git@localhost
  数据目录      : $CONFIG_DIR
  单元目录      : $UNIT_DIR

若是全新实例：打开 http://localhost:5730/ 注册第一个账号（该账号将成为管理员）。
若为恢复的实例：直接用原有账号登录。
EOF
