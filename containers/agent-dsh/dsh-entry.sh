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

# npm/pnpm 不会保留包内脚本的可执行位，而 dsh-ego-browser 依赖自带 wrapper
# （root/容器下加 --no-sandbox）去拉起 Chromium；缺位会 EACCES 起不了浏览器。
# 每次启动补齐；插件重装后同样自动恢复。
if [ -d "$DSH_HOME/profiles/web/node_modules/dsh-ego-browser/bin" ]; then
	chmod 0755 "$DSH_HOME/profiles/web/node_modules/dsh-ego-browser/bin"/*.sh 2>/dev/null || true
fi

# 第三方插件会以「静态 import」引用 DSH 内部包（dsh-webchat → dsh-settings、
# dsh-llm 等），而 pnpm 只把它们装在全局 dsh 包自己的 node_modules 里，profile
# 侧解析不到（表现为 failed to import）。这里把全局 @deepseek-ai/* 全部软链进
# profile：同一 realpath → Node 仍解析到同一模块实例，服务单例语义不变。
DSH_REAL="$(readlink -f "$(command -v dsh)" 2>/dev/null || true)"
if [ -n "$DSH_REAL" ]; then
	GLOBAL_AI="$(dirname "$(dirname "$DSH_REAL")")/node_modules/@deepseek-ai"
	PROFILE_AI="$DSH_HOME/profiles/web/node_modules/@deepseek-ai"
	if [ -d "$GLOBAL_AI" ] && [ -d "$PROFILE_AI" ]; then
		for pkg in "$GLOBAL_AI"/*; do
			[ -e "$pkg" ] || continue
			name="$(basename "$pkg")"
			[ -e "$PROFILE_AI/$name" ] || ln -s "$pkg" "$PROFILE_AI/$name" 2>/dev/null || true
		done
	fi
fi

# 容器内虚拟桌面：让 ego-browser 以「原生有头」方式运行在虚拟显示上，
# 用户经 <子域>/vnc/vnc.html 跨设备查看/操作真实浏览器窗口。
if [ "${FELIX_DESKTOP:-1}" = "1" ] && [ -x /usr/local/bin/felix-desktop-start.sh ]; then
	if [ ! -S "/tmp/.X11-unix/X${FELIX_DISPLAY_NUM:-99}" ]; then
		setsid /usr/local/bin/felix-desktop-start.sh >/tmp/felix-desktop.log 2>&1 &
	fi
	export DISPLAY=":${FELIX_DISPLAY_NUM:-99}"
fi

# OpenAI 兼容的模型中转（Go 原生，固化协议边界）：DSH 侧以「自定义 OpenAI
# 提供商」接入 http://127.0.0.1:3160/v1；上游真实 API Key 只在本容器配置里。
if [ -x /usr/local/bin/felix-llm-relay ]; then
	FELIX_RELAY_CONFIG="${FELIX_RELAY_CONFIG:-/data/llm-relay.json}" \
		setsid /usr/local/bin/felix-llm-relay >>/tmp/felix-llm-relay.log 2>&1 &
fi

# 平台默认开发规则：网络搜索走无头浏览器 + Bing。仅在装了 dsh-dev-rules 且
# 规则文件为空/不存在时播种，不覆盖用户已有规则。
if [ -d "$DSH_HOME/profiles/web/node_modules/dsh-dev-rules" ] \
	&& [ -f /usr/local/bin/seed-dev-rules.py ]; then
	python3 /usr/local/bin/seed-dev-rules.py \
		|| echo "[dsh-entry] 警告：dev-rules 默认规则未播种（见上）" >&2
fi

exec dsh "$@"
