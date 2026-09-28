#!/usr/bin/env python3
"""放开 DSH 的 `--host 0.0.0.0` 保护（仅当环境变量显式开启）。

背景：DSH 拒绝把 Web 服务绑到 0.0.0.0（防止把 RCE 暴露到网络），这是正确的
默认。但本平台给每个 Agent 分配**独立 bridge 网络**，容器内 0.0.0.0 只对
该网络与本机端口映射可见，公网仍必须经网关鉴权；不绑 0.0.0.0 则宿主端口
映射无法转发到容器（进程只听回环）。

因此把该校验改为**环境变量门控**：只有我们托管的容器会设置
`DSH_ALLOW_NON_LOOPBACK=1`，其他场景（本机/用户自行部署）仍保持拒绝。

补丁幂等：已含该变量则跳过；目标片段找不到则构建失败，强制升级时复核。
"""

import glob
import os
import sys

PATTERN = 'if (options.host === "0.0.0.0") program.error('
REPLACEMENT = (
    'if (options.host === "0.0.0.0" && !process.env.DSH_ALLOW_NON_LOOPBACK) program.error('
)


def main() -> int:
    found = glob.glob(
        "/opt/fnm/**/node_modules/@deepseek-ai/dsh-web-app/lib/startup.js",
        recursive=True,
    )
    paths = sorted({os.path.realpath(path) for path in found})
    if len(paths) != 1:
        print(f"allow-nonloopback: 期望恰好一个 startup.js，实际 {paths}", file=sys.stderr)
        return 1

    path = paths[0]
    text = open(path, encoding="utf-8").read()
    if "DSH_ALLOW_NON_LOOPBACK" in text:
        print("allow-nonloopback: 已含环境变量门控（上游已改或已打过补丁），跳过")
        return 0
    if PATTERN not in text:
        print(
            "allow-nonloopback: 未找到 0.0.0.0 校验片段（上游可能已改），请人工复核",
            file=sys.stderr,
        )
        return 1

    open(path, "w", encoding="utf-8").write(text.replace(PATTERN, REPLACEMENT, 1))
    print("allow-nonloopback: 已改为 DSH_ALLOW_NON_LOOPBACK 门控")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
