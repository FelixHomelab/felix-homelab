#!/bin/sh
# DSH 容器入口：
#   · 首次启动把镜像里烤好的 web profile（含 dsh-market 插件市场）播种到数据卷；
#   · 镜像升级（DSH / 市场版本戳变化）时**非破坏合并**刷新 profile：
#     官方文件更新，用户自己装的插件、收藏/分组/备注（state.json）与手工补丁保留。
#
# 仅用 POSIX sh，镜像内无需 bash。
set -eu

: "${DSH_HOME:=/data/dsh}"
IMAGE_VERSION="$(cat /opt/dsh-home/.dsh-image-version 2>/dev/null || echo unknown)"
VOLUME_VERSION="$(cat "$DSH_HOME/.dsh-image-version" 2>/dev/null || echo none)"

if [ "$IMAGE_VERSION" != "$VOLUME_VERSION" ] || [ ! -e "$DSH_HOME/profiles" ]; then
	echo "[dsh-entry] 合并刷新 profile：$VOLUME_VERSION -> $IMAGE_VERSION" >&2
	mkdir -p "$DSH_HOME"
	python3 /usr/local/bin/dsh-merge-profile.py /opt/dsh-home/profiles "$DSH_HOME/profiles"
	cp -a /opt/dsh-home/.dsh-image-version "$DSH_HOME/.dsh-image-version"
fi

exec dsh "$@"
