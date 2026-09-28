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
	GUARDIAN_VERSION="${DSHGUARDIAN_VERSION:-0.4.4}"
	COSTMETER_VERSION="${DSHCOSTMETER_VERSION:-1.7.40}"
	OC2DSH_VERSION="${OPENCODE2DSH_VERSION:-0.3.3}"
	IMAGE="${AGENT_DSH_IMAGE:-localhost/felix-agent-dsh:latest}"
	DIR="$REPO_DIR/containers/agent-dsh"
	BUILD_ARGS=(--build-arg "DSH_VERSION=$VERSION" --build-arg "DSHMARKET_VERSION=$MARKET_VERSION" --build-arg "DSHGUARDIAN_VERSION=$GUARDIAN_VERSION" --build-arg "DSHCOSTMETER_VERSION=$COSTMETER_VERSION" --build-arg "OPENCODE2DSH_VERSION=$OC2DSH_VERSION")
	;;
*)
	printf '未知模板：%s（可选 opencode / dsh）\n' "$KIND" >&2
	exit 1
	;;
esac

log "构建模板镜像 $IMAGE（$KIND $VERSION${MARKET_VERSION:+ / market $MARKET_VERSION}${GUARDIAN_VERSION:+ / guardian $GUARDIAN_VERSION}${COSTMETER_VERSION:+ / cost-meter $COSTMETER_VERSION}${OC2DSH_VERSION:+ / opencode2dsh $OC2DSH_VERSION}）"
podman build "${BUILD_ARGS[@]}" -t "$IMAGE" -f "$DIR/Containerfile" "$DIR"
log "完成：$IMAGE"
