#!/usr/bin/env python3
"""解锁 DSH 的远程「设置 / 提供商」特权面。

背景：DSH 0.1.7 在客户端把设置文档编辑限定为 loopback（浏览器 host 为
localhost/127.x）才能用，远程域名访问时模型页报
`settings are unavailable in this browser`。该限制**只在客户端**
（`dsh-client-connection` 的 `isLoopback` 布尔），服务端没有特权校验。

本平台每个用户运行在**独立容器**里，且网关（Caddy forward_auth + 主站会话）
已完成账户级鉴权，因此把这一客户端门控放开：远程 Web 与本地部署体验一致，
用户可以自选服务商（DeepSeek 官方 / 自定义 OpenAI 兼容端点）、管理密钥与模型。

补丁是精确字符串替换，找不到目标即构建失败，强制在升级 DSH 时人工复核。
"""

import glob
import os
import sys

PATTERN = (
    "isLoopback: transport?.ownsHost === true || pageLocation === void 0 || "
    "isLoopbackHostname(pageLocation.hostname),"
)
REPLACEMENT = "isLoopback: true,"


def main() -> int:
    found = glob.glob(
        "/opt/fnm/**/node_modules/@deepseek-ai/dsh-client-connection/lib/client.js",
        recursive=True,
    )
    # /opt/fnm/aliases/default 与 node-versions/<ver> 指向同一文件：去重后再校验
    paths = sorted({os.path.realpath(path) for path in found})
    if len(paths) != 1:
        print(f"unlock-remote-settings: 期望恰好一个 client.js，实际 {paths}", file=sys.stderr)
        return 1

    path = paths[0]
    text = open(path, encoding="utf-8").read()
    if PATTERN not in text:
        print(
            "unlock-remote-settings: 未找到 isLoopback 目标表达式（上游可能已改动），"
            "请人工复核后再更新补丁",
            file=sys.stderr,
        )
        return 1

    open(path, "w", encoding="utf-8").write(text.replace(PATTERN, REPLACEMENT, 1))
    print(f"unlock-remote-settings: patched {path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
