#!/bin/sh
# 终端型 Agent 入口：在 AGENT_PORT 上开 ttyd，承载 TUI 代理。
set -e
PORT="${AGENT_PORT:-7681}"
mkdir -p "$HOME"
exec ttyd -W -p "$PORT" -t 'fontSize=14' -t 'titleFixed=Agent' "${AGENT_BIN:-sh}"
