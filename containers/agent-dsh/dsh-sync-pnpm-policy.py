#!/usr/bin/env python3
"""把镜像 bake 的 pnpm 供应链策略并进数据卷的 pnpm-workspace.yaml。

背景：DSH 0.2 起 pnpm 会对 lockfile 做「最小发布年龄」审计
（ERR_PNPM_MINIMUM_RELEASE_AGE_VIOLATION）。镜像烘焙时 `dsh plugin add`
会自动把超新包写进 `minimumReleaseAgeExclude`，但卷内 workspace 是用户层，
profile 合并会保留旧内容——用户之后装/升级插件就会被策略拦截。

这里按节（minimumReleaseAgeExclude 列表 / allowBuilds 映射）做并集：
镜像的条目补进卷里，卷里已有的保留；其余内容原样不动。

用法：dsh-sync-pnpm-policy.py <卷 profile 目录> <镜像 profile 目录>
"""

import os
import sys

KEYS = ("minimumReleaseAgeExclude", "allowBuilds")


def parse_sections(lines):
    """按「顶格 `key:` + 缩进内容行」切段。返回 (顺序, {key: [内容行]})。"""
    order = []
    sections = {}
    current = None
    for line in lines:
        stripped = line.rstrip("\n")
        if stripped and not stripped[0].isspace() and stripped.endswith(":"):
            current = stripped[:-1]
            order.append(current)
            sections[current] = []
        elif current is not None and stripped.strip():
            sections[current].append(stripped)
    return order, sections


def item_ident(line, key):
    """取条目标识：列表按包名（忽略 @version），映射按键名。"""
    body = line.strip().lstrip("- ").strip()
    name = body.split(":")[0].strip().strip('"')
    if key != "minimumReleaseAgeExclude":
        return name
    # 同名旧版本会遮蔽新版本（pnpm 按包名匹配），因此按包名归并
    if name.startswith("@"):
        scope, _, rest = name.partition("/")
        return scope + "/" + rest.split("@")[0]
    return name.split("@")[0]


def merge_sections(image_lines, user_lines):
    image_order, image_sections = parse_sections(image_lines)
    user_order, user_sections = parse_sections(user_lines)
    for key in KEYS:
        if key not in image_sections:
            continue
        merged = []
        occupied = set()
        # 镜像条目优先：同名（列表按包名）覆盖卷里的旧条目
        for line in image_sections[key]:
            merged.append(line)
            occupied.add(item_ident(line, key))
        for line in user_sections.get(key, []):
            ident = item_ident(line, key)
            if ident in occupied:
                continue
            if any(item_ident(kept, key) == ident for kept in merged):
                continue
            merged.append(line)
            occupied.add(ident)
        user_sections[key] = merged
        if key not in user_order:
            user_order.append(key)
    return user_order, user_sections


def render(order, sections, original_lines):
    """以原文件为底重渲染：保留原有段内容与顺序，补上缺失的节。"""
    out = []
    emitted = set()
    i = 0
    while i < len(original_lines):
        line = original_lines[i].rstrip("\n")
        if line and not line[0].isspace() and line.endswith(":"):
            key = line[:-1]
            emitted.add(key)
            out.append(line)
            out.extend(sections.get(key, []))
            i += 1
            while i < len(original_lines):
                nxt = original_lines[i].rstrip("\n")
                if nxt and not nxt[0].isspace() and nxt.endswith(":"):
                    break
                i += 1
            continue
        out.append(line)
        i += 1
    for key in order:
        if key in emitted or not sections.get(key):
            continue
        out.append(f"{key}:")
        out.extend(sections[key])
    return "\n".join(out) + "\n"


def main() -> int:
    if len(sys.argv) != 3:
        print(__doc__)
        return 2
    user_dir, image_dir = sys.argv[1], sys.argv[2]
    user_path = os.path.join(user_dir, "pnpm-workspace.yaml")
    image_path = os.path.join(image_dir, "pnpm-workspace.yaml")
    if not os.path.isfile(user_path) or not os.path.isfile(image_path):
        return 0

    with open(user_path, encoding="utf-8") as handle:
        user_lines = handle.read().splitlines()
    with open(image_path, encoding="utf-8") as handle:
        image_lines = handle.read().splitlines()

    order, sections = merge_sections(image_lines, user_lines)
    merged = render(order, sections, user_lines)

    before = "\n".join(user_lines) + "\n"
    if merged != before:
        with open(user_path, "w", encoding="utf-8") as handle:
            handle.write(merged)
        print("[dsh-sync-pnpm-policy] 已并入镜像的供应链策略豁免/allowBuilds")
    return 0


if __name__ == "__main__":
    sys.exit(main())
