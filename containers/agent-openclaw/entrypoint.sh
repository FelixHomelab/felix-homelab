#!/bin/sh
# OpenClaw 入口：首次启动生成网关配置（绑定 lan + token 鉴权 + 控制台），前台运行。
set -e
PORT="${AGENT_PORT:-18789}"
TOKEN="${AGENT_TOKEN:-}"
CONF_DIR="$HOME/.openclaw"
mkdir -p "$CONF_DIR"
if [ ! -f "$CONF_DIR/openclaw.json" ]; then
	cat > "$CONF_DIR/openclaw.json" <<JSON
{
  "gateway": {
    "mode": "local",
    "port": ${PORT},
    "bind": "lan",
    "auth": { "mode": "token", "token": "${TOKEN}" },
    "controlUi": { "enabled": true }
  }
}
JSON
fi
exec openclaw gateway run
