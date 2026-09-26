#!/usr/bin/env bash
#
# 注册并启动 Forgejo Actions Runner
#
# 使用 Forgejo 自带的离线注册（forgejo-cli actions register），
# 通过共享密钥完成注册，全过程无需交互，可重复执行（幂等）。
#
set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/felix-workstation"
CONFIG_FILE="$CONFIG_DIR/runner-config.yml"
CRED_FILE="$CONFIG_DIR/runner.secret"
RUNNER_IMAGE="${RUNNER_IMAGE:-data.forgejo.org/forgejo/runner:13}"
FORGEJO_CONTAINER="${FORGEJO_CONTAINER:-felix-workstation-forgejo}"

log() { printf '\033[1;36m[felix]\033[0m %s\n' "$*"; }
die() { printf '\033[1;31m[felix]\033[0m %s\n' "$*" >&2; exit 1; }

podman container exists "$FORGEJO_CONTAINER" || die "Forgejo 容器未运行，请先执行 make install"

# 1. 读取或生成共享密钥（40 位十六进制，恰好 20 字节）
if [ -f "$CRED_FILE" ]; then
	SECRET="$(tr -d '\r\n' < "$CRED_FILE")"
else
	SECRET="$(openssl rand -hex 20)"
	printf '%s\n' "$SECRET" > "$CRED_FILE"
	chmod 600 "$CRED_FILE"
fi
[ "${#SECRET}" -eq 40 ] || die "runner.secret 必须是 40 位十六进制字符串"

# 2. 幂等注册，取回 uuid
UUID="$(podman exec --user 1000 -w /data/gitea "$FORGEJO_CONTAINER" \
	forgejo forgejo-cli actions register --config /data/gitea/conf/app.ini \
	--name felix-workstation --secret "$SECRET" 2>/dev/null \
	| grep -oiE '[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}' | tail -n1)"
[ -n "$UUID" ] || die "注册 Runner 失败，未取得 uuid"
log "Runner uuid: $UUID"

# 3. 基于当前 Runner 镜像生成默认配置，再注入必要设置
podman run --rm "$RUNNER_IMAGE" forgejo-runner generate-config > "$CONFIG_FILE"

sed -i \
	-e '/A_TEST_ENV_NAME_1/d' \
	-e '/A_TEST_ENV_NAME_2/d' \
	-e 's/^  envs:$/  envs: {}/' \
	-e 's|^  env_file: .env$|  env_file: ""|' \
	-e 's|^  dir: ""|  dir: /data/cache|' \
	-e 's|^  network: ""$|  network: host|' \
	"$CONFIG_FILE"

# 3b. 构建标签镜像（含 Podman archive-copy 回归的本地修复），再注入标签
[ -f "$CONFIG_DIR/runner-labels.txt" ] || die "缺少标签定义文件 $CONFIG_DIR/runner-labels.txt（可执行 make install 生成）"
"$REPO_DIR/scripts/build-runner-images.sh" "$CONFIG_DIR/runner-labels.txt"
LABELS_FILE="$CONFIG_DIR/runner-labels.resolved.txt"
[ -f "$LABELS_FILE" ] || LABELS_FILE="$CONFIG_DIR/runner-labels.txt"
LABELS_BLOCK="  labels:
$(awk '!/^[[:space:]]*(#|$)/ { sub(/^[[:space:]]+/, ""); printf "    - \"%s\"\n", $0 }' "$LABELS_FILE")"

LABELS="$LABELS_BLOCK" awk '
	/^  labels: \[\]$/ { next }
	/^runner:$/ { print; print ENVIRON["LABELS"]; next }
	{ print }
' "$CONFIG_FILE" > "$CONFIG_FILE.new" && mv "$CONFIG_FILE.new" "$CONFIG_FILE"

# 默认配置末尾已有 "server:\n  connections:"，直接追加连接。
#
# 网络说明：
#   - Runner 在 Pod 内，直连 127.0.0.1:3000 最可靠；
#   - 由 Runner 派发的作业容器在 Pod 之外，无法访问 127.0.0.1，
#     因此通过 runner.envs 把作业的 GITHUB_SERVER_URL 指向
#     host.containers.internal:5730（Podman 自动写入 /etc/hosts 的宿主网关）。
cat >> "$CONFIG_FILE" <<EOF
    forgejo:
      url: http://127.0.0.1:3000/
      uuid: $UUID
      token: $SECRET
EOF

# 校验配置确实写入成功，避免在文件落盘前重启
grep -q 'url: http://127.0.0.1:3000/' "$CONFIG_FILE" || die "runner 配置写入失败"
sync "$CONFIG_FILE" 2>/dev/null || sync

# 4. 安装单元、启动 Runner
UNIT_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/containers/systemd"
log "链接 Quadlet 单元并启动 Runner"
ln -sfn "$REPO_DIR/quadlet/felix-workstation-runner.container" \
	"$UNIT_DIR/felix-workstation-runner.container"
systemctl --user daemon-reload
systemctl --user reset-failed felix-workstation-runner.service 2>/dev/null || true
systemctl --user restart felix-workstation-runner.service
sleep 2
systemctl --user is-active --quiet felix-workstation-runner.service \
	|| die "Runner 启动失败，请查看 journalctl --user -u felix-workstation-runner.service"
log "Runner 已启动"
