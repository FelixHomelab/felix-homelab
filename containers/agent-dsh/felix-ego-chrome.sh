#!/bin/sh
# ego-browser 的浏览器启动包装（EGO_LINUX_CHROME 指向本脚本）。
#
# 容器里以「原生有头」方式在虚拟显示上跑真实 Chromium，用户经 noVNC 跨设备接管：
#   · 必须带一个私有会话 D-Bus：缺 D-Bus 时 GTK 初始化会卡死（实测有头模式
#     永不启动 DevTools，加 dbus-run-session 后 2 秒即绪）；
#   · root/容器下必须 --no-sandbox（宿主 unshare 不允许内核沙箱）；
#   · 关掉首启对话框，避免挡住 DevTools 就绪。
set -eu

# 容器里没有无障碍辅助技术，关掉 GTK 的 a11y 桥，避免拉起 at-spi 总线（~10MB）
export NO_AT_BRIDGE=1
export GTK_A11Y=none

exec dbus-run-session -- /usr/bin/chromium-browser \
	--no-sandbox \
	--no-first-run \
	--no-default-browser-check \
	"$@"
