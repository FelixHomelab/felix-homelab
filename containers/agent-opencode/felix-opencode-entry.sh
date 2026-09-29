#!/bin/sh
# OpenCode 容器入口包装：为每个用户数据卷播种默认插件配置，然后 exec 原来的 opencode。
#
# 默认启用 agent-cache-optimizer（prompt 前缀缓存优化，稳定块前置、提升 KV 缓存
# 命中率、降低使用成本）：源码随镜像烤自 fork 固定提交，启动时种子到家目录并
# 以路径插件注入；用户已有的 opencode.json 不会被覆盖，只做幂等合并。
set -eu

CFG="${XDG_CONFIG_HOME:-$HOME/.config}/opencode/opencode.json"
# 插件源码随镜像烤在 /opt（fork 固定提交）；但 OpenCode 的路径插件只加载
# 项目/家目录下的目录（/opt 会被静默跳过），因此按镜像版本戳种子到 $HOME 再引用。
SRC="/opt/agent-cache-optimizer"
PLUGIN="$HOME/.local/share/opencode-plugins/agent-cache-optimizer"
if [ -d "$SRC" ]; then
	want="$(cat "$SRC/.felix-rev" 2>/dev/null || echo unknown)"
	have="$(cat "$PLUGIN/.felix-rev" 2>/dev/null || echo none)"
	if [ "$want" != "$have" ] || [ ! -f "$PLUGIN/index.ts" ]; then
		rm -rf "$PLUGIN"
		mkdir -p "$(dirname "$PLUGIN")"
		cp -r "$SRC" "$PLUGIN"
	fi
fi

mkdir -p "$(dirname "$CFG")"
if [ ! -f "$CFG" ]; then
	printf '{\n  "$schema": "https://opencode.ai/config.json",\n  "plugin": ["%s"]\n}\n' "$PLUGIN" >"$CFG"
else
	tmp="$(mktemp)"
	if jq --arg p "$PLUGIN" '
		.plugin = (
			((.plugin // [])
				| map(select((type == "string") and ((startswith("agent-cache-optimizer")) | not))))
			+ [$p] | unique
		)
	' "$CFG" >"$tmp" 2>/dev/null; then
		mv "$tmp" "$CFG"
	else
		rm -f "$tmp"
		echo "[felix-opencode] 警告：$CFG 不是合法 JSON，未注入默认插件" >&2
	fi
fi

exec opencode "$@"
