#!/usr/bin/env bash
#
# 构建主站（Felix Homelab 社区站）镜像 localhost/felix-homelab-site:latest
#
#   scripts/build-site.sh            构建（用缓存）
#   scripts/build-site.sh --no-cache 不用缓存重建
#
set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
EXTRA_ARGS=()
[ "${1:-}" = "--no-cache" ] && EXTRA_ARGS+=(--no-cache)

# 资源目录构建戳：每次发布新 URL（/pkg-<stamp>/…），客户端与 CDN 旧缓存自然失效
ASSET_STAMP="${ASSET_STAMP:-$(date +%Y%m%d%H%M%S)}"

podman build "${EXTRA_ARGS[@]}" \
	--build-arg "ASSET_STAMP=$ASSET_STAMP" \
	-t localhost/felix-homelab-site:latest \
	-f "$REPO_DIR/containers/site/Containerfile" \
	"$REPO_DIR/site"
