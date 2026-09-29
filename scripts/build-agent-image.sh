#!/usr/bin/env bash
#
# 构建多租户 Agent 模板镜像。
#
#   scripts/build-agent-image.sh [opencode|dsh]     默认 opencode
#
# 版本来源优先级：环境变量 > ~/.config/felix-homelab/.env > Containerfile 默认值。
# 固定版本是 Agent 稳定性的前提，请勿在生产使用 latest。
set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/felix-homelab"
KIND="${1:-opencode}"

# shellcheck disable=SC1091
if [ -f "$CONFIG_DIR/.env" ]; then
	set -a
	. "$CONFIG_DIR/.env"
	set +a
fi

log() { printf '\033[1;36m[agent]\033[0m %s\n' "$*"; }

case "$KIND" in
opencode)
	VERSION="${OPENCODE_VERSION:-2.0.18}"
	IMAGE="${AGENT_IMAGE:-localhost/felix-agent-opencode:latest}"
	DIR="$REPO_DIR/containers/agent-opencode"
	BUILD_ARGS=(--build-arg "OPENCODE_VERSION=$VERSION")
	;;
dsh)
	VERSION="${DSH_VERSION:-0.1.7-rc.2}"
	MARKET_VERSION="${DSHMARKET_VERSION:-1.66.3}"
	GUARDIAN_REF="${DSHGUARDIAN_REF:-1bca78ed0329fa921e447ea12eedbcf3990e1179}"
	COSTMETER_VERSION="${DSHCOSTMETER_VERSION:-1.7.40}"
	OC2DSH_REF="${OPENCODE2DSH_REF:-d5e866ae77b30d66212a7cfb87921a870a3bdc54}"
	SIDEBAR_VERSION="${DSHBETTERSIDEBAR_VERSION:-0.22.1}"
	EGOBROWSER_REF="${DSHEGOBROWSER_REF:-e20e90a18744a79ba19e9af3d918d5e8573dd68f}"
	DEV_RULES_REF="${DSHDEV_RULES_REF:-83c5ff329a1ecb9e8dc37da02eee17998f904dee}"
	OBSCURA_VER="${OBSCURA_VERSION:-v0.2.3}"
	IMAGE="${AGENT_DSH_IMAGE:-localhost/felix-agent-dsh:latest}"
	DIR="$REPO_DIR/containers/agent-dsh"
	BUILD_ARGS=(--build-arg "DSH_VERSION=$VERSION" --build-arg "DSHMARKET_VERSION=$MARKET_VERSION" --build-arg "DSHGUARDIAN_REF=$GUARDIAN_REF" --build-arg "DSHCOSTMETER_VERSION=$COSTMETER_VERSION" --build-arg "OPENCODE2DSH_REF=$OC2DSH_REF" --build-arg "DSHBETTERSIDEBAR_VERSION=$SIDEBAR_VERSION" --build-arg "DSHEGOBROWSER_REF=$EGOBROWSER_REF" --build-arg "DSHDEV_RULES_REF=$DEV_RULES_REF" --build-arg "OBSCURA_VERSION=$OBSCURA_VER")
	;;
*)
	printf '未知模板：%s（可选 opencode / dsh）\n' "$KIND" >&2
	exit 1
	;;
esac

# 版本固化：除 :latest 外再打一个不可变版本标签（回滚用），并只保留最近
# AGENT_IMAGE_KEEP 个版本标签（默认 3）。正在被实例使用的镜像不会被删。
prune_old_versions() {
	local repo="$1" current="$2" keep="$3" tag full kept=0
	[[ "$keep" =~ ^[0-9]+$ ]] || keep=3
	while IFS= read -r full; do
		tag="${full#"$repo":}"
		[ "$tag" = "latest" ] && continue
		[[ "$tag" =~ ^v?[0-9] ]] || continue
		[ "$tag" = "$current" ] && continue
		if [ "$kept" -lt "$keep" ]; then
			kept=$((kept + 1))
			continue
		fi
		if podman image rm "$full" >/dev/null 2>&1; then
			log "清理旧版本标签：$full"
		else
			warn "旧版本仍被容器使用，保留：$full"
		fi
	done < <(podman images --format '{{.CreatedAt}}|{{.Repository}}:{{.Tag}}' \
		| sort -r \
		| awk -F'|' -v r="$repo" 'index($2, r ":") == 1 { print $2 }')
}

log "构建模板镜像 $IMAGE（$KIND $VERSION${MARKET_VERSION:+ / market $MARKET_VERSION}${GUARDIAN_REF:+ / guardian ${GUARDIAN_REF:0:8}}${COSTMETER_VERSION:+ / cost-meter $COSTMETER_VERSION}${OC2DSH_REF:+ / opencode2dsh ${OC2DSH_REF:0:8}}${SIDEBAR_VERSION:+ / better-sidebar $SIDEBAR_VERSION}${EGOBROWSER_REF:+ / ego-browser ${EGOBROWSER_REF:0:8}}${DEV_RULES_REF:+ / dev-rules ${DEV_RULES_REF:0:8}}）"
podman build "${BUILD_ARGS[@]}" -t "$IMAGE" -f "$DIR/Containerfile" "$DIR"
REPO="${IMAGE%:*}"
podman tag "$IMAGE" "$REPO:$VERSION"
prune_old_versions "$REPO" "$VERSION" "${AGENT_IMAGE_KEEP:-3}"
log "完成：$IMAGE（版本标签 $REPO:$VERSION；agent-ctl.sh versions 查看留存）"
