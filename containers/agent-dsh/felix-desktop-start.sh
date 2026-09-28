#!/bin/sh
# 容器内虚拟桌面：原生有头 Chromium 的显示载体。
#
#   Xvfb（虚拟显示） + openbox（轻量窗口管理，弹窗/焦点正常）
#   + x11vnc（只监听回环） + noVNC/websockify（网页服务，经 Agent 网关 /vnc 暴露）
#
# 由 dsh-entry.sh 在后台拉起。用户从任何设备打开
# https://<子域>.agent.<域名>/vnc/vnc.html 即可看到并操作容器里真实浏览器窗口，
# 不依赖用户自己的桌面/设备。
set -eu

DISPLAY_NUM="${FELIX_DISPLAY_NUM:-99}"
VNC_PORT="${FELIX_VNC_PORT:-5900}"
NOVNC_PORT="${FELIX_NOVNC_PORT:-6080}"
SIZE="${FELIX_DISPLAY_SIZE:-1440x900}"

mkdir -p /tmp/.X11-unix

# 已在跑就不重复拉起（容器重启后是全新 PID 空间，这里主要防脚本被调用两次）
if [ ! -S "/tmp/.X11-unix/X$DISPLAY_NUM" ]; then
	Xvfb ":$DISPLAY_NUM" -screen 0 "${SIZE}x24" -nolisten tcp -ac \
		>/tmp/felix-xvfb.log 2>&1 &
	i=0
	while [ ! -S "/tmp/.X11-unix/X$DISPLAY_NUM" ] && [ "$i" -lt 100 ]; do
		sleep 0.2
		i=$((i + 1))
	done
fi

DISPLAY=":$DISPLAY_NUM" openbox >/tmp/felix-openbox.log 2>&1 &

x11vnc -display ":$DISPLAY_NUM" -localhost -rfbport "$VNC_PORT" \
	-forever -shared -nopw -quiet >/tmp/felix-x11vnc.log 2>&1 &

# websockify 自带 noVNC 静态资源；只监听容器内网卡，由 Agent 网关经回环端口映射转发
websockify --web /usr/share/novnc "$NOVNC_PORT" "127.0.0.1:$VNC_PORT" \
	>/tmp/felix-novnc.log 2>&1 &

wait
