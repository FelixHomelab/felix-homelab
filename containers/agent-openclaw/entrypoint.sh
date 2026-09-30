#!/bin/sh
# OpenClaw 入口：首次启动生成网关配置（绑定 lan + token 鉴权 + 控制台），前台运行。
#
# trustedProxies：代理（本站 Agent 网关）经宿主端口发布进入容器，来源地址属于
# 容器自身网段（pasta 映射，实测为容器网段地址）。这里在运行时解析本容器网段
# CIDR + 回环，写入 trustedProxies，满足 OpenClaw 的代理归属校验。
set -e
PORT="${AGENT_PORT:-18789}"
TOKEN="${AGENT_TOKEN:-}"
CONF_DIR="$HOME/.openclaw"
mkdir -p "$CONF_DIR"

TRUSTED="$(python3 - <<'PY'
import json, socket, struct

entries = {"127.0.0.1", "::1"}
try:
    # 自身 IP（pasta 映射下，宿主转发进来的流量会以该地址出现）
    own = socket.gethostbyname(socket.gethostname())
    entries.add(own)
except Exception:
    pass
try:
    for line in open("/proc/net/route").read().strip().splitlines()[1:]:
        fields = line.split()
        if len(fields) < 8:
            continue
        dest, gw, mask = int(fields[1], 16), int(fields[2], 16), int(fields[7], 16)
        if dest == 0 and mask == 0 and gw != 0:
            entries.add(socket.inet_ntoa(struct.pack("<I", gw)))  # 默认网关
            continue
        if dest != 0 and mask != 0:
            net = socket.inet_ntoa(struct.pack("<I", dest))
            entries.add(f"{net}/{bin(mask).count('1')}")  # on-link 网段
except Exception:
    pass
print(json.dumps(sorted(entries)))
PY
)"

# 平台托管的键每次启动刷新（端口/鉴权/代理信任），其余用户配置保留
PORT="$PORT" TOKEN="$TOKEN" TRUSTED="$TRUSTED" python3 - <<'PY'
import json, os

path = os.path.join(os.environ["HOME"], ".openclaw", "openclaw.json")
os.makedirs(os.path.dirname(path), exist_ok=True)
config = {}
if os.path.exists(path):
    try:
        config = json.load(open(path))
    except Exception:
        config = {}
gw = config.setdefault("gateway", {})
gw["mode"] = "local"
gw["port"] = int(os.environ["PORT"])
gw["bind"] = "lan"
gw.setdefault("auth", {})
gw["auth"]["mode"] = "token"
gw["auth"]["token"] = os.environ["TOKEN"]
gw.setdefault("controlUi", {})["enabled"] = True
gw["trustedProxies"] = json.loads(os.environ["TRUSTED"])
with open(path, "w") as handle:
    json.dump(config, handle, indent=2)
    handle.write("\n")
PY

# FreeRide：构建期已烘焙到 /opt，首次启动注册为全局技能
if [ -d /opt/openclaw-skills/free-ride ] && [ ! -e "$HOME/.openclaw/skills/free-ride" ]; then
	openclaw skills install /opt/openclaw-skills/free-ride --as free-ride --global >/dev/null 2>&1 || true
fi

exec openclaw gateway run
