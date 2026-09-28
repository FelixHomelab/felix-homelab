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

# 运行期从插件市场安装/更新插件会跑 pnpm install，可能把打过兼容补丁的官方
# 插件还原成原版（实测装主题后 opencode2dsh 恢复 settingsScope 依赖，界面起不来）。
# 每次启动都幂等重打一遍；上游修复后脚本自动跳过。
if [ -f /usr/local/bin/patch-opencode2dsh-configforms.py ]; then
	python3 /usr/local/bin/patch-opencode2dsh-configforms.py "$DSH_HOME/profiles/web" \
		|| echo "[dsh-entry] 警告：opencode2dsh 兼容补丁未应用（见上），界面可能异常" >&2
fi

exec dsh "$@"
