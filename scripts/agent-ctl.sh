#!/usr/bin/env bash
#
# Felix-Homelab 多租户 AI Agent 编排（P1 试点）
#
# 角色：站点是控制面（订阅表 + 鉴权 + 后台/首屏），本脚本是执行面。
#   · systemd .path 监视 $CONFIG_DIR/agents/requests/，有请求就执行；
#   · systemd .timer 每 10 分钟 reconcile 一次（崩溃/重启后保活）；
#   · 每个「用户 + 实例号(slot)」一个独立容器（模板镜像 + 独立数据卷 + 独立子域），
#     通过 Caddy 子域 + 主站会话鉴权访问。
#
# 用法：
#   agent-ctl.sh build [kind]              构建模板镜像（默认 opencode；kind=dsh）
#   agent-ctl.sh grant <user> [kind] [slot] 创建并启动（授权仍以站点后台为准）
#   agent-ctl.sh start|stop|remove <user> [slot]
#   agent-ctl.sh setkey <user> [slot] [KEY=VALUE ...]  写 DSH 密钥（只写不读值）
#   agent-ctl.sh apply                      处理请求目录 + reconcile + 刷新状态
#   agent-ctl.sh tick                       同上（供 timer 调用）
#   agent-ctl.sh list                       列出 Agent
#   agent-ctl.sh versions                   列出版本标签（回滚用）与在用实例
#   agent-ctl.sh pin <kind> <版本|latest>    固定模板镜像版本（写回 .env）
#   agent-ctl.sh recreate-all <kind|all>     按当前固定版本重建实例（换镜像）
#   agent-ctl.sh sync-routes                按 state 重写全部 Caddy 路由
#
set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/felix-homelab"
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/felix-homelab"

AGENTS_DIR="$CONFIG_DIR/agents"
STATE_FILE="$AGENTS_DIR/state.json"
STATUS_FILE="$AGENTS_DIR/status.json"
REQUESTS_DIR="$AGENTS_DIR/requests"
CADDY_DIR="$AGENTS_DIR/caddy"
WORK_ROOT="$DATA_DIR/agents"

# shellcheck disable=SC1091
if [ -f "$CONFIG_DIR/.env" ]; then
	set -a
	# .env 是 KEY=value 纯文本（密码为随机 hex），可直接 source
	. "$CONFIG_DIR/.env"
	set +a
fi

AGENT_IMAGE="${AGENT_IMAGE:-localhost/felix-agent-opencode:latest}"
AGENT_DSH_IMAGE="${AGENT_DSH_IMAGE:-localhost/felix-agent-dsh:latest}"
AGENT_BASE_DOMAIN="${AGENT_BASE_DOMAIN:-wraindrock.com}"
AGENT_LOCAL_DOMAIN="${AGENT_LOCAL_DOMAIN:-agent.localhost}"
AGENT_PORT_BASE="${AGENT_PORT_BASE:-20001}"
AGENT_PORT_MAX="${AGENT_PORT_MAX:-20099}"
AGENT_MEMORY="${AGENT_MEMORY:-2g}"
AGENT_CPUS="${AGENT_CPUS:-1.5}"
AGENT_PIDS_LIMIT="${AGENT_PIDS_LIMIT:-512}"
# 站点容器在宿主回环上的直连端口（供外置 Agent 网关 forward_auth 调用）
AGENT_SITE_PORT="${AGENT_SITE_PORT:-5735}"
# Agent 网关（host 网络 Caddy）单元与容器：每用户路由由它承载
GATEWAY_UNIT="${GATEWAY_UNIT:-felix-homelab-agent-gateway.service}"
GATEWAY_CONTAINER="${GATEWAY_CONTAINER:-felix-homelab-agent-gateway}"
# 每个 Agent 独立 bridge 网络（felix-agent-<slug>）
AGENT_NET_PREFIX="${AGENT_NET_PREFIX:-felix-agent-}"
# 定期重建容器（清掉可写层里的持久化改动；数据卷/工作区保留）
AGENT_RECREATE_DAYS="${AGENT_RECREATE_DAYS:-7}"
# noVNC 端口基址（DSH 实例才有）：实际端口 = 基址 + (agent 端口 - AGENT_PORT_BASE)
AGENT_VNC_PORT_BASE="${AGENT_VNC_PORT_BASE:-21001}"
# 默认丢弃的危险能力（rootless 下本就无法取得宿主特权，这里进一步收窄攻击面）
AGENT_CAP_DROP="${AGENT_CAP_DROP:-SYS_ADMIN,SYS_MODULE,SYS_RAWIO,SYS_PTRACE,SYS_BOOT,MKNOD,NET_ADMIN,AUDIT_WRITE,AUDIT_READ,WAKE_ALARM,SYS_TIME,SYS_TTY_CONFIG}"
# 空闲睡眠：无活动超过该秒数则停容器（desired=sleeping，下次访问自动唤醒）
AGENT_IDLE_SECONDS="${AGENT_IDLE_SECONDS:-300}"
# 撤销后保留数据+域名多久（天），期间续期原样复活
AGENT_GRACE_DAYS="${AGENT_GRACE_DAYS:-30}"
ACTIVITY_DIR="$AGENTS_DIR/activity"

# 路由文件内容是否发生变化（决定是否需要重启网关；start/唤醒不该重启）
ROUTE_CHANGED=0

log()  { printf '\033[1;36m[agent]\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m[agent]\033[0m %s\n' "$*" >&2; }
die()  { printf '\033[1;31m[agent]\033[0m %s\n' "$*" >&2; exit 1; }

slugify() { printf '%s' "$1" | tr '[:upper:]' '[:lower:]' | tr '_' '-'; }
# 实例状态键：slot 1 沿用用户名，slot≥2 用 user#slot
state_key() {
	local slot="${2:-1}"
	if [ "$slot" -gt 1 ]; then
		printf '%s#%s' "$1" "$slot"
	else
		printf '%s' "$1"
	fi
}
container_of() { printf 'felix-agent-%s' "$(slug_of "$1")"; }

# 模板对应的域名段（与站点保持一致的命名）
# 本地生成子域（CLI 用；正常由站点生成并随请求传入）。
# 与站点一致：18 位随机码，一级标签（`<码>.wraindrock.com`）。
gen_subdomain() {
	printf '%s' "$(openssl rand -hex 9)"
}

# 读取某个状态键的字段（键可能是 user 或 user#slot）
slug_of() {
	local slug
	slug="$(state_field "$1" slug)"
	[ -n "$slug" ] || die "state 缺少 $1 的 slug"
	printf '%s' "$slug"
}

