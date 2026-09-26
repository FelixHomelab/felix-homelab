#!/usr/bin/env bash
#
# 等待 Forgejo 容器就绪
#
set -euo pipefail

CONTAINER="${FORGEJO_CONTAINER:-felix-workstation-forgejo}"
TRIES="${1:-60}"

for _ in $(seq 1 "$TRIES"); do
	if podman exec "$CONTAINER" wget -q --spider http://127.0.0.1:3000/api/healthz 2>/dev/null; then
		echo "Forgejo 已就绪"
		exit 0
	fi
	sleep 2
done

echo "等待 Forgejo 超时" >&2
exit 1
