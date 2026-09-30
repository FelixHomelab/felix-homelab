#!/bin/sh
# 终端型 Agent 入口：在 AGENT_PORT 上开 ttyd，承载 TUI 代理。
set -e
PORT="${AGENT_PORT:-7681}"
mkdir -p "$HOME"

# Pi：构建期烘焙的扩展（/opt/pi-home/.pi）在首次启动时复制到运行时 HOME
if [ "${AGENT_BIN}" = "pi" ] && [ ! -d "$HOME/.pi/agent" ] && [ -d /opt/pi-home/.pi ]; then
	cp -a /opt/pi-home/.pi "$HOME/.pi"
fi

exec ttyd -W -p "$PORT" -t 'fontSize=14' -t 'titleFixed=Agent' "${AGENT_BIN:-sh}"
