#!/usr/bin/env bash
#
# 构建个人主页（主站）镜像 localhost/felix-homepage:latest
#
#   scripts/build-site.sh            构建（用缓存）
#   scripts/build-site.sh --no-cache 不用缓存重建
#
set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
EXTRA_ARGS=()
[ "${1:-}" = "--no-cache" ] && EXTRA_ARGS+=(--no-cache)

podman build "${EXTRA_ARGS[@]}" \
	-t localhost/felix-homepage:latest \
	-f "$REPO_DIR/containers/site/Containerfile" \
	"$REPO_DIR/site"
