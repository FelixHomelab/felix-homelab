#!/bin/sh
# 切换 ego 无头浏览器的引擎（写 /data/.ego-engine，包装脚本下次启动时生效）：
#
#   felix-ego-engine obscura   省内存（默认，~40-60MB；Bing/百度搜索等普通页面够用）
#   felix-ego-engine chromium  有头真实 Chromium（跑在容器虚拟显示上，noVNC 可见；
#                              百度百科/知乎等反爬页面只有它过得去；内存 ~300MB+）
#
# 切换会结束当前浏览器进程（任务空间会随下一次工具调用按新模式重建）。
set -eu

MODE="${1:-}"
case "$MODE" in
obscura | chromium) ;;
*)
	echo "用法：felix-ego-engine obscura|chromium" >&2
	exit 2
	;;
esac

printf '%s\n' "$MODE" >/data/.ego-engine

# 结束当前引擎进程（Obscura 与 Chromium 都可能残留）
me=$$
for p in /proc/[0-9]*; do
	pid=${p#/proc/}
	[ "$pid" = "$me" ] && continue
	comm=$(cat "$p/comm" 2>/dev/null) || continue
	case "$comm" in
	obscura* | chrome | chrome_crashpad | chromium*) kill -9 "$pid" 2>/dev/null || true ;;
	esac
done

# 清掉旧的就绪文件，避免运行时误连已死的端口
rm -f /data/.local/share/ego-lite-linux/profile/DevToolsActivePort 2>/dev/null || true
find /data -maxdepth 5 -name DevToolsActivePort -delete 2>/dev/null || true

case "$MODE" in
obscura) DESC="Obscura（省内存）" ;;
chromium) DESC="Chromium 有头（可过反爬；可在观察窗/noVNC 看到）" ;;
esac
echo "ego 引擎已切换为：$DESC（当前浏览器已停止，下一次 ego_* 调用生效）"
