#!/usr/bin/env python3
"""修补 dsh-my-guardian 的 peer 预检：跳过「宿主提供」的包。

背景：dsh-my-guardian 0.4.4（当前 npm latest）的依赖预检不区分宿主提供的包，
会把守护自己的 `react` peer 报成「缺少依赖 react（请先安装）」并给出
`dsh plugin add react` 建议——而 react/react-dom/@deepseek-ai/* 由宿主前端
运行时注入，装进 profile 反而会多出一份拷贝并遮蔽宿主版本。

上游 main 已改为「宿主提供、无需安装」（#407/#410），但尚未发版。这里在构建期打
等价的精确补丁：peer 预检直接跳过这三类包，不再产生误报与危险建议。

补丁幂等：若文件里已存在 `isHostProvided`（上游已修）则跳过；
目标片段找不到则构建失败，强制升级时人工复核。
"""

import sys

TARGET = "/opt/dsh-home/profiles/web/node_modules/dsh-my-guardian/lib/dep-precheck.js"

HELPERS = """const HOST_PACKAGE_NAMES = new Set(['react', 'react-dom']);
function isHostProvided(spec) {
    const base = spec.startsWith('@') ? spec.split('/').slice(0, 2).join('/') : spec.split('/')[0];
    return base.startsWith('@deepseek-ai/') || HOST_PACKAGE_NAMES.has(base);
}
"""

MARKER_FN = "function classifyPeers(peers, meta, pluginDir, profileDir) {"
MARKER_LOOP = (
    "    const result = examinePeer(dep, range, meta[dep]?.optional === true, pluginDir, profileDir);"
)


def main() -> int:
    text = open(TARGET, encoding="utf-8").read()

    if "isHostProvided" in text:
        print("patch-guardian: 已含宿主包跳过逻辑（上游已修），跳过")
        return 0

    if MARKER_FN not in text or MARKER_LOOP not in text:
        print(
            "patch-guardian: 未找到预期片段（上游结构可能已变），请人工复核",
            file=sys.stderr,
        )
        return 1

    text = text.replace(MARKER_FN, HELPERS + MARKER_FN, 1)
    text = text.replace(
        MARKER_LOOP,
        "    if (isHostProvided(dep)) continue;\n" + MARKER_LOOP,
        1,
    )
    open(TARGET, "w", encoding="utf-8").write(text)
    print("patch-guardian: 已跳过宿主提供的 peer（react / react-dom / @deepseek-ai/*）")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
