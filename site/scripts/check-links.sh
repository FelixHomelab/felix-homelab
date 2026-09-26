#!/usr/bin/env bash
# 站点自检：路由可达性 + 站内链接完整性。
#
# 项目没有单元测试——这一层全是「渲染出来的 HTML 对不对」，用 Rust 测试去断言
# HTML 字符串只会写成脆弱的快照断言。真正的判据是**打一遍真实服务**：这一页打得开
# 吗、页面上每个站内链接指得到东西吗。所以这个脚本是本站的验收命令，不是辅助工具。
#
# 用法：
#   ./scripts/check-links.sh                      # 默认 http://127.0.0.1:8080
#   BASE_URL=https://example.com ./scripts/check-links.sh
#
# 退出码：0 全部通过；1 有路由或链接不合格。
set -uo pipefail

BASE_URL="${BASE_URL:-http://127.0.0.1:8080}"
TIMEOUT="${TIMEOUT:-10}"

# 这些页面的 HTML 会被抓下来，其中的站内链接逐个核验。
# 有意包含每个板块的代表页：首页、列表、详情、分类、静态页、动态路由。
PAGES=(
  "/"
  "/blog"
  "/blog/writing-guide"
  "/blog/site-rewrite"
  "/projects"
  "/projects/nebula"
  "/projects/grant-felix-homepage"
  "/sky"
  "/sky/gameplay"
  "/sky/gallery"
  "/sky/boosting"
  "/about"
  "/contact"
  "/login"
  "/register"
  "/rss.xml"
)

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

fail=0
note() { printf '%s\n' "$*"; }

# 取 HTTP 状态码；连不上时打印 `000`。
#
# `|| true` 是必须的：curl 连不上时会**既**输出 `000` 又以非 0 退出，写成 `|| echo 000`
# 会拼出 `000000` 这种看不懂的码（踩过）。状态码只从 -w 取，退出码这里不关心。
http_code() {
  curl -s --max-time "$TIMEOUT" -o "$1" -w '%{http_code}' "$2" 2>/dev/null || true
}

# 刚重启完的服务偶尔会拒一次连接。000 是「压根没连上」，隔一秒重试一次再判死：
# 真挂了的服务两次都连不上，只有瞬时抖动会被这一次重试吃掉。
probe() {
  local code
  code="$(http_code "$1" "$2")"
  if [ "$code" = "000" ]; then
    sleep 1
    code="$(http_code "$1" "$2")"
    [ "$code" = "000" ] && code="000(重试后仍连不上)"
  fi
  printf '%s' "$code"
}

note "== 路由可达性（$BASE_URL）=="
for p in "${PAGES[@]}"; do
  code="$(probe "$TMP/page.html" "$BASE_URL$p")"
  if [ "$code" != "200" ]; then
    note "  [FAIL $code] $p"
    fail=$((fail + 1))
  else
    # 抓页面里的站内链接：只取以 / 开头的，跳过 # 锚点
    grep -o 'href="/[^"#]*"' "$TMP/page.html" | sed 's/href="//;s/"$//' >> "$TMP/links.txt"
  fi
done

sort -u "$TMP/links.txt" -o "$TMP/links.txt" 2>/dev/null || : > "$TMP/links.txt"
total="$(wc -l < "$TMP/links.txt" | tr -d ' ')"

note "== 站内链接（$total 个去重后的目标）=="
while read -r l; do
  [ -n "$l" ] || continue
  code="$(probe /dev/null "$BASE_URL$l")"
  # 302 也算通过：登录态跳转是预期行为
  if [ "$code" != "200" ] && [ "$code" != "302" ]; then
    note "  [FAIL $code] $l"
    fail=$((fail + 1))
  fi
done < "$TMP/links.txt"

if [ "$fail" -eq 0 ]; then
  note "全部通过：$(( ${#PAGES[@]} )) 个路由 + $total 个站内链接"
  exit 0
fi
note "不合格项：$fail"
exit 1
