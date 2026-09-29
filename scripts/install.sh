#!/usr/bin/env bash
#
# Felix-Homelab 安装脚本（rootless Podman + Quadlet）
#
# 作用：
#   1. 生成 ~/.config/felix-homelab/{.env,Caddyfile,runner-config.yml}
#   2. 安装 docker.io 镜像加速配置
#   3. 将 quadlet 单元软链接到 ~/.config/containers/systemd/
#   4. 拉取镜像、启动 Pod 及核心容器
#   5. 自动注册并启动 Forgejo Actions Runner
#
set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/felix-homelab"
UNIT_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/containers/systemd"
REGISTRY_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/containers/registries.conf.d"
SYSTEMD_USER_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
BACKUP_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/felix-homelab/backups"

CORE_SERVICES=(
	felix-homelab-db.service
	felix-homelab-forgejo.service
	felix-homelab-site.service
	felix-homelab-caddy.service
	felix-homelab-agent-gateway.service
	felix-homelab-homepage.service
	felix-homelab-autoheal.service
	felix-homelab-backup.service
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

# 旧命名（Felix-Workstation）部署必须先迁移，否则会以空数据启动新实例
OLD_CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/felix-workstation"
if [ ! -d "$CONFIG_DIR" ] && [ -d "$OLD_CONFIG_DIR" ]; then
	die "检测到旧命名配置 $OLD_CONFIG_DIR：请先执行 scripts/migrate-rename.sh 迁移到 felix-homelab"
fi

# ---------------------------------------------------------------------------
# 1. 目录
# ---------------------------------------------------------------------------
mkdir -p "$CONFIG_DIR" "$UNIT_DIR" "$REGISTRY_DIR" "$SYSTEMD_USER_DIR" "$BACKUP_DIR"

# ---------------------------------------------------------------------------
# 2. docker.io 镜像加速（可选，已存在则保留用户配置）
# ---------------------------------------------------------------------------
if [ ! -f "$REGISTRY_DIR/100-felix-homelab.conf" ]; then
	log "安装 docker.io 镜像加速配置"
	install -m 0644 "$REPO_DIR/config/registries.conf" \
		"$REGISTRY_DIR/100-felix-homelab.conf"
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
cat > "$PODMAN_DROPIN_DIR/10-felix-homelab.conf" <<'EOF'
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
	log "站长密码（ADMIN_PASSWORD）: $APW"
else
	log "保留已存在的 .env"
fi

# 确保关键配置存在（便于旧版本升级）
grep -q '^FORGEJO__security__INSTALL_LOCK=' "$CONFIG_DIR/.env" \
	|| echo 'FORGEJO__security__INSTALL_LOCK=true' >> "$CONFIG_DIR/.env"

# 多租户 Agent 配置（老安装补默认值；已存在则保留用户修改）
ensure_env_default() {
	grep -q "^$1=" "$CONFIG_DIR/.env" || echo "$1=$2" >> "$CONFIG_DIR/.env"
}
ensure_env_default AGENT_IMAGE localhost/felix-agent-opencode:latest
ensure_env_default AGENT_DSH_IMAGE localhost/felix-agent-dsh:latest
ensure_env_default AGENT_BASE_DOMAIN agent.grantfelix.top
ensure_env_default AGENT_LOCAL_DOMAIN agent.localhost
ensure_env_default AGENT_PORT_BASE 20001
ensure_env_default AGENT_PORT_MAX 20099
ensure_env_default AGENT_MEMORY 2g
ensure_env_default AGENT_CPUS 1.5
ensure_env_default AGENT_PIDS_LIMIT 512
ensure_env_default AGENT_IDLE_SECONDS 300
ensure_env_default AGENT_GRACE_DAYS 30
ensure_env_default AGENT_RECREATE_DAYS 7
ensure_env_default AGENT_IMAGE_KEEP 3
ensure_env_default AGENT_VNC_PORT_BASE 21001
ensure_env_default OPENCODE_VERSION 2.0.18
ensure_env_default AGENT_CACHE_OPTIMIZER_REF CFG="${XDG_CONFIG_HOME:-$HOME/.config}/opencode/opencode.json"
# 插件源码随镜像烤在 /opt（fork 固定提交）；但 OpenCode 的路径插件只加载
# 项目/家目录下的目录（/opt 会被静默跳过），因此按镜像版本戳种子到 $HOME 再引用。
SRC="/opt/agent-cache-optimizer"
PLUGIN="$HOME/.local/share/opencode-plugins/agent-cache-optimizer"
if [ -d "$SRC" ]; then
	want="$(cat "$SRC/.felix-rev" 2>/dev/null || echo unknown)"
	have="$(cat "$PLUGIN/.felix-rev" 2>/dev/null || echo none)"
	if [ "$want" != "$have" ] || [ ! -f "$PLUGIN/index.ts" ]; then
		rm -rf "$PLUGIN"
		mkdir -p "$(dirname "$PLUGIN")"
		cp -r "$SRC" "$PLUGIN"
	fi
fi
ensure_env_default DSH_VERSION 0.1.7-rc.2
ensure_env_default DSHMARKET_VERSION 1.66.3
ensure_env_default DSHGUARDIAN_REF 1bca78ed0329fa921e447ea12eedbcf3990e1179
ensure_env_default DSHCOSTMETER_VERSION 1.7.40
ensure_env_default OPENCODE2DSH_REF d5e866ae77b30d66212a7cfb87921a870a3bdc54
ensure_env_default DSHBETTERSIDEBAR_VERSION 0.22.1
ensure_env_default DSHEGOBROWSER_REF 3fc4245d9b95b1eb2cfe540e32d04c363ba9ca31
ensure_env_default OBSCURA_VERSION v0.2.3
ensure_env_default DSHDEV_RULES_REF 83c5ff329a1ecb9e8dc37da02eee17998f904dee
# 会话 cookie 的共享父域（Agent 子域 SSO；站点按请求 Host 自适应）
ensure_env_default COOKIE_DOMAIN grantfelix.top

# frpc 配置（可选：经云服务器中转）。仅在首次安装时植入模板；
# 未填写 serverAddr/token 前不会链接并启动 frpc 单元（见第 5 节）。
mkdir -p "$CONFIG_DIR/frp"
if [ ! -f "$CONFIG_DIR/frp/frpc.toml" ]; then
	log "植入 frpc 配置模板 $CONFIG_DIR/frp/frpc.toml（可选，启用公网中转用）"
	install -m 0600 "$REPO_DIR/config/frpc.toml.example" "$CONFIG_DIR/frp/frpc.toml"
fi

# Agent 网关隧道配置：从 frpc.toml 同步 serverAddr/serverPort/token，
# 只多一条 20081 → 5740（Agent 网关）的隧道。
install -m 0600 "$REPO_DIR/config/frpc-agent.toml.example" "$CONFIG_DIR/frp/frpc-agent.toml"
if ! grep -q 'CHANGE_ME' "$CONFIG_DIR/frp/frpc.toml" 2>/dev/null; then
	addr="$(sed -n 's/^serverAddr[[:space:]]*=[[:space:]]*"\(.*\)"/\1/p' "$CONFIG_DIR/frp/frpc.toml" | head -1)"
	port="$(sed -n 's/^serverPort[[:space:]]*=[[:space:]]*\([0-9]*\).*/\1/p' "$CONFIG_DIR/frp/frpc.toml" | head -1)"
	token="$(sed -n 's/^auth\.token[[:space:]]*=[[:space:]]*"\(.*\)"/\1/p' "$CONFIG_DIR/frp/frpc.toml" | head -1)"
	[ -n "$addr" ] && sed -i "s|^serverAddr = .*|serverAddr = \"$addr\"|" "$CONFIG_DIR/frp/frpc-agent.toml"
	[ -n "$port" ] && sed -i "s|^serverPort = .*|serverPort = $port|" "$CONFIG_DIR/frp/frpc-agent.toml"
	[ -n "$token" ] && sed -i "s|^auth\.token = .*|auth.token = \"$token\"|" "$CONFIG_DIR/frp/frpc-agent.toml"
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

# Agent 网关（host 网络 Caddy）配置
mkdir -p "$CONFIG_DIR/agent-gateway"
install -m 0644 "$REPO_DIR/config/agent-gateway/Caddyfile" "$CONFIG_DIR/agent-gateway/Caddyfile"

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
for script in backup.sh; do
	install -m 0755 "$REPO_DIR/config/backup/$script" "$CONFIG_DIR/backup/$script"
done
install -m 0755 "$REPO_DIR/scripts/sync-backup.sh" "$CONFIG_DIR/backup/sync-backup.sh"

# 备份配置仅在首次安装时植入，之后保留用户配置
# （sync/ 整目录挂进容器；整个目录挂载可避免宿主机替换文件后容器读到旧 inode）
if [ ! -f "$CONFIG_DIR/sync/backup.conf" ]; then
	install -m 0644 "$REPO_DIR/config/backup/backup.conf.example" "$CONFIG_DIR/sync/backup.conf"
fi

# —— 多租户 AI Agent（独立 bridge 网络 + Agent 网关）——
# 配置目录：state.json（宿主）、status.json（回写后台）、requests/（后台请求）、
# caddy/（每用户路由，同时被 Agent 网关容器挂载）
mkdir -p "$CONFIG_DIR/agents/caddy" "$CONFIG_DIR/agents/requests"
if [ ! -f "$CONFIG_DIR/agents/state.json" ]; then
	printf '{"agents":{}}\n' >"$CONFIG_DIR/agents/state.json"
fi
chmod 600 "$CONFIG_DIR/agents/state.json"
if [ ! -f "$CONFIG_DIR/agents/caddy/00-empty.caddy" ]; then
	printf '# 占位文件：各用户路由由 agent-ctl.sh 生成；保留它是为了让 Caddy 的 glob import 永远有匹配。\n' \
		>"$CONFIG_DIR/agents/caddy/00-empty.caddy"
fi
# 执行面脚本复制到配置目录（systemd 单元引用它，不依赖仓库路径）
install -m 0755 "$REPO_DIR/scripts/agent-ctl.sh" "$CONFIG_DIR/agents/agent-ctl.sh"

# 后台触发用的请求文件（Path 单元监视；必须先存在，否则会被建成目录）
: >"$CONFIG_DIR/sync/backup-request"
: >"$CONFIG_DIR/sync/sync-request"

# --- 备份调度与触发（宿主机 systemd user 单元）---
# 定时：每天 03:00 备份（Persistent 错过后补跑），04:00 同步到各渠道
# 手动：后台写 backup-request / sync-request，由 .path 单元触发
cat >"$SYSTEMD_USER_DIR/felix-homelab-backup-run.service" <<'EOF'
[Unit]
Description=Felix-Homelab: 执行一次备份（Forgejo/主站）
After=felix-homelab-backup.service felix-homelab-forgejo.service felix-homelab-db.service
Requires=felix-homelab-backup.service

[Service]
Type=oneshot
ExecStart=/usr/bin/podman exec felix-homelab-backup sh /usr/local/bin/backup.sh once
EOF

cat >"$SYSTEMD_USER_DIR/felix-homelab-backup.timer" <<'EOF'
[Unit]
Description=Felix-Homelab: 每日备份

[Timer]
OnCalendar=*-*-* 03:00:00
Persistent=true
RandomizedDelaySec=5m
Unit=felix-homelab-backup-run.service

[Install]
WantedBy=timers.target
EOF

cat >"$SYSTEMD_USER_DIR/felix-homelab-backup-request.path" <<'EOF'
[Unit]
Description=Felix-Homelab: 后台请求备份

[Path]
PathChanged=%h/.config/felix-homelab/sync/backup-request
Unit=felix-homelab-backup-run.service

[Install]
WantedBy=paths.target
EOF

cat >"$SYSTEMD_USER_DIR/felix-homelab-backup-sync.service" <<'EOF'
[Unit]
Description=Felix-Homelab: 同步备份到各渠道
[Service]
Type=oneshot
ExecStart=%h/.config/felix-homelab/backup/sync-backup.sh
EOF

cat >"$SYSTEMD_USER_DIR/felix-homelab-backup-sync.timer" <<'EOF'
[Unit]
Description=Felix-Homelab: 每日同步备份

[Timer]
OnCalendar=*-*-* 04:00:00
Persistent=true
RandomizedDelaySec=10m
Unit=felix-homelab-backup-sync.service

[Install]
WantedBy=timers.target
EOF

cat >"$SYSTEMD_USER_DIR/felix-homelab-backup-sync-request.path" <<'EOF'
[Unit]
Description=Felix-Homelab: 后台请求同步备份

[Path]
PathChanged=%h/.config/felix-homelab/sync/sync-request
Unit=felix-homelab-backup-sync.service

[Install]
WantedBy=paths.target
EOF

# --- 多租户 Agent：请求触发 + 定时保活 ---
cat >"$SYSTEMD_USER_DIR/felix-homelab-agent-run.service" <<'EOF'
[Unit]
Description=Felix-Homelab: 处理 Agent 请求并保活
After=felix-homelab-pod.service
Wants=felix-homelab-pod.service

[Service]
Type=oneshot
# 本服务会 `podman run/start` 用户 Agent 容器；默认 KillMode=control-group 会在
# 服务退出时把同一 cgroup 里新建的容器一并杀掉（表现为容器 exit 130）。必须关闭。
KillMode=none
ExecStart=%h/.config/felix-homelab/agents/agent-ctl.sh tick
EOF

cat >"$SYSTEMD_USER_DIR/felix-homelab-agent-request.path" <<'EOF'
[Unit]
Description=Felix-Homelab: 监视 Agent 请求目录

[Path]
PathChanged=%h/.config/felix-homelab/agents/requests
Unit=felix-homelab-agent-run.service

[Install]
WantedBy=paths.target
EOF

cat >"$SYSTEMD_USER_DIR/felix-homelab-agent.timer" <<'EOF'
[Unit]
Description=Felix-Homelab: 定时保活 Agent 容器

[Timer]
OnBootSec=2min
OnCalendar=*:0/2
Persistent=false
RandomizedDelaySec=30s
Unit=felix-homelab-agent-run.service

[Install]
WantedBy=timers.target
EOF

systemctl --user daemon-reload
systemctl --user enable --now felix-homelab-backup.timer >/dev/null
systemctl --user enable --now felix-homelab-backup-sync.timer >/dev/null
systemctl --user enable --now felix-homelab-backup-request.path >/dev/null
systemctl --user enable --now felix-homelab-backup-sync-request.path >/dev/null
systemctl --user enable --now felix-homelab-agent-request.path >/dev/null
systemctl --user enable --now felix-homelab-agent.timer >/dev/null

# runner 配置在注册前先放一个空文件，保证挂载目标存在
[ -f "$CONFIG_DIR/runner-config.yml" ] || : > "$CONFIG_DIR/runner-config.yml"

# ---------------------------------------------------------------------------
# 5. Quadlet 单元软链接
# ---------------------------------------------------------------------------
log "链接 Quadlet 单元到 $UNIT_DIR"
# runner 单元在注册时（register-runner.sh）才链接并启动，
# 避免 pod 在 runner 尚未注册时反复拉起它。
# frpc 同理：云服务器地址/token 未配置前不链接，避免容器反复重启。
for f in "$REPO_DIR"/quadlet/*; do
	case "$(basename "$f")" in
		felix-homelab-runner.container)
			continue
			;;
		felix-homelab-frpc.container | felix-homelab-agent-frpc.container)
			if ! grep -q 'CHANGE_ME' "$CONFIG_DIR/frp/frpc.toml" 2>/dev/null; then
				ln -sfn "$f" "$UNIT_DIR/$(basename "$f")"
			else
				rm -f "$UNIT_DIR/$(basename "$f")"
				warn "跳过 $(basename "$f")：请先编辑 $CONFIG_DIR/frp/frpc.toml（serverAddr/token）后重跑 make install"
			fi
			continue
			;;
	esac
	ln -sfn "$f" "$UNIT_DIR/$(basename "$f")"
done

# 已配置 frpc 时纳入本次启动列表（含 Agent 网关隧道）
if [ -L "$UNIT_DIR/felix-homelab-frpc.container" ]; then
	CORE_SERVICES+=(felix-homelab-frpc.service)
fi
if [ -L "$UNIT_DIR/felix-homelab-agent-frpc.container" ]; then
	CORE_SERVICES+=(felix-homelab-agent-frpc.service)
fi

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

# frpc 镜像仅在启用公网中转（已链接 frpc 单元）时拉取
if [ -L "$UNIT_DIR/felix-homelab-frpc.container" ]; then
	podman pull docker.io/snowdreamtech/frpc:0.71.0-alpine
fi

# 主站镜像：本地构建（Leptos 首次编译较慢，仅在缺失时构建）
if ! podman image exists localhost/felix-homelab-site:latest; then
	log "构建主站镜像（首次需要编译 Rust，可能较久）"
	"$REPO_DIR/scripts/build-site.sh"
fi

# ---------------------------------------------------------------------------
# 7. 启动
# ---------------------------------------------------------------------------
log "reload systemd --user"
systemctl --user daemon-reload

log "启动数据库"
systemctl --user start felix-homelab-db.service
/usr/bin/podman wait --condition=healthy felix-homelab-db

log "启动核心服务"
# Quadlet 会把带 [Install] 的单元自动挂到 default.target.wants，
# 因此无需 systemctl enable，直接 start 即可，开机也会自启。
systemctl --user start "${CORE_SERVICES[@]}"

log "等待 Forgejo 就绪..."
"$REPO_DIR/scripts/wait-for-forgejo.sh"

# Homepage 挂载了 podman.sock；升级安装时重启它以刷新 socket inode
systemctl --user try-restart felix-homelab-homepage.service 2>/dev/null || true
# Caddyfile 是单文件 bind mount，install 替换后 inode 会变；Agent 网关同理
# （主 Caddy 已不再承载 Agent 路由，这里顺带重启网关以载入新 Caddyfile）
systemctl --user try-restart felix-homelab-caddy.service 2>/dev/null || true
systemctl --user try-restart felix-homelab-agent-gateway.service 2>/dev/null || true

# ---------------------------------------------------------------------------
# 8. 注册 Actions Runner
# ---------------------------------------------------------------------------
log "注册 Forgejo Actions Runner"
"$REPO_DIR/scripts/register-runner.sh"

printf '\n\033[1;32mFelix-Homelab 部署完成！\033[0m\n'
cat <<EOF
  主站(社区站)  : http://localhost:5729/   (直连 http://localhost:5733/)
  Forgejo Web   : http://localhost:5730/   (Caddy: http://forgejo.localhost:5729/)
  Forgejo SSH   : ssh -p 5731 git@localhost
  公网访问      : 配置 frpc 后经云域名访问（见 README「公网访问（云服务器中转）」）
  数据目录      : $CONFIG_DIR
  单元目录      : $UNIT_DIR

若是全新实例：打开 http://localhost:5730/ 注册第一个账号（该账号将成为管理员）。
若为恢复的实例：直接用原有账号登录。
EOF
