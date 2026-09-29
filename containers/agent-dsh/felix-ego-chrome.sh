#!/bin/sh
# ego 浏览器「双引擎」启动包装（EGO_LINUX_CHROME 指向本脚本）。
#
#   无头（默认，EGO_LINUX_HEADLESS=1 或参数带 --headless*）
#       → Obscura（Rust + CDP，实测载入页面 ~40MB；原生截图/串流可用）
#   有头（「弹出窗口」/需要真实 Chromium 的场合）
#       → Chromium，跑在容器虚拟显示上（noVNC 可见；私有会话 D-Bus + --no-sandbox）
#
# ego 运行时按「Chrome 命令行」拉起浏览器并读 <user-data-dir>/DevToolsActivePort；
# Obscura 是「CDP 服务 + 客户端连接」形态，因此这里做协议适配：
#   选端口 → obscura serve → 等 /json/version 就绪 → 写 DevToolsActivePort → 前台守护。
# Obscura 缺失或启动失败时自动回退 Chromium 无头，保证功能不中断。
set -eu

LOG=/tmp/felix-ego-chrome.log
log() { echo "[ego-chrome] $(date +%H:%M:%S) $*" >>"$LOG"; }

headless=0
profile=""
rport=""
for arg in "$@"; do
	case "$arg" in
	--headless*) headless=1 ;;
	--user-data-dir=*) profile="${arg#--user-data-dir=}" ;;
	--remote-debugging-port=*) rport="${arg#--remote-debugging-port=}" ;;
	esac
done
[ -n "$profile" ] || profile=/data/.local/share/ego-browser
[ "$rport" = "0" ] && rport=""

free_port() {
	python3 - <<'PY'
import socket
sock = socket.socket()
sock.bind(("127.0.0.1", 0))
print(sock.getsockname()[1])
sock.close()
PY
}

chromium_headless() {
	log "回退 Chromium 无头（DISPLAY=${DISPLAY:-无}）"
	exec dbus-run-session -- /usr/bin/chromium-browser \
		--no-sandbox --no-first-run --no-default-browser-check "$@"
}

chromium_headed() {
	# 容器里没有无障碍辅助技术，关掉 GTK 的 a11y 桥，避免拉起 at-spi 总线（~10MB）
	export NO_AT_BRIDGE=1
	export GTK_A11Y=none
	log "有头模式 → Chromium（DISPLAY=${DISPLAY:-无}）"
	exec dbus-run-session -- /usr/bin/chromium-browser \
		--no-sandbox --no-first-run --no-default-browser-check "$@"
}

if [ "$headless" != "1" ]; then
	chromium_headed "$@"
fi

# 引擎开关（felix-ego-engine 写入）：默认 obscura；chromium = 有头 Chromium
# （百度百科等反爬页面只有有头能过；无头 Chromium 同样会被识破）
ENGINE="$(cat /data/.ego-engine 2>/dev/null || echo obscura)"
if [ "$ENGINE" = "chromium" ]; then
	# 丢掉运行时的 --headless*，以「有头」启动（虚拟显示上，noVNC 可见）。
	# 原地位过滤：保持每个参数独立（UA 等参数带空格，不能整串拼接）。
	_n=$#
	while [ "$_n" -gt 0 ]; do
		_arg="$1"
		shift
		case "$_arg" in
		--headless*) ;;
		*) set -- "$@" "$_arg" ;;
		esac
		_n=$((_n - 1))
	done
	chromium_headed "$@"
fi

if [ ! -x /opt/obscura/obscura ]; then
	chromium_headless "$@"
fi

port="${rport:-$(free_port)}"
mkdir -p "$profile"
log "无头模式 → Obscura port=$port storage=$profile"

# Obscura 默认拦截内网（SSRF 防护）；Agent 需要访问局域网/本机开发服务，
# 与 Chromium 的行为保持一致，显式放行（容器本身已有网络隔离与鉴权）。
OBSCURA_ALLOW_PRIVATE_NETWORK=1 /opt/obscura/obscura serve \
	--port "$port" \
	--storage-dir "$profile" \
	--stealth \
	--font-dir /usr/share/fonts \
	>>"$LOG" 2>&1 &
pid=$!
trap 'kill "$pid" 2>/dev/null' TERM INT

ready=0
i=0
while [ "$i" -lt 60 ]; do
	if curl -sf --max-time 1 "http://127.0.0.1:$port/json/version" >/dev/null 2>&1; then
		ready=1
		break
	fi
	if ! kill -0 "$pid" 2>/dev/null; then
		log "Obscura 提前退出"
		chromium_headless "$@"
	fi
	sleep 0.25
	i=$((i + 1))
done

if [ "$ready" != "1" ]; then
	log "Obscura 启动超时（15s），回退 Chromium 无头"
	kill "$pid" 2>/dev/null || true
	chromium_headless "$@"
fi

# 运行时读取第一行作为端口，再探测 /json/version 拿 WebSocket 地址
printf '%s\n%s\n' "$port" "/devtools/browser/obscura" >"$profile/DevToolsActivePort"
log "Obscura 就绪（DevToolsActivePort 已写入）"

wait "$pid"
