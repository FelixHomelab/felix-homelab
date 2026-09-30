#!/usr/bin/env bash
#
# 构建站内语音转文字（STT）镜像 localhost/felix-whisper:latest
#
#   scripts/build-whisper.sh            构建（用缓存）
#   scripts/build-whisper.sh --no-cache 不用缓存重建
#
# 说明：为什么不用官方 whisper.cpp 镜像——其二进制按构建机（AVX512）编译，
# 在本机（i7-13650HX，无 AVX512）加载模型即 SIGILL；因此改用 faster-whisper
# （CTranslate2，CUDA 开箱即用，自带 CPU 回退），见 containers/whisper/。
set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
EXTRA_ARGS=()
[ "${1:-}" = "--no-cache" ] && EXTRA_ARGS+=(--no-cache)

podman build "${EXTRA_ARGS[@]}" \
	-t localhost/felix-whisper:latest \
	-f "$REPO_DIR/containers/whisper/Containerfile" \
	"$REPO_DIR/containers/whisper"
