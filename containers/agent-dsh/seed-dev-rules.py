#!/usr/bin/env python3
"""向 DSH 数据卷播种 dsh-dev-rules 的平台默认规则。

dev-rules 插件把规则存在 $DSH_HOME/dev-rules.json。平台约定：Agent 的网络
搜索默认走无头浏览器（ego-browser 的 ego_* 工具）+ Bing（国内网络下
Google/DuckDuckGo 不可用）。这里**只填空文档**，绝不覆盖用户已有内容；
用户删掉这条规则（但保留其它内容）后，下次启动不会被重新加回。

用法：seed-dev-rules.py（读 DSH_HOME 环境变量，默认 /data/dsh）
"""

import json
import os
import pathlib
import sys
import tempfile

DEFAULT_RULE = {
    "id": "felix-default-web-search",
    "title": "网络搜索默认用无头浏览器 + Bing",
    "content": (
        "需要联网搜索或查资料时，默认使用无头浏览器工具（ego-browser 插件的 "
        "ego_* 工具）：先用 ego_navigate 打开 "
        "https://www.bing.com/search?q=<URL 编码后的关键词>，再用 ego_snapshot "
        "读取结果并进入目标页面。不要用 curl/wget 或普通 HTTP 抓取搜索引擎结果页"
        "（会被拦截或结果残缺）；国内网络下 Google / DuckDuckGo 不可用，统一使用 "
        "Bing（www.bing.com 或 cn.bing.com）。遇到登录 / 人机验证时，提示用户"
        "通过观察窗接管，不要尝试绕过验证。"
    ),
    "group": "平台默认",
    "enabled": True,
}


def main() -> None:
    home = pathlib.Path(os.environ.get("DSH_HOME") or "/data/dsh")
    path = home / "dev-rules.json"
    doc = None
    if path.exists():
        try:
            doc = json.loads(path.read_text(encoding="utf-8"))
        except Exception as exc:  # 坏文件不碰，交给插件自己报错
            print(f"dev-rules.json 解析失败，跳过播种：{exc}", file=sys.stderr)
            return
    if doc is None:
        doc = {"version": 1, "enabled": True, "global": [], "projects": []}
    elif (doc.get("global") or []) or (doc.get("projects") or []):
        # 用户已有内容，绝不改动
        return

    rules = doc.setdefault("global", [])
    if any(
        isinstance(rule, dict) and rule.get("id") == DEFAULT_RULE["id"]
        for rule in rules
    ):
        return
    rules.insert(0, DEFAULT_RULE)

    home.mkdir(parents=True, exist_ok=True)
    fd, tmp = tempfile.mkstemp(dir=str(home), prefix=".dev-rules.", suffix=".tmp")
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as handle:
            json.dump(doc, handle, ensure_ascii=False, indent=2)
            handle.write("\n")
        os.replace(tmp, path)
        print("dev-rules：已播种平台默认规则（网络搜索走无头浏览器 + Bing）")
    except Exception:
        try:
            os.remove(tmp)
        except OSError:
            pass
        raise


if __name__ == "__main__":
    main()