ensure_layout() {
	mkdir -p "$AGENTS_DIR" "$REQUESTS_DIR" "$CADDY_DIR" "$WORK_ROOT" "$ACTIVITY_DIR"
	if [ ! -f "$STATE_FILE" ]; then
		printf '{"agents":{}}\n' >"$STATE_FILE"
	fi
	chmod 600 "$STATE_FILE"
	if [ ! -f "$CADDY_DIR/00-empty.caddy" ]; then
		printf '# 占位文件：各用户路由由 agent-ctl.sh 生成；保留它是为了让 Caddy 的 glob import 永远有匹配。\n' \
			>"$CADDY_DIR/00-empty.caddy"
	fi
}

# ---------------------------------------------------------------------------
# state.json 读写（Python 标准库；文件仅宿主可读，含每用户网关口令）
# ---------------------------------------------------------------------------

state_upsert() {
	local username="$1" slot="$2" slug="$3" kind="$4" port="$5" password="$6" subdomain="${7:-}"
	python3 - "$STATE_FILE" "$username" "$slot" "$slug" "$kind" "$port" "$password" "$subdomain" <<'PY'
import json, os, sys, tempfile, time
path, username, slot, slug, kind, port, password, subdomain = sys.argv[1:9]
try:
    data = json.load(open(path))
except Exception:
    data = {"agents": {}}
agents = data.setdefault("agents", {})
key = username if int(slot) <= 1 else f"{username}#{slot}"
entry = agents.get(key, {})
now = time.strftime("%Y-%m-%dT%H:%M:%S")
entry.update({"username": username, "slot": int(slot), "slug": slug, "kind": kind,
              "port": int(port), "password": password, "updated_at": now})
if subdomain:
    entry["subdomain"] = subdomain
entry.pop("removed_at", None)
entry.setdefault("created_at", now)
agents[key] = entry
data["agents"] = agents
fd, tmp = tempfile.mkstemp(dir=os.path.dirname(path))
with os.fdopen(fd, "w") as handle:
    json.dump(data, handle, ensure_ascii=False, indent=2)
os.replace(tmp, path)
PY
	chmod 600 "$STATE_FILE"
}

state_set_kind() {
	python3 - "$STATE_FILE" "$1" "$2" <<'PY'
import json, os, sys, tempfile
path, key, kind = sys.argv[1:4]
data = json.load(open(path))
entry = data.setdefault("agents", {}).get(key)
if entry is None:
    sys.exit("state 中没有该项")
entry["kind"] = kind
fd, tmp = tempfile.mkstemp(dir=os.path.dirname(path))
with os.fdopen(fd, "w") as handle:
    json.dump(data, handle, ensure_ascii=False, indent=2)
os.replace(tmp, path)
PY
}

state_remove() {
	python3 - "$STATE_FILE" "$1" <<'PY'
import json, os, sys, tempfile
path, key = sys.argv[1], sys.argv[2]
data = json.load(open(path))
data.get("agents", {}).pop(key, None)
fd, tmp = tempfile.mkstemp(dir=os.path.dirname(path))
with os.fdopen(fd, "w") as handle:
    json.dump(data, handle, ensure_ascii=False, indent=2)
os.replace(tmp, path)
PY
}

state_set_recreated() {
	python3 - "$STATE_FILE" "$1" <<'PY'
import json, os, sys, tempfile, time
path, key = sys.argv[1], sys.argv[2]
data = json.load(open(path))
entry = data.setdefault("agents", {}).get(key)
if entry is None:
    sys.exit("state 中没有该项")
entry["recreated_at"] = time.strftime("%Y-%m-%dT%H:%M:%S")
fd, tmp = tempfile.mkstemp(dir=os.path.dirname(path))
with os.fdopen(fd, "w") as handle:
    json.dump(data, handle, ensure_ascii=False, indent=2)
os.replace(tmp, path)
PY
}

state_set_subdomain() {
	python3 - "$STATE_FILE" "$1" "$2" <<'PY'
import json, os, sys, tempfile
path, key, subdomain = sys.argv[1:4]
data = json.load(open(path))
entry = data.setdefault("agents", {}).get(key)
if entry is None:
    sys.exit("state 中没有该项")
entry["subdomain"] = subdomain
fd, tmp = tempfile.mkstemp(dir=os.path.dirname(path))
with os.fdopen(fd, "w") as handle:
    json.dump(data, handle, ensure_ascii=False, indent=2)
os.replace(tmp, path)
PY
}

state_set_desired() {
	python3 - "$STATE_FILE" "$1" "$2" <<'PY'
import json, os, sys, tempfile, time
path, key, desired = sys.argv[1:4]
data = json.load(open(path))
entry = data.setdefault("agents", {}).get(key)
if entry is None:
    sys.exit("state 中没有该项")
entry["desired"] = desired
if desired == "removed":
    entry.setdefault("removed_at", time.strftime("%Y-%m-%dT%H:%M:%S"))
else:
    entry.pop("removed_at", None)
fd, tmp = tempfile.mkstemp(dir=os.path.dirname(path))
with os.fdopen(fd, "w") as handle:
    json.dump(data, handle, ensure_ascii=False, indent=2)
os.replace(tmp, path)
PY
}

state_field() {
	python3 - "$STATE_FILE" "$1" "$2" <<'PY'
import json, sys
try:
    data = json.load(open(sys.argv[1]))
except Exception:
    data = {}
print(data.get("agents", {}).get(sys.argv[2], {}).get(sys.argv[3], ""))
PY
}

# 输出所有状态键（user 或 user#slot）
state_keys() {
	python3 - "$STATE_FILE" <<'PY'
import json, sys
data = json.load(open(sys.argv[1]))
for key in data.get("agents", {}):
    print(key)
PY
}

# 输出 key<TAB>username<TAB>slot（兼容没有 slot 字段的旧条目）
state_entries() {
	python3 - "$STATE_FILE" <<'PY'
import json, sys
data = json.load(open(sys.argv[1]))
for key, entry in data.get("agents", {}).items():
    username = entry.get("username", key.split("#")[0])
    slot = int(entry.get("slot", key.split("#")[1] if "#" in key else 1))
    print(f"{key}\t{username}\t{slot}")
PY
}

state_ports() {
	python3 - "$STATE_FILE" <<'PY'
import json, sys
data = json.load(open(sys.argv[1]))
for entry in data.get("agents", {}).values():
    print(entry.get("port", ""))
PY
}

# ---------------------------------------------------------------------------
# 容器与路由
# ---------------------------------------------------------------------------

alloc_port() {
	local used p
	used=" $(
		{
			state_ports
			podman ps -a --filter label=felix.agent=1 --format '{{.Label "felix.agent.port"}}' 2>/dev/null || true
		} | tr '\n' ' '
	)"
	for ((p = AGENT_PORT_BASE; p <= AGENT_PORT_MAX; p++)); do
		case "$used" in
		*" $p "*) ;;
		*)
			printf '%s' "$p"
			return 0
			;;
		esac
	done
	return 1
}

