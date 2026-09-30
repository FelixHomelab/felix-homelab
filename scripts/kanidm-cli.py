#!/usr/bin/env python3
"""非交互执行 Kanidm CLI 的小助手（登录密码经 pty 自动应答）。

Kanidm CLI 只从 TTY 读密码，无法直接管道喂入；本脚本在 pty 里运行
kanidm/tools 容器，登录会话缓存在命名卷 `kanidm-cli-home`。

用法：
  PW=$(grep -E '^KANIDM_ADMIN_PASSWORD=' ~/.config/felix-homelab/.env | cut -d= -f2-) \
    scripts/kanidm-cli.py 'kanidm system oauth2 list'

  PW=... scripts/kanidm-cli.py        # 不带参数则进入交互式 shell
"""
import os
import pty
import select
import sys

PW = os.environ["PW"]
if len(sys.argv) > 1:
    script = "kanidm login >/dev/null 2>&1; " + sys.argv[1]
else:
    script = "kanidm login && exec sh"

cmd = [
    "podman", "run", "--rm", "-i", "-t", "--network", "host",
    "--add-host", "id.wraindrock.com:127.0.0.1",
    "-v", "kanidm-cli-home:/tmp/k",
    "-e", "HOME=/tmp/k",
    "-e", "KANIDM_URL=https://id.wraindrock.com:8443",
    "-e", "KANIDM_NAME=idm_admin",
    "-e", "KANIDM_ACCEPT_INVALID_CERTS=true",
    "--entrypoint", "sh",
    "docker.io/kanidm/tools:1.11.2",
    "-c", script,
]

pid, fd = pty.fork()
if pid == 0:
    os.execvp(cmd[0], cmd)

os.write(fd, (PW + "\n").encode())
out = []
while True:
    r, _, _ = select.select([fd], [], [], 60)
    if not r:
        break
    try:
        data = os.read(fd, 4096)
    except OSError:
        break
    if not data:
        break
    out.append(data.decode(errors="replace"))

_, status = os.waitpid(pid, 0)
text = "".join(out).replace("\r", "")
for line in text.splitlines():
    if "verify_ca" in line or line.strip() == PW:
        continue
    print(line)
sys.exit(os.waitstatus_to_exitcode(status))
