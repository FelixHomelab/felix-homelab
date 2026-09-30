#!/home/felix/.cache/felix-homelab-tools/cdpvenv/bin/python
"""非交互执行 Kanidm CLI 的小助手（动态应答登录/reauth 密码，输出实时流式打印）。

Kanidm CLI 只从 TTY 读密码；高特权操作在特权窗口过期后会再次要求 reauth。
本脚本在 pty 里运行 kanidm/tools 容器，凡是遇到密码类提示就自动输入 PW。
登录会话缓存在命名卷 `kanidm-cli-home`。

用法：
  PW=$(grep -E '^KANIDM_ADMIN_PASSWORD=' ~/.config/felix-homelab/.env | cut -d= -f2-) \
    scripts/kanidm-cli.py 'kanidm system oauth2 list'
"""
import os
import sys

import pexpect

PW = os.environ["PW"]
if len(sys.argv) > 1:
    script = "kanidm logout >/dev/null 2>&1; kanidm login >/dev/null 2>&1; " + sys.argv[1]
else:
    script = "kanidm logout >/dev/null 2>&1; kanidm login && exec sh"

args = [
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

class _Redactor:
    """过滤输出中的密码回显（pty 在某些提示下会回显输入）。"""

    def __init__(self, stream, secret):
        self.stream = stream
        self.secret = secret

    def write(self, text):
        self.stream.write(text.replace(self.secret, "***"))
        return len(text)

    def flush(self):
        self.stream.flush()


child = pexpect.spawn(args[0], args[1:], encoding="utf-8", timeout=30, dimensions=(50, 200))
child.logfile_read = _Redactor(sys.stdout, PW)  # 实时流式输出（已脱敏）
while True:
    idx = child.expect(
        [r"(?i)password for", r"(?i)password:", r"(?i)reauthenticate", r"(?i)enter\s",
         r"\[Y/n\]", r"\[y/N\]", pexpect.EOF, pexpect.TIMEOUT]
    )
    if idx <= 5:
        child.sendline(PW if idx <= 3 else "y")
    elif idx == 6:  # EOF
        break
    # timeout：命令仍在跑，继续等
child.close()
sys.exit(child.exitstatus or 0)