container_state() {
	podman inspect -f '{{.State.Status}}' "$(container_of "$1")" 2>/dev/null || printf 'missing'
}

route_write() {
	local key="$1"
	local slug subdomain port password b64 username slot
	slug="$(state_field "$key" slug)"
	subdomain="$(state_field "$key" subdomain)"
	port="$(state_field "$key" port)"
	password="$(state_field "$key" password)"
	username="$(state_field "$key" username)"
	slot="$(state_field "$key" slot)"
	[ -n "$slot" ] || slot=1
	[ -n "$subdomain" ] || subdomain="$slug"
	if [ -z "$slug" ] || [ -z "$port" ] || [ -z "$password" ]; then
		die "state 缺少 $key 的 slug/port/password"
	fi

	b64="$(printf 'opencode:%s' "$password" | base64 | tr -d '\n')"
	local kind vnc_block=""
	kind="$(state_field "$key" kind)"
	if [ "$kind" = "dsh" ]; then
		local vnc_port=$((AGENT_VNC_PORT_BASE + port - AGENT_PORT_BASE))
		vnc_block="
# 容器内虚拟桌面（noVNC）：真实 Chromium 窗口，跨设备查看/操作
@agent_${slug}_vnc host $subdomain.$AGENT_LOCAL_DOMAIN $subdomain.$AGENT_BASE_DOMAIN
handle_path /vnc/* {
	forward_auth 127.0.0.1:$AGENT_SITE_PORT {
		uri /api/agent/auth?user=$username&slot=$slot&vnc=1
	}
	reverse_proxy 127.0.0.1:$vnc_port
}"
	fi
	local before=""
	[ -f "$CADDY_DIR/$slug.caddy" ] && before="$(cat "$CADDY_DIR/$slug.caddy")"
	cat >"$CADDY_DIR/$slug.caddy" <<EOF
# Felix-Homelab Agent：$username（实例 $subdomain，由 scripts/agent-ctl.sh 生成，请勿手改）
# 命名匹配器必须用块状写法：单行里塞多个匹配器（host + not path）会被
# 当作 host 的参数解析，导致 /vnc/* 也被主路由吞掉。
@agent_$slug {
	host $subdomain.$AGENT_LOCAL_DOMAIN $subdomain.$AGENT_BASE_DOMAIN
	not path /vnc/*
}
handle @agent_$slug {
	forward_auth 127.0.0.1:$AGENT_SITE_PORT {
		uri /api/agent/auth?user=$username&slot=$slot&orig={http.request.uri}
	}
	reverse_proxy 127.0.0.1:$port {
		header_up Authorization "Basic $b64"
	}
}$vnc_block
EOF
	chmod 600 "$CADDY_DIR/$slug.caddy"
	# 宿主新建的文件可能带上默认 SELinux 类型，Caddy 容器读不了，尽力修正
	chcon -t container_file_t "$CADDY_DIR/$slug.caddy" 2>/dev/null || true
	if [ "$before" != "$(cat "$CADDY_DIR/$slug.caddy")" ]; then
		ROUTE_CHANGED=1
	fi
}

route_remove() {
	# 参数是内部 slug：路由文件名一直用 slug，匹配的 host 用 subdomain
	local slug
	slug="$(slugify "$1")"
	rm -f "$CADDY_DIR/$slug.caddy"
}

caddy_reload() {
	# Agent 流量由独立网关（host 网络 Caddy）承载；它是 systemd（Quadlet）单元，
	# 必须让 systemd 重启，直接 podman restart 会与单元 cgroup 管理打架。
	if systemctl --user cat "$GATEWAY_UNIT" >/dev/null 2>&1; then
		log "重载 Agent 网关（systemctl --user restart $GATEWAY_UNIT）"
		systemctl --user restart "$GATEWAY_UNIT"
	elif podman container exists "$GATEWAY_CONTAINER" 2>/dev/null; then
		warn "未见 systemd 单元，退化为 podman restart $GATEWAY_CONTAINER"
		podman restart "$GATEWAY_CONTAINER" >/dev/null
	else
		warn "找不到 $GATEWAY_CONTAINER，跳过网关重载"
	fi
}

image_of() {
	case "$1" in
	dsh) printf '%s' "$AGENT_DSH_IMAGE" ;;
	*) printf '%s' "$AGENT_IMAGE" ;;
	esac
}

require_image() {
	local image
	image="$(image_of "$1")"
	if ! podman image exists "$image"; then
		die "模板镜像不存在：$image（先执行 make agent-build 或 make agent-build-dsh）"
	fi
}

container_create() {
	local key="$1" username slug kind port password volume workspace image
	username="$(state_field "$key" username)"
	slug="$(state_field "$key" slug)"
	kind="$(state_field "$key" kind)"
	port="$(state_field "$key" port)"
	password="$(state_field "$key" password)"
	[ -n "$kind" ] || kind="opencode"

	require_image "$kind"
	volume="felix-agent-$slug-data"
	workspace="$WORK_ROOT/$slug/workspace"
	mkdir -p "$workspace"
	podman volume exists "$volume" 2>/dev/null || podman volume create "$volume" >/dev/null

	# 每个 Agent 独立 bridge 网络：与主 Pod / 其他 Agent 网络层隔离
	local network="${AGENT_NET_PREFIX}${slug}"
	if ! podman network exists "$network" 2>/dev/null; then
		podman network create --label felix.agent=1 "$network" >/dev/null
	fi

	log "创建容器 $(container_of "$key")（$kind，端口 $port，内存 $AGENT_MEMORY，CPU $AGENT_CPUS，网络 $network）"

	local -a args=(
		-d
		--name "$(container_of "$key")"
		--network "$network"
		# 只发布到宿主回环：外置 Agent 网关经 127.0.0.1 反代
		--publish "127.0.0.1:$port:$port"
		--security-opt no-new-privileges
		--cap-drop "$AGENT_CAP_DROP"
		--label felix.agent=1
		--label "felix.agent.user=$username"
		--label "felix.agent.slug=$slug"
		--label "felix.agent.kind=$kind"
		--label "felix.agent.port=$port"
		--label "felix.agent.network=$network"
		--label autoheal=true
		--memory "$AGENT_MEMORY"
		--cpus "$AGENT_CPUS"
		--pids-limit "$AGENT_PIDS_LIMIT"
		--env HOME=/data
		--env XDG_DATA_HOME=/data/.local/share
		--env XDG_CONFIG_HOME=/data/.config
		--env XDG_CACHE_HOME=/data/.cache
		# 数据卷带 :Z：每次启动按当前容器的 SELinux MCS 类别重打标签，
		# 否则容器重建后（新类别）会读不了旧卷里的文件
		--volume "$volume:/data:Z"
		# 工作区是宿主 bind mount：必须带 :Z 重打 SELinux 标签，
		# 否则容器内读不到（表现为 agent/pnpm “权限拒绝/功能缺失”）
		--volume "$workspace:/workspace:Z"
		--workdir /workspace
		# 只验证 HTTP 端口有响应：DSH 未登录时 / 返回 401，用 -f 会误判不健康
		--health-cmd "curl -sS --max-time 5 -o /dev/null http://127.0.0.1:$port/"
		--health-interval 30s
		--health-timeout 5s
		--health-start-period 30s
		--health-retries 3
	)

	case "$kind" in
	dsh)
		# DSH 明确拒绝 0.0.0.0（防 RCE）；同 Pod 共享网络命名空间，
		# 绑 127.0.0.1 后 Caddy 仍能经回环反代。
		# 信任栅栏必须用“浏览器实际访问的域名”（随机子域），用内部 slug 会让
		# DSH 自己的 /api 全部 403（表现为设置/模型页不可用）。
		local dsh_host
		dsh_host="$(state_field "$key" subdomain)"
		[ -n "$dsh_host" ] || dsh_host="$slug"
		args+=(--env "DSH_HOME=/data/dsh")
		# 见镜像内 allow-nonloopback-host.py：独立网络下允许绑 0.0.0.0
		args+=(--env "DSH_ALLOW_NON_LOOPBACK=1")
		# 容器内虚拟桌面（Xvfb+openbox+x11vnc+noVNC）：
		#   · DISPLAY 让 ego-browser 以“原生有头”跑真实 Chromium；
		#   · EGO_LINUX_CHROME 指向镜像内包装（私有会话 D-Bus + --no-sandbox）；
		#   · noVNC 端口发布到宿主回环，网关以 <子域>/vnc/ 路径暴露，跨设备可用。
		local vnc_port=$((AGENT_VNC_PORT_BASE + port - AGENT_PORT_BASE))
		args+=(--publish "127.0.0.1:$vnc_port:$vnc_port")
		args+=(--label "felix.agent.vnc=$vnc_port")
		# 默认无头 → 包装脚本走 Obscura（~40MB）；DISPLAY 保留给「弹出窗口」
		# 切到有头 Chromium（虚拟显示，noVNC 里可见）
		args+=(--env "EGO_LINUX_HEADLESS=1")
		args+=(--env "DISPLAY=:${FELIX_DISPLAY_NUM:-99}")
		args+=(--env "FELIX_NOVNC_PORT=$vnc_port")
		args+=(--env "EGO_LINUX_CHROME=/usr/local/bin/felix-ego-chrome.sh")
		# --patch：容器内禁用市场的一键重启（重启由平台/管理员操作）
		# 独立网络里绑 0.0.0.0 才能被宿主端口映射转发；该网络仅此容器，
		# 外部仍只能经网关（回环发布）访问。
		args+=("$(image_of "$kind")" web --patch /opt/dsh-home/agent-patch.yml
			--host 0.0.0.0 --port "$port" --no-open
			--trusted-host "$dsh_host.$AGENT_LOCAL_DOMAIN"
			--trusted-host "$dsh_host.$AGENT_BASE_DOMAIN")
		;;
	*)
		args+=(--env "OPENCODE_SERVER_PASSWORD=$password")
		args+=("$(image_of "$kind")" serve --hostname 0.0.0.0 --port "$port")
		;;
	esac

	# 换镜像重建时，旧容器的端口/网络释放有毫秒级竞态（实测偶发 create 失败）；
	# 失败时短暂等待重试一次，仍失败才向上报错。
	if ! podman run "${args[@]}" >/dev/null; then
		warn "容器创建失败，2 秒后重试一次：$(container_of "$key")"
		sleep 2
		podman run "${args[@]}" >/dev/null
	fi
	state_set_recreated "$key"
}

# 宿主侧就绪探测：发布端口在宿主回环，只有真正能响应 HTTP 才算就绪
wake_ready() {
	local key="$1" port waited=0
	port="$(state_field "$key" port)"
	[ -n "$port" ] || return 1
	while [ "$waited" -lt 40 ]; do
		if curl -s --max-time 2 -o /dev/null "http://127.0.0.1:$port/"; then
			return 0
		fi
		sleep 1
		waited=$((waited + 1))
	done
	return 1
}

do_create() {
	local username="$1" kind="${2:-opencode}" slot="${3:-1}" subdomain="${4:-}"
	local key
	key="$(state_key "$username" "$slot")"
	ensure_layout

	if [ -z "$(state_field "$key" port)" ]; then
		local port password slug
		port="$(alloc_port)" || die "端口池已满（$AGENT_PORT_BASE-$AGENT_PORT_MAX）"
		password="$(openssl rand -hex 16)"
		slug="$(slugify "$username")"
		[ "$slot" -gt 1 ] && slug="$slug-$slot"
		[ -n "$subdomain" ] || subdomain="$(gen_subdomain "$username" "$kind")"
		state_upsert "$username" "$slot" "$slug" "$kind" "$port" "$password" "$subdomain"
	else
		local current_kind
		current_kind="$(state_field "$key" kind)"
		if [ "$current_kind" != "$kind" ]; then
			log "$username #$slot 模板切换：$current_kind → $kind（重建容器，数据卷保留）"
			podman rm -f "$(container_of "$key")" >/dev/null 2>&1 || true
			state_set_kind "$key" "$kind"
		fi
		# 站点是域名的权威来源：续期/迁移时同步过来（无则本地生成兜底）
		if [ -n "$subdomain" ] && [ "$subdomain" != "$(state_field "$key" subdomain)" ]; then
			state_set_subdomain "$key" "$subdomain"
			# DSH 的 --trusted-host 固化在启动参数里，域名变更必须重建容器才生效
			if [ "$kind" = "dsh" ]; then
				log "$username #$slot 域名变更 → 重建容器以应用 --trusted-host"
				podman rm -f "$(container_of "$key")" >/dev/null 2>&1 || true
			fi
		elif [ -z "$(state_field "$key" subdomain)" ]; then
			state_set_subdomain "$key" "$(gen_subdomain "$username" "$kind")"
		fi
	fi

	if ! podman container exists "$(container_of "$key")" 2>/dev/null; then
		container_create "$key"
	else
		podman start "$(container_of "$key")" >/dev/null 2>&1 || true
	fi
	state_set_desired "$key" started
	route_write "$key"
	# 计入一次活动：避免刚授权/启动就被睡眠判定误睡
	: >"$ACTIVITY_DIR/$(state_field "$key" subdomain)"
	# 等真正能响应再返回：站点唤醒回调依赖状态里的 ready
	wake_ready "$key" || warn "$username #$slot 启动后 40s 内未就绪（站点会继续重试）"
}

do_start() {
	local username="$1" slot="${2:-1}" key
	key="$(state_key "$username" "$slot")"
	if [ -z "$(state_field "$key" port)" ]; then
		die "state 中没有 $username #$slot（先在后台开通，或 agent-ctl.sh grant）"
	fi
	if ! podman container exists "$(container_of "$key")" 2>/dev/null; then
		container_create "$key"
	else
		podman start "$(container_of "$key")" >/dev/null 2>&1 || true
	fi
	state_set_desired "$key" started
	route_write "$key"
	# 唤醒即计一次活动
	: >"$ACTIVITY_DIR/$(state_field "$key" subdomain)"
	wake_ready "$key" || warn "$username #$slot 唤醒后 40s 内未就绪（站点会继续重试）"
}

do_stop() {
	local username="$1" slot="${2:-1}" key
	key="$(state_key "$username" "$slot")"
	if podman container exists "$(container_of "$key")" 2>/dev/null; then
		podman stop -t 15 "$(container_of "$key")" >/dev/null 2>&1 || true
	fi
	if [ -n "$(state_field "$key" port)" ]; then
		state_set_desired "$key" stopped
	fi
}

do_remove() {
	local username="$1" slot="${2:-1}" key slug
	key="$(state_key "$username" "$slot")"
	slug="$(state_field "$key" slug)"
	if podman container exists "$(container_of "$key")" 2>/dev/null; then
		podman rm -f "$(container_of "$key")" >/dev/null 2>&1 || true
	fi
	if [ -n "$slug" ]; then
		rm -f "$CADDY_DIR/$slug.caddy"
	fi
	if [ -n "$(state_field "$key" port)" ]; then
		state_set_desired "$key" removed
	fi
}

# 永久删除：容器/数据卷/工作区/路由/state 条目全部立即回收（不可恢复）。
do_purge() {
	local username="$1" slot="${2:-1}" key slug subdomain
	key="$(state_key "$username" "$slot")"
	slug="$(state_field "$key" slug)"
	subdomain="$(state_field "$key" subdomain)"
	[ -n "$slug" ] || die "state 中没有 $username #$slot"
	log "永久删除：$username #$slot（$subdomain）"
	podman rm -f "$(container_of "$key")" >/dev/null 2>&1 || true
	podman volume rm -f "felix-agent-$slug-data" >/dev/null 2>&1 || true
	podman network rm -f "${AGENT_NET_PREFIX}${slug}" >/dev/null 2>&1 || true
	rm -rf "$WORK_ROOT/$slug"
	rm -f "$CADDY_DIR/$slug.caddy"
	state_remove "$key"
}

# DSH 的「设置/提供商」是特权面：只有浏览器地址为 localhost 时才可编辑；
# 远程访问按官方凭据层写 `$DSH_HOME/.credentials.yaml`（官方文档：外部编辑会
# 热发布；LLM 与联网搜索都引用 DEEPSEEK_API_KEY）。因此这里直接改凭据文件的
# refs 段：不重启、不换令牌、用户无感。文件 0600，只写不读值。
do_setkey() {
	local username="$1" slot="$2"
	shift 2
	local key kind slug volume_dir cred_file
	key="$(state_key "$username" "$slot")"
	kind="$(state_field "$key" kind)"
	slug="$(state_field "$key" slug)"
	[ -n "$slug" ] || die "state 中没有 $username #$slot（先在后台开通）"
	[ "$kind" = "dsh" ] || die "当前实例是 $kind；setkey 只用于 dsh（OpenCode 在其 Web UI 内配置）"

	volume_dir="$(podman volume inspect "felix-agent-$slug-data" -f '{{.Mountpoint}}' 2>/dev/null)" \
		|| die "数据卷 felix-agent-$slug-data 不存在"
	cred_file="$volume_dir/dsh/.credentials.yaml"
	mkdir -p "$volume_dir/dsh"
	chmod 700 "$volume_dir/dsh"

	if [ "$#" -eq 0 ]; then
		if [ -f "$cred_file" ]; then
			show_ref_names "$cred_file" | sed 's/$/=<已设置>/'
		else
			echo "（尚未设置任何密钥）"
		fi
		return 0
	fi

	local pair name
	for pair in "$@"; do
		case "$pair" in
		*=*) ;;
		*) die "参数格式应为 KEY=VALUE：$pair" ;;
		esac
		name="${pair%%=*}"
		[[ "$name" =~ ^[A-Z][A-Z0-9_]*$ ]] || die "非法的环境变量名：$name"
	done

	python3 - "$cred_file" "$@" <<'PY'
import os, sys, tempfile

path, pairs = sys.argv[1], sys.argv[2:]
updates = {}
for pair in pairs:
    key, _, value = pair.partition("=")
    updates[key] = "'" + value.replace("'", "''") + "'"

text = open(path).read() if os.path.exists(path) else "version: 1\n"
lines = text.splitlines()
while lines and lines[-1].strip() == "":
    lines.pop()

start = next((i for i, line in enumerate(lines) if line.rstrip() == "refs:"), None)
if start is None:
    lines.append("refs:")
    for key, value in updates.items():
        lines.append(f"  {key}: {value}")
else:
    end = len(lines)
    for index in range(start + 1, len(lines)):
        line = lines[index]
        if line.strip() == "":
            continue
        if not line.startswith(" "):
            end = index
            break
    entry_at = {}
    for index in range(start + 1, end):
        stripped = lines[index].lstrip()
        if not stripped or stripped.startswith("#") or ":" not in stripped:
            continue
        entry_at[stripped.split(":", 1)[0].strip()] = index
    insert_at = end
    for key, value in updates.items():
        if key in entry_at:
            lines[entry_at[key]] = f"  {key}: {value}"
        else:
            lines.insert(insert_at, f"  {key}: {value}")
            insert_at += 1

new_text = "\n".join(lines) + "\n"
directory = os.path.dirname(path)
fd, tmp = tempfile.mkstemp(dir=directory)
with os.fdopen(fd, "w") as handle:
    handle.write(new_text)
os.chmod(tmp, 0o600)
os.replace(tmp, path)
PY

	log "已更新凭据（仅显示名称）：$(printf '%s ' "${@%%=*}")；DSH 热加载即时生效"
}

# 打印凭据文件 refs 段的引用名（不显示值）
show_ref_names() {
	python3 - "$1" <<'PY'
import sys
try:
    lines = open(sys.argv[1]).read().splitlines()
except Exception:
    raise SystemExit
start = next((i for i, line in enumerate(lines) if line.rstrip() == "refs:"), None)
if start is None:
    raise SystemExit
for line in lines[start + 1:]:
    if line.strip() == "":
        continue
    if not line.startswith(" "):
        break
    stripped = line.lstrip()
    if stripped.startswith("#") or ":" not in stripped:
        continue
    print(stripped.split(":", 1)[0].strip())
PY
}

# ---------------------------------------------------------------------------
# 请求处理 / 保活 / 状态
# ---------------------------------------------------------------------------

parse_request() {
	python3 - "$1" <<'PY'
import json, sys
try:
    data = json.load(open(sys.argv[1]))
except Exception:
    sys.exit(0)
slot = int(data.get("slot", 1) or 1)
print(data.get("action", ""), data.get("username", ""), data.get("kind", ""), slot,
      data.get("subdomain", ""), data.get("key_name", ""), data.get("key_value", ""), sep="\t")
PY
}

apply_requests() {
	ensure_layout
	local changed=0 file action username kind slot subdomain key_name key_value
	shopt -s nullglob
	for file in "$REQUESTS_DIR"/*.json; do
		IFS=$'\t' read -r action username kind slot subdomain key_name key_value < <(parse_request "$file") || true
		[ -n "${slot:-}" ] || slot=1
		if [ -z "$action" ] || [ -z "$username" ]; then
			warn "忽略无法解析的请求：$(basename "$file")"
			rm -f "$file"
			continue
		fi
		if [[ ! "$username" =~ ^[A-Za-z0-9_-]+$ ]]; then
			warn "忽略含非法用户名的请求：$username"
			rm -f "$file"
			continue
		fi
		# 动作放在子 shell 里执行：失败只影响这一条，请求文件保留待下轮重试
		case "$action" in
		grant)
			log "处理授权：$username #$slot（$kind）"
			if (do_create "$username" "${kind:-opencode}" "$slot" "$subdomain"); then changed=1; else
				warn "授权失败，保留请求稍后重试：$(basename "$file")"
				continue
			fi
			;;
		start)
			log "处理启动：$username #$slot"
			# 动作在子 shell 里跑，变量出不来：用路由目录内容哈希判断是否要重载
			local routes_before routes_after
			routes_before="$(cat "$CADDY_DIR"/*.caddy 2>/dev/null | md5sum)"
			if (do_start "$username" "$slot"); then
				routes_after="$(cat "$CADDY_DIR"/*.caddy 2>/dev/null | md5sum)"
				[ "$routes_before" = "$routes_after" ] || changed=1
			else
				warn "启动失败，保留请求稍后重试：$(basename "$file")"
				continue
			fi
			;;
		stop)
			log "处理暂停：$username #$slot"
			(do_stop "$username" "$slot") || warn "暂停失败：$username #$slot"
			;;
		remove)
			log "处理删除：$username #$slot"
			if (do_remove "$username" "$slot"); then changed=1; else
				warn "删除失败，保留请求稍后重试：$(basename "$file")"
				continue
			fi
			;;
		purge)
			log "处理永久删除：$username #$slot"
			if (do_purge "$username" "$slot"); then changed=1; else
				warn "永久删除失败，保留请求稍后重试：$(basename "$file")"
				continue
			fi
			;;
		setkey)
			log "处理凭据更新：$username #$slot（$key_name）"
			if [ -z "$key_name" ] || [ -z "$key_value" ]; then
				warn "setkey 请求缺少 key_name/key_value，忽略：$(basename "$file")"
			elif (do_setkey "$username" "$slot" "$key_name=$key_value"); then :; else
				warn "凭据更新失败，保留请求稍后重试：$(basename "$file")"
				continue
			fi
			;;
		*)
			warn "忽略未知动作：$action"
			;;
		esac
		rm -f "$file"
	done
	shopt -u nullglob
	[ "$changed" = 1 ] && caddy_reload || true
}

reconcile() {
	ensure_layout
	local key username slot desired state recreated age now
	now="$(date +%s)"
	while IFS=$'\t' read -r key username slot; do
		[ -n "$key" ] || continue
		desired="$(state_field "$key" desired)"
		state="$(container_state "$key")"

		# 定期重建：可写层非持久（数据卷/工作区保留），清掉可能的持久化改动。
		# 睡眠中的实例直接删容器，下次唤醒会按新镜像/新配置重建。
		recreated="$(state_field "$key" recreated_at)"
		if [ -n "$recreated" ] && [ "$desired" != "removed" ] && [ "$desired" != "stopped" ]; then
			age=$((now - $(date -d "$recreated" +%s 2>/dev/null || echo "$now")))
			if [ "$age" -gt $((AGENT_RECREATE_DAYS * 86400)) ]; then
				log "定期重建：$username #$slot（可写层已用 $((age / 86400)) 天）"
				podman rm -f "$(container_of "$key")" >/dev/null 2>&1 || true
				# 时间戳记成“本刻”：睡眠实例的容器已删，避免每轮 tick 重复删与刷日志；
				# started 的紧接着会重建并再次更新时间戳。
				state_set_recreated "$key"
				state="missing"
			fi
		fi

		case "$desired:$state" in
		started:running | stopped:exited | stopped:created | sleeping:* | removed:* | :*) ;;
		started:*)
			log "保活启动：$username #$slot（当前 $state）"
			if [ "$state" = "missing" ]; then
				(container_create "$key") || warn "重建 $username #$slot 失败，下轮再试"
			else
				podman start "$(container_of "$key")" >/dev/null 2>&1 || true
			fi
			;;
		stopped:running)
			log "按暂停状态停止：$username #$slot"
			podman stop -t 15 "$(container_of "$key")" >/dev/null 2>&1 || true
			;;
		esac
	done < <(state_entries)
}

# 空闲睡眠：desired=started 且运行中，但活动时间超过阈值 → 停容器（desired=sleeping）
sleep_idle() {
	ensure_layout
	python3 - "$STATE_FILE" "$ACTIVITY_DIR" "$AGENT_IDLE_SECONDS" <<'PY'
import json, os, subprocess, sys, tempfile, time

state_file, activity_dir, idle_seconds = sys.argv[1], sys.argv[2], int(sys.argv[3])
try:
    data = json.load(open(state_file))
except Exception:
    data = {"agents": {}}
now = time.time()
changed = False
for key, entry in data.get("agents", {}).items():
    if entry.get("desired") != "started":
        continue
    if entry.get("kind") != "dsh" and entry.get("kind") != "opencode":
        pass
    name = f"felix-agent-{entry.get('slug', '')}"
    try:
        proc = subprocess.run(["podman", "inspect", name], capture_output=True, text=True, timeout=15)
        if proc.returncode != 0:
            continue
        info = json.loads(proc.stdout)[0]
        if info.get("State", {}).get("Status") != "running":
            continue
        started = info.get("State", {}).get("StartedAt")
    except Exception:
        continue

    last = None
    path = os.path.join(activity_dir, entry.get("subdomain", ""))
    if entry.get("subdomain") and os.path.exists(path):
        last = os.path.getmtime(path)
    if last is None and started:
        try:
            last = time.mktime(time.strptime(started[:19], "%Y-%m-%dT%H:%M:%S"))
        except Exception:
            last = None
    if last is None or now - last < idle_seconds:
        continue

    slot = entry.get('slot') or (key.split('#', 1)[1] if '#' in key else 1)
    print(f"[agent] 空闲睡眠：{entry.get('username')} #{slot}（{int(now - last)}s 无活动）")
    subprocess.run(["podman", "stop", "-t", "15", name], capture_output=True, text=True, timeout=60)
    entry["desired"] = "sleeping"
    entry.pop("removed_at", None)
    changed = True

if changed:
    fd, tmp = tempfile.mkstemp(dir=os.path.dirname(state_file))
    with os.fdopen(fd, "w") as handle:
        json.dump(data, handle, ensure_ascii=False, indent=2)
    os.replace(tmp, state_file)
PY
}

# 撤销宽限期到期回收：容器/卷/工作区/路由/state 条目全部删除（域名随之释放）
purge_removed() {
	ensure_layout
	python3 - "$STATE_FILE" "$CADDY_DIR" "$WORK_ROOT" "$AGENT_GRACE_DAYS" <<'PY'
import json, os, shutil, subprocess, sys, tempfile, time

state_file, caddy_dir, work_root, grace_days = sys.argv[1], sys.argv[2], sys.argv[3], int(sys.argv[4])
try:
    data = json.load(open(state_file))
except Exception:
    data = {"agents": {}}
now = time.time()
removed_keys = []
for key, entry in data.get("agents", {}).items():
    if entry.get("desired") != "removed":
        continue
    stamp = entry.get("removed_at")
    if not stamp:
        continue
    try:
        removed_at = time.mktime(time.strptime(stamp[:19], "%Y-%m-%dT%H:%M:%S"))
    except Exception:
        continue
    if now - removed_at < grace_days * 86400:
        continue
    removed_keys.append(key)

for key in removed_keys:
    entry = data["agents"][key]
    slug = entry.get("slug", "")
    slot = entry.get('slot') or (key.split('#', 1)[1] if '#' in key else 1)
    print(f"[agent] 宽限期到期回收：{entry.get('username')} #{slot}")
    subprocess.run(["podman", "rm", "-f", f"felix-agent-{slug}"], capture_output=True, text=True, timeout=60)
    subprocess.run(["podman", "volume", "rm", "-f", f"felix-agent-{slug}-data"], capture_output=True, text=True, timeout=60)
    subprocess.run(["podman", "network", "rm", "-f", f"felix-agent-{slug}"], capture_output=True, text=True, timeout=60)
    shutil.rmtree(os.path.join(work_root, slug), ignore_errors=True)
    if slug:
        try:
            os.remove(os.path.join(caddy_dir, f"{slug}.caddy"))
        except OSError:
            pass
    data["agents"].pop(key, None)

if removed_keys:
    fd, tmp = tempfile.mkstemp(dir=os.path.dirname(state_file))
    with os.fdopen(fd, "w") as handle:
        json.dump(data, handle, ensure_ascii=False, indent=2)
    os.replace(tmp, state_file)
PY
}

sync_status() {
	ensure_layout
	python3 - "$STATE_FILE" "$STATUS_FILE" <<'PY'
import json, os, re, subprocess, sys, tempfile, time

state_file, status_file = sys.argv[1:3]
try:
    state = json.load(open(state_file))
except Exception:
    state = {"agents": {}}


def volume_dir(slug):
    try:
        proc = subprocess.run(
            ["podman", "volume", "inspect", f"felix-agent-{slug}-data", "-f", "{{.Mountpoint}}"],
            capture_output=True, text=True, timeout=15)
        if proc.returncode == 0 and proc.stdout.strip():
            return proc.stdout.strip()
    except Exception:
        pass
    return None


def read_ref_names(directory):
    """读取凭据文件 refs 段的引用名（只读名字，不读值）。"""
    if not directory:
        return []
    path = os.path.join(directory, "dsh", ".credentials.yaml")
    try:
        lines = open(path).read().splitlines()
    except Exception:
        return []
    start = next((i for i, line in enumerate(lines) if line.rstrip() == "refs:"), None)
    if start is None:
        return []
    names = []
    for line in lines[start + 1:]:
        if line.strip() == "":
            continue
        if not line.startswith(" "):
            break
        stripped = line.lstrip()
        if stripped.startswith("#") or ":" not in stripped:
            continue
        names.append(stripped.split(":", 1)[0].strip())
    return names


agents = {}
for entry in state.get("agents", {}).values():
    slug = entry.get("slug", "unknown")
    subdomain = entry.get("subdomain") or slug
    kind = entry.get("kind", "opencode")
    name = f"felix-agent-{slug}"
    record = {"kind": kind, "port": entry.get("port", 0),
              "state": "missing", "health": None, "token": None, "keys": [],
              "desired": entry.get("desired", ""), "subdomain": subdomain,
              "ready": False}
    started_at = None
    try:
        proc = subprocess.run(["podman", "inspect", name], capture_output=True, text=True, timeout=15)
        if proc.returncode == 0:
            info = json.loads(proc.stdout)[0]
            st = info.get("State", {})
            record["state"] = st.get("Status", "unknown")
            started_at = st.get("StartedAt")
            health = st.get("Health") or {}
            record["health"] = health.get("Status")
    except Exception:
        record["state"] = "unknown"

    # 就绪：发布端口在宿主回环上能响应 HTTP（站点唤醒回调据此放行）
    ready = False
    port = record.get("port") or 0
    if record["state"] == "running" and port:
        try:
            probe = subprocess.run(
                ["curl", "-s", "--max-time", "2", "-o", "/dev/null", f"http://127.0.0.1:{port}/"],
                capture_output=True, text=True, timeout=5)
            ready = probe.returncode == 0
        except Exception:
            ready = False
    record["ready"] = ready
    record["keys"] = read_ref_names(volume_dir(slug))
    # DSH 自带令牌登录：只取“本次启动之后”的日志，避免抓到上一次启动的 token。
    # 新进程可能还没来得及打印，短轮询等待。
    if kind == "dsh" and record["state"] == "running" and started_at:
        def fetch_token():
            logs = subprocess.run(
                ["podman", "logs", "--since", started_at, "--tail", "200", name],
                capture_output=True, text=True, timeout=15)
            text = (logs.stdout or "") + (logs.stderr or "")
            found = re.findall(r"token=([A-Za-z0-9_-]+)", text)
            return found[-1] if found else None

        token = fetch_token()
        for _ in range(10):
            if token:
                break
            time.sleep(2)
            token = fetch_token()
        record["token"] = token
    agents[subdomain] = record

data = {"updated_at": time.strftime("%Y-%m-%dT%H:%M:%S"), "agents": agents}
fd, tmp = tempfile.mkstemp(dir=os.path.dirname(status_file))
with os.fdopen(fd, "w") as handle:
    json.dump(data, handle, ensure_ascii=False, indent=2)
os.replace(tmp, status_file)
PY
}

list_agents() {
	ensure_layout
	printf '%-14s %-5s %-9s %-8s %-9s %-8s %-10s %s\n' USER SLOT KIND PORT DESIRED STATE HEALTH SUBDOMAIN
	local key username slot
	while IFS=$'\t' read -r key username slot; do
		[ -n "$key" ] || continue
		printf '%-14s %-5s %-9s %-8s %-9s %-8s %-10s %s\n' \
			"$username" \
			"$slot" \
			"$(state_field "$key" kind)" \
			"$(state_field "$key" port)" \
			"$(state_field "$key" desired)" \
			"$(container_state "$key")" \
			"$(podman inspect -f '{{if .State.Health}}{{.State.Health.Status}}{{end}}' "$(container_of "$key")" 2>/dev/null || true)" \
			"$(state_field "$key" subdomain)"
	done < <(state_entries)
}

list_versions() {
	local created image size used current
	printf '%-46s %-17s %-9s %-24s %s\n' "镜像" "创建时间" "大小" "在用实例" "当前指向"
	while IFS='|' read -r created image size; do
		used="$(podman ps -a --filter "ancestor=$image" --format '{{.Names}}' 2>/dev/null | paste -sd, -)"
		current=""
		[ "$image" = "$AGENT_IMAGE" ] && current="opencode"
		[ "$image" = "$AGENT_DSH_IMAGE" ] && current="${current:+$current,}dsh"
		printf '%-46s %-17s %-9s %-24s %s\n' \
			"$image" "$(printf '%s' "$created" | cut -d' ' -f1-2)" "$size" "${used:-—}" "${current:-—}"
	done < <(podman images \
		--format '{{.CreatedAt}}|{{.Repository}}:{{.Tag}}|{{.Size}}' 2>/dev/null \
		| grep -E '(^|/)felix-agent-(opencode|dsh):' | sort -r)
}

# 固定模板镜像版本：把 AGENT_IMAGE / AGENT_DSH_IMAGE 写回 .env（版本标签由
# build-agent-image.sh 留存，默认最近 3 个）。改完用 recreate-all 让实例生效。
pin_image() {
	local kind="${1:-}" ref="${2:-}" var repo target
	case "$kind" in
	opencode) var=AGENT_IMAGE ;;
	dsh) var=AGENT_DSH_IMAGE ;;
	*) die "用法：agent-ctl.sh pin <opencode|dsh> <版本|latest>" ;;
	esac
	[ -n "$ref" ] || die "用法：agent-ctl.sh pin <opencode|dsh> <版本|latest>"
	repo="${!var%:*}"
	if [ "$ref" = "latest" ]; then
		target="$repo:latest"
	else
		target="$repo:$ref"
		podman image exists "$target" \
			|| die "版本镜像不存在：$target（agent-ctl.sh versions 查看本机留存版本）"
	fi
	python3 - "$CONFIG_DIR/.env" "$var" "$target" <<'PY'
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
key, value = sys.argv[2], sys.argv[3]
lines = path.read_text().splitlines() if path.exists() else []
out = []
found = False
for line in lines:
    if line.startswith(key + "="):
        out.append(f"{key}={value}")
        found = True
    else:
        out.append(line)
if not found:
    out.append(f"{key}={value}")
path.write_text("\n".join(out) + "\n")
PY
	log "已固定 $var=$target（写回 $CONFIG_DIR/.env）"
	log "执行 agent-ctl.sh recreate-all $kind 让实例生效"
}

# 按当前固定版本重建实例：started 的立即用新镜像重建，sleeping/stopped 的
# 只删容器（下次唤醒/启动自然换镜像）。数据卷/工作区/域名/登录态均保留。
recreate_all() {
	local kind="${1:-all}" key k
	ensure_layout
	while IFS=$'\t' read -r key _username _slot; do
		[ -n "$key" ] || continue
		k="$(state_field "$key" kind)"
		[ "$kind" = "all" ] || [ "$k" = "$kind" ] || continue
		[ "$(state_field "$key" desired)" = "removed" ] && continue
		if [ "$(container_state "$key")" != "missing" ]; then
			log "换镜像重建：$(state_field "$key" username) #$(state_field "$key" slot)"
			podman rm -f "$(container_of "$key")" >/dev/null 2>&1 || true
		fi
	done < <(state_entries)
	reconcile
	sync_status
}

sync_routes() {
	ensure_layout
	local key
	while IFS=$'\t' read -r key _username _slot; do
		[ -n "$key" ] || continue
		[ "$(state_field "$key" desired)" = "removed" ] && continue
		route_write "$key"
	done < <(state_entries)
}

build_image() {
	local kind="${1:-opencode}"
	exec "$REPO_DIR/scripts/build-agent-image.sh" "$kind"
}

usage() {
	sed -n '3,20p' "$0" | sed 's/^# \{0,1\}//'
}

case "${1:-}" in
build)
	shift
	build_image "${1:-opencode}"
	;;
grant)
	shift
	[ $# -ge 1 ] || die "用法：agent-ctl.sh grant <user> [kind] [slot]"
	do_create "$1" "${2:-opencode}" "${3:-1}"
	sync_status
	caddy_reload
	warn "已创建容器；授权状态仍以站点后台（agent_subscriptions）为准"
	;;
start | stop | remove)
	action="$1"
	shift
	[ $# -ge 1 ] || die "用法：agent-ctl.sh $action <user> [slot]"
	ensure_layout
	ROUTE_CHANGED=0
	"do_$action" "$1" "${2:-1}"
	sync_status
	if [ "$action" != "stop" ] && { [ "$action" = "remove" ] || [ "$ROUTE_CHANGED" = 1 ]; }; then
		caddy_reload
	fi
	;;
setkey)
	shift
	[ $# -ge 1 ] || die "用法：agent-ctl.sh setkey <user> [slot] [KEY=VALUE ...]"
	setkey_user="$1"
	shift
	setkey_slot=1
	if [ $# -ge 1 ] && [[ "$1" =~ ^[0-9]+$ ]]; then
		setkey_slot="$1"
		shift
	fi
	ensure_layout
	do_setkey "$setkey_user" "$setkey_slot" "$@"
	sync_status
	;;
apply | tick)
	apply_requests
	reconcile
	sleep_idle
	purge_removed
	sync_status
	;;
list)
	list_agents
	;;
versions)
	list_versions
	;;
pin)
	shift
	pin_image "${1:-}" "${2:-}"
	;;
recreate-all)
	shift
	recreate_all "${1:-all}"
	;;
sync-routes)
	sync_routes
	caddy_reload
	;;
*)
	usage
	exit 1
	;;
esac
