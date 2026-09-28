#!/usr/bin/env python3
"""DSH 0.1.7 兼容补丁：客户端 settingsScope 服务已被 configForms 取代。

背景：opencode2dsh 0.3.3 的浏览器半边把 `settingsScope` 列为硬性注入服务，
而 DSH 0.1.7 起拆掉了该服务（改名/由 @deepseek-ai/dsh-client-ui-settings 的
`configForms` 承担，见上游 issue #20/#24、PR #23）。客户端因此永远停在
`pending (waiting for service: settingsScope)`，web boot 判定条目未激活，
整个界面起不来。

补丁内容（幂等，构建期执行，上游修好后自动跳过）：
  1. package.json 的 `dsh.client.inject`：settingsScope → configForms；
  2. lib/client.js 导出的 inject 数组同步改名；
  3. `ctx.settingsScope.bind({ namespace })` 换成 `ctx.configForms.get(namespace)`
     适配出的旧 scope 面（getSnapshot/subscribe/set/unset，签名一致）。

用法：patch-opencode2dsh-configforms.py <profile 目录>
"""

import json
import sys
from pathlib import Path

PLUGIN = "node_modules/@opencode2dsh/dsh-plugin"
BIND_CALL = "ctx.settingsScope.bind({ namespace: SETTINGS_NAMESPACE })"
ADAPTER = """(() => {
\tconst __form = ctx.configForms.get(SETTINGS_NAMESPACE);
\treturn {
\t\tgetSnapshot: () => __form.getSnapshot(),
\t\tsubscribe: (fn) => __form.subscribe(fn),
\t\tset: (field, value) => __form.set(field, value),
\t\tunset: (field) => __form.unset(field)
\t};
})()"""


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("用法：patch-opencode2dsh-configforms.py <profile 目录>")
    plugin_dir = Path(sys.argv[1]) / PLUGIN
    pkg_path = plugin_dir / "package.json"
    client_path = plugin_dir / "lib/client.js"

    if not pkg_path.exists():
        # 未安装该插件（或用户已卸载）：无需处理，入口脚本每次启动都会调用本脚本
        print("未安装 @opencode2dsh/dsh-plugin，跳过")
        return
    pkg = json.loads(pkg_path.read_text(encoding="utf-8"))
    inject = pkg.get("dsh", {}).get("client", {}).get("inject")
    if not inject:
        raise SystemExit("package.json 缺少 dsh.client.inject，请人工复核")
    if "settingsScope" not in inject:
        if "configForms" in inject:
            print("补丁已生效（或上游已修复），跳过")
            return
        # 上游若改成 package row 形式（PR #23 方案），无需本补丁
        print("inject 未包含 settingsScope（上游已修复），跳过")
        return
    pkg["dsh"]["client"]["inject"] = [
        "configForms" if name == "settingsScope" else name for name in inject
    ]
    pkg_path.write_text(
        json.dumps(pkg, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )

    if not client_path.exists():
        raise SystemExit("lib/client.js 不存在，请人工复核")
    text = client_path.read_text(encoding="utf-8")
    if BIND_CALL not in text:
        raise SystemExit("client.js 未找到 settingsScope 绑定调用点，请人工复核")
    text = text.replace('"settingsScope"', '"configForms"')
    text = text.replace(BIND_CALL, ADAPTER, 1)
    if "settingsScope" in text or "ctx.settingsScope" in text:
        raise SystemExit("client.js 仍有 settingsScope 残留，请人工复核")
    client_path.write_text(text, encoding="utf-8")
    print("opencode2dsh：客户端已迁移到 configForms（settingsScope 兼容补丁）")


if __name__ == "__main__":
    main()
