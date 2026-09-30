#!/usr/bin/env python3
"""插件客户端包构建期 minify（esbuild）。

DSH 的插件 client 包是未压缩的编译产物（首屏两个 preload 组合包 ~11MB），
在慢链路上首访需数十秒。这里在构建期用 esbuild 统一压缩；只在该文件确实
变小时替换，单个文件失败不影响其他文件。

注意：不要加 `--keep-names`。实测其 `__name()` 包装会让
`@deepseek-ai/dsh-client-ui-settings` 在客户端导入时抛
`TypeError: Property description must be an object: undefined`，
并连锁导致 configForms/locale 等 52 个客户端条目 pending（客户端启动失败）。
"""

import os
import subprocess
import sys

ROOTS = ["/opt/dsh-home/profiles/web/node_modules"]
GLOBAL_DSH = "/opt/fnm/node-versions"
if os.path.isdir(GLOBAL_DSH):
    for ver in os.listdir(GLOBAL_DSH):
        root = os.path.join(
            GLOBAL_DSH, ver, "installation/lib/node_modules/@deepseek-ai/dsh/node_modules"
        )
        if os.path.isdir(root):
            ROOTS.append(root)


def client_files():
    seen = set()
    for root in ROOTS:
        if not os.path.isdir(root):
            continue
        for dirpath, _dirs, files in os.walk(root, followlinks=True):
            for name in files:
                if name != "client.js" and not (
                    name.endswith(".js") and os.path.basename(dirpath) == "client"
                ):
                    continue
                real = os.path.realpath(os.path.join(dirpath, name))
                if real in seen:
                    continue
                seen.add(real)
                yield real


def minify(path: str) -> int:
    before = os.path.getsize(path)
    tmp = path + ".min"
    proc = subprocess.run(
        [
            "esbuild",
            path,
            "--minify",
            "--legal-comments=none",
            "--target=es2022",
            f"--outfile={tmp}",
        ],
        capture_output=True,
        text=True,
    )
    if proc.returncode != 0:
        print(f"  ! {path}: esbuild 失败，保持原样\n    {proc.stderr.strip()[:200]}", file=sys.stderr)
        if os.path.exists(tmp):
            os.unlink(tmp)
        return 0
    after = os.path.getsize(tmp)
    if after < before:
        os.replace(tmp, path)
        return before - after
    os.unlink(tmp)
    return 0


def main() -> int:
    total = 0
    count = 0
    for path in client_files():
        saved = minify(path)
        if saved:
            count += 1
            total += saved
    print(f"[minify-client-bundles] 压缩 {count} 个文件，共 -{total / 1024 / 1024:.2f}MB")
    return 0


if __name__ == "__main__":
    sys.exit(main())
