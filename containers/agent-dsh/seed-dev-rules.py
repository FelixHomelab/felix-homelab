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

# 反爬回退规则：无头引擎（Obscura）过不了安全验证时，切到有头 Chromium
FALLBACK_RULE = {
    "id": "felix-default-antibot-fallback",
    "title": "反爬/安全验证时切换浏览器引擎",
    "content": (
        "浏览器默认用省内存的 Obscura 引擎；如果页面标题出现「安全验证 / 验证码 / "
        "Just a moment / 人机验证」，或 ego_snapshot 返回空内容："
        "① 在 shell 里执行 `felix-ego-engine chromium`（切到有头真实 Chromium，"
        "反爬页面只有它能过）；② 重新打开任务空间再访问该页；"
        "③ 处理完这类站点后，可执行 `felix-ego-engine obscura` 切回省内存模式。"
        "不要反复用同一引擎重试被拦的页面。"
    ),
    "group": "平台默认",
    "enabled": True,
}


def main() -> None:
    home = pathlib.Path(os.environ.get("DSH_HOME") or "/data/dsh")
    path = home / "dev-rules.json"
    marker = home / ".dev-rules-platform-v1"
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
        # 用户已有内容：只做一次性的平台规则迁移（用标记保证不反悔：
        # 用户之后自行删除就不会被重新加回）
        if not marker.exists():
            rules = doc.setdefault("global", [])
            if not any(isinstance(r, dict) and r.get("id") == FALLBACK_RULE["id"] for r in rules):
                rules.insert(0, FALLBACK_RULE)
                _write(home, path, doc)
                marker.write_text("ok\n", encoding="utf-8")
                print("dev-rules：已补种平台规则（反爬回退）")
        return

    rules = doc.setdefault("global", [])
    existing = {rule.get("id") for rule in rules if isinstance(rule, dict)}
    if DEFAULT_RULE["id"] in existing and FALLBACK_RULE["id"] in existing:
        return
    if FALLBACK_RULE["id"] not in existing:
        rules.insert(0, FALLBACK_RULE)
    if DEFAULT_RULE["id"] not in existing:
        rules.insert(0, DEFAULT_RULE)

    _write(home, path, doc)
    marker.write_text("ok\n", encoding="utf-8")
    print("dev-rules：已播种平台默认规则（搜索规则 + 反爬回退）")


def _write(home: pathlib.Path, path: pathlib.Path, doc: dict) -> None:
    home.mkdir(parents=True, exist_ok=True)
    fd, tmp = tempfile.mkstemp(dir=str(home), prefix=".dev-rules.", suffix=".tmp")
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as handle:
            json.dump(doc, handle, ensure_ascii=False, indent=2)
            handle.write("\n")
        os.replace(tmp, path)
    except Exception:
        try:
            os.remove(tmp)
        except OSError:
            pass
        raise


if __name__ == "__main__":
    main()
