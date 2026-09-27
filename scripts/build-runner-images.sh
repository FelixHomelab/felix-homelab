#!/usr/bin/env bash
#
# 为 Runner 标签涉及的所有镜像构建“修复版”本地镜像。
#
# 修复内容：把镜像内 /var/run 由绝对符号链接改为相对符号链接，
# 规避 Podman 5.8.7/buildah 1.43.4 的 archive-copy 回归。
# 详见 containers/runner-image/Containerfile。
#
# 输入：$CONFIG_DIR/runner-labels.txt（<标签>:docker://<镜像>）
# 输出：$CONFIG_DIR/runner-labels.resolved.txt（镜像改为本地修复版）
#
# 若 Podman 已修复，可设 FELIX_NO_RUNNER_FIX=1 跳过（直接使用原始镜像）。
#
set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/felix-homelab"
LABELS_FILE="${1:-$CONFIG_DIR/runner-labels.txt}"
OUT_FILE="$CONFIG_DIR/runner-labels.resolved.txt"
CONTAINERFILE="$REPO_DIR/containers/runner-image/Containerfile"

log() { printf '\033[1;36m[felix]\033[0m %s\n' "$*"; }

[ -f "$LABELS_FILE" ] || {
	echo "缺少标签文件: $LABELS_FILE" >&2
	exit 1
}

: > "$OUT_FILE"

while IFS= read -r line; do
	# 跳过注释与空行
	[[ "$line" =~ ^[[:space:]]*(#|$) ]] && continue

	# 解析 <标签>:docker://<镜像>
	[[ "$line" == *":docker://"* ]] || continue
	name="${line%%:docker://*}"
	image="${line#*:docker://}"
	name="${name//[[:space:]]/}"
	image="${image//[[:space:]]/}"
	[ -n "$name" ] && [ -n "$image" ] || continue

	# Podman 已修复时直接透传
	if [ "${FELIX_NO_RUNNER_FIX:-0}" = "1" ]; then
		printf '%s:docker://%s\n' "$name" "$image" >> "$OUT_FILE"
		continue
	fi

	tag="localhost/felix-runner:$(printf '%s' "$image" | tr ':/' '__')"
	if ! podman image exists "$tag"; then
		log "构建修复版镜像: $image -> $tag"
		if ! podman build --format docker -q \
			--build-arg "BASE=$image" \
			-t "$tag" \
			-f "$CONTAINERFILE" \
			"$(dirname "$CONTAINERFILE")" >/dev/null; then
			printf '\033[1;33m[felix]\033[0m 构建失败，跳过标签 "%s"（镜像 %s）\n' "$name" "$image" >&2
			continue
		fi
	fi
	printf '%s:docker://%s\n' "$name" "$tag" >> "$OUT_FILE"
done < "$LABELS_FILE"

log "已生成 $OUT_FILE"
