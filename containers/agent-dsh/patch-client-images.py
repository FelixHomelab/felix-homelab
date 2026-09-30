#!/usr/bin/env python3
"""客户端内嵌大图瘦身（构建期执行）。

背景：DSH 的部分客户端包里把引导欢迎图等以 base64 data URI 内嵌进
`client.js`（settings-account 一个文件就 ~4.8MB），首次加载整包下发，几乎
不可压缩——这是 Web UI 首屏慢/卡的主因。

策略：把内嵌的 PNG 统一转成小尺寸 WebP（保留观感，体积降一个数量级）；
转换失败则退化为 1x1 透明占位。只处理大于阈值的图片，其他内容不动。

另外把 combo 资源 URL 的 `&rev=` 改成 `?rev=`（服务端生成与查表是同一个
字符串，改动天然自洽）：这样路径以 `.js` 结尾，Cloudflare 默认缓存规则即可
命中，多个用户共享一份边缘缓存；DSH 自己的 chunk URL 本来就是这个形式。
"""

import base64
import io
import os
import re
import sys

import PIL
from PIL import Image

MAX_WIDTH = 960
WEBP_QUALITY = 65
MIN_BYTES = 40 * 1024  # 小于该大小的内嵌图不动

PATTERN = re.compile(rb"data:image/(png|jpeg|jpg);base64,([A-Za-z0-9+/=]+)")

ROOTS = [
    "/opt/dsh-home/profiles/web/node_modules",
]
GLOBAL_DSH = "/opt/fnm/node-versions"
if os.path.isdir(GLOBAL_DSH):
    for ver in os.listdir(GLOBAL_DSH):
        roots = os.path.join(
            GLOBAL_DSH, ver, "installation/lib/node_modules/@deepseek-ai/dsh/node_modules"
        )
        if os.path.isdir(roots):
            ROOTS.append(roots)


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


def shrink(data: bytes) -> bytes:
    img = Image.open(io.BytesIO(data))
    if img.width > MAX_WIDTH:
        height = max(1, round(img.height * MAX_WIDTH / img.width))
        img = img.resize((MAX_WIDTH, height), Image.LANCZOS)
    out = io.BytesIO()
    img.save(out, format="WEBP", quality=WEBP_QUALITY, method=6)
    return out.getvalue()


def placeholder() -> bytes:
    out = io.BytesIO()
    Image.new("RGBA", (1, 1), (0, 0, 0, 0)).save(out, format="PNG")
    return out.getvalue()


def patch_file(path: str) -> int:
    src = open(path, "rb").read()
    saved = 0

    def repl(match: "re.Match[bytes]") -> bytes:
        nonlocal saved
        payload = match.group(2)
        if len(payload) < MIN_BYTES:
            return match.group(0)
        try:
            png = base64.b64decode(payload)
            webp = shrink(png)
            if len(webp) >= len(png):
                return match.group(0)
            saved += len(payload) - len(base64.b64encode(webp))
            return b"data:image/webp;base64," + base64.b64encode(webp)
        except Exception as exc:  # noqa: BLE001 - 构建期兜底，不能中断镜像
            print(f"  ! {path}: 转换失败，退化占位：{exc}", file=sys.stderr)
            ph = placeholder()
            saved += len(payload) - len(base64.b64encode(ph))
            return b"data:image/png;base64," + base64.b64encode(ph)

    out = PATTERN.sub(repl, src)
    if out != src:
        open(path, "wb").write(out)
    return saved


COMBO_OLD = 'join(",")}&rev=${rev}'
COMBO_NEW = 'join(",")}?rev=${rev}'


def patch_combo_cache_url() -> None:
    """把 combo URL 的 rev 分隔符移到查询串，命中 CDN 的 .js 默认缓存。"""
    for root in ROOTS:
        for dirpath, _dirs, _files in os.walk(root, followlinks=True):
            if "dsh-client-modules" not in dirpath or not dirpath.endswith("lib"):
                continue
            path = os.path.join(dirpath, "index.js")
            if not os.path.isfile(path):
                continue
            src = open(path, encoding="utf-8").read()
            if COMBO_NEW in src and COMBO_OLD not in src:
                print("[patch-client-images] combo URL 已是 ?rev= 形式")
                return
            if src.count(COMBO_OLD) != 1:
                raise SystemExit(
                    f"[patch-client-images] combo URL 目标不唯一（{path}）："
                    f"{src.count(COMBO_OLD)} 处，升级 DSH 后需人工复核"
                )
            open(path, "w", encoding="utf-8").write(src.replace(COMBO_OLD, COMBO_NEW))
            print("[patch-client-images] combo URL 已改为 ?rev=（CDN 可缓存 .js）")
            return
    raise SystemExit("[patch-client-images] 未找到 dsh-client-modules/lib/index.js")


def main() -> int:
    patch_combo_cache_url()
    total = 0
    count = 0
    for path in client_files():
        saved = patch_file(path)
        if saved:
            count += 1
            total += saved
            print(f"  - {path.replace('/opt/', '')}: -{saved / 1024:.0f}KB")
    print(f"[patch-client-images] Pillow {PIL.__version__}；瘦身 {count} 个文件，共 -{total / 1024 / 1024:.2f}MB")
    return 0


if __name__ == "__main__":
    sys.exit(main())
