#!/usr/bin/env python3
"""把镜像里烤好的 DSH profile 合并进数据卷（保留用户自己装的东西）。

背景：镜像升级（DSH/插件市场版本变化）时需要刷新 profile 里的官方文件，
但用户可能已经通过插件市场装了自己的插件、收藏、分组、备注，以及手工编辑过
profile 的 `cordis.patch.yml`。直接覆盖会把它们抹掉，因此这里做「非破坏合并」：

- `package.json`：结构以镜像为准，`dependencies` 取并集（卷里的用户插件保留）；
- `cordis.patch.yml` / `cordis.yml` / `state.json` 等用户层文件：卷里已有则保留；
- `node_modules`：镜像里的条目一律以镜像为准刷新（含 pnpm 元数据与插件兼容
  补丁）；用户自己装的插件不在镜像里，继续保留。

用法：dsh-merge-profile.py <镜像 profile 目录> <卷 profile 目录>
"""

import json
import os
import shutil
import sys


def copy_entry(src: str, dst: str) -> None:
    if os.path.islink(src):
        os.symlink(os.readlink(src), dst)
    elif os.path.isdir(src):
        shutil.copytree(src, dst, symlinks=True)
    else:
        shutil.copy2(src, dst)


def remove_entry(path: str) -> None:
    if os.path.islink(path) or os.path.isfile(path):
        os.remove(path)
    elif os.path.isdir(path):
        shutil.rmtree(path)


def merge_package_json(src: str, dst: str) -> None:
    with open(src, encoding="utf-8") as handle:
        image_pkg = json.load(handle)
    user_pkg = {}
    if os.path.exists(dst):
        try:
            with open(dst, encoding="utf-8") as handle:
                user_pkg = json.load(handle)
        except Exception:
            user_pkg = {}

    dependencies = {
        **(image_pkg.get("dependencies") or {}),
        **(user_pkg.get("dependencies") or {}),
    }
    # 名册取并集：镜像自带的 bundle（顺序以镜像为准）在前，用户在插件市场自己
    # 装的插件/主题追加在后——否则镜像一升级，用户的主题就从名册里消失。
    image_bundles = (
        ((image_pkg.get("dsh") or {}).get("profile") or {}).get("bundles") or []
    )
    user_bundles = (
        ((user_pkg.get("dsh") or {}).get("profile") or {}).get("bundles") or []
    )
    bundles = list(image_bundles)
    for name in user_bundles:
        if name not in bundles:
            bundles.append(name)
    # 守护是看门狗，必须最先加载（名册并集后仍强制置顶）
    if "dsh-my-guardian" in bundles:
        bundles.remove("dsh-my-guardian")
        bundles.insert(0, "dsh-my-guardian")

    merged = dict(image_pkg)
    merged["dependencies"] = dependencies
    dsh = dict(image_pkg.get("dsh") or {})
    profile = dict(dsh.get("profile") or {})
    profile["bundles"] = bundles
    dsh["profile"] = profile
    merged["dsh"] = dsh
    # 保留用户自定义的其它顶层字段（版本、脚本等），但 name/dsh/profile 以镜像为准
    for key, value in user_pkg.items():
        if key not in ("name", "private", "dsh", "dependencies"):
            merged[key] = value

    with open(dst, "w", encoding="utf-8") as handle:
        json.dump(merged, handle, ensure_ascii=False, indent=2)
        handle.write("\n")


def merge_dir(image_dir: str, volume_dir: str) -> None:
    os.makedirs(volume_dir, exist_ok=True)
    for name in os.listdir(image_dir):
        src = os.path.join(image_dir, name)
        dst = os.path.join(volume_dir, name)

        if name == "package.json":
            merge_package_json(src, dst)
            continue

        if name == "node_modules" and os.path.isdir(src):
            os.makedirs(dst, exist_ok=True)
            for child in os.listdir(src):
                child_src = os.path.join(src, child)
                child_dst = os.path.join(dst, child)
                # 镜像里的条目（官方插件与各依赖，含 pnpm 的 .modules.yaml）
                # 以镜像为准刷新：store 路径要跟运行期一致，插件的兼容补丁
                # 也要随镜像更新。用户自己装的插件不在镜像 node_modules 里，
                # 因此不受影响、照常保留。
                if os.path.lexists(child_dst):
                    remove_entry(child_dst)
                copy_entry(child_src, child_dst)
            continue

        # 两边都是目录 → 递归合并（例如 profiles/web 下的 package.json 与 node_modules）
        if os.path.isdir(src) and not os.path.islink(src) and os.path.isdir(dst):
            merge_dir(src, dst)
            continue

        # 用户层文件（cordis.patch.yml / cordis.yml / state.json …）：卷里已有则保留
        if os.path.exists(dst):
            continue
        copy_entry(src, dst)


if __name__ == "__main__":
    if len(sys.argv) != 3:
        raise SystemExit("用法：dsh-merge-profile.py <镜像profile目录> <卷profile目录>")
    merge_dir(sys.argv[1], sys.argv[2])
