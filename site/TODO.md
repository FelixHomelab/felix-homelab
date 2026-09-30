# Felix Homelab 社区站 — 实施计划

> 设计与取舍见 `DESIGN.md`，本文只讲**怎么做**与**怎么证明做完了**。
> 「完成」的判据是产物 + 可直接复制执行的验证命令，且命令要真跑过。

---

## 阶段 1：骨架（已完成）

**产物**：`cargo leptos` 一条命令同时产出 wasm 包与服务端二进制；Axum 承载 Leptos
SSR；SQLite 连接池挂进 `AppState`；`/healthz` 顺带探测数据库。

**已验证的命令**（本机实跑，输出见提交记录）：

```bash
export PATH="$HOME/.cargo/bin:$PATH"

cargo leptos build                 # 前端 wasm + 服务端 bin 均成功
cargo leptos serve                 # 监听 http://127.0.0.1:8080

curl -s http://127.0.0.1:8080/healthz
# 期望：{"sqlite":"3.46.0","status":"ok"}

curl -s http://127.0.0.1:8080/ | grep -o '<h1>[^<]*</h1>'
# 期望：<h1>Felix Homelab</h1>（证明是服务端渲染，不是空壳 HTML）
```

**环境前提**（本机非默认，换机器需照做）：Arch 包版 Rust 不含
`wasm32-unknown-unknown`，且容器内 `sudo` 被禁；须走用户级 rustup。完整三步见
`DESIGN.md` 第九节。

**踩过的坑**，避免重犯：

- 端口不能用 `3000`——**Forgejo 正占着 3000**，用了会 `Address already in use`。
  站点固定用 `8080`。
- axum 的 `FromRef` 派生宏需要 `features = ["macros"]`，默认不开。
- `sqlx::Error` 没实现 `IntoResponse`，不能直接当处理器错误类型，要显式映射。
- `.gitignore` 里绝不能有 `*.lock`——旧仓库正是被它误伤了 `Cargo.lock`。

---

## 阶段 2：内容层（已完成）

**产物**：`content/{posts,projects}/` 下的 Markdown 在进程启动时载入内存并建索引；
博客列表、正文、标签页、项目列表与详情可访问，全部服务端渲染。

**拆解**：

1. 引入 `gray_matter`（front matter）与 `pulldown-cmark`（Markdown → HTML）
2. `src/content.rs`：分三块——两端共有的数据结构、仅服务端的载入索引、以及作为
   前端唯一入口的 server function（这样 wasm 包里不会夹带整站内容）
3. 页面组件：`/blog`、`/blog/:slug`、`/blog/tag/:tag`、`/projects`、`/projects/:slug`
4. 示例内容：两篇文章 + 一个项目（**待站长替换成自己的内容**）

**已验证的命令**（本机实跑）：

```bash
cargo leptos build && cargo leptos serve

curl -s http://127.0.0.1:8080/blog | grep -oE 'class="card-title"' | wc -l   # 2
curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:8080/blog/nope     # 404
curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:8080/projects/nope # 404
curl -s 'http://127.0.0.1:8080/blog/tag/建站' | grep -oE 'class="card-title"' | wc -l  # 2
```

**草稿判据**（用一个 `draft: true` 的探针文章实跑后删除）：

| 检查 | 结果 |
|---|---|
| 不出现在 `/blog` 列表 | 未出现 |
| 不能用网址直达 `/blog/zz-draft-probe` | `404` |
| 不泄漏到标签页 | 未漏出 |
| 标签计数不含草稿 | `建站 · 2`（而非 3） |

**踩过的坑**：

- **SSR 模式必须显式设为 `SsrMode::Async`**。默认的 `OutOfOrder` 会把解析好的内容
  塞进 `<template>`，靠内联 JS 搬位——禁用 JS 的访客和爬虫只看到 fallback；而且
  `<head>` 先于数据发出，写在 `<Suspense>` 里的 `<Title>` 会整个丢失。已对全部
  数据驱动的路由加上 `ssr=SsrMode::Async`。
- **front matter 里不能出现未加引号的冒号**。`summary: 验证 draft: true 的行为`
  会让 YAML 解析失败并使**进程启动失败**——这是刻意的快速失败，报错会点名文件，
  但写内容时要知道给带冒号的值加引号。
- `Resource::map()` 给的是 `&T`（借用），要 `.get()` 才拿到拥有所有权的值，
  否则会撞上「cannot move out of ... behind a shared reference」。
- `#[server]` 属性宏由 `leptos_macro` 提供、经 `leptos::prelude::*` 带入；
  模块里漏了这句 `use` 就会出现「cannot find attribute `server` in this scope」。

**完成判据**：示例文章在 `draft: false` 时出现在列表、`draft: true` 时列表与直达
都取不到。✅

---

## 阶段 3：主题系统（未登录访客部分已完成）

**产物**：亮暗三档（auto / light / dark）与自定义主色、背景图；主题在**服务端**就
决定并写进 `<html>`，因此没有闪烁窗口。顶栏有三档切换器，`/me` 有完整的外观设置。

**已完成**：`src/theme.rs`（偏好解析与校验、CSS 变量生成、跨组件状态）、`<Html>`
上的 class 与 style 注入、三档切换器、`/me` 的配色与背景图表单。

**待阶段 4 接入**：账号级持久化（写 `user_prefs`）与背景图**上传**。目前背景图只
支持外链与站内相对路径，上传要等账号体系就绪后走 `POST /api/me/background`。

**已验证的命令**（本机实跑）：

```bash
cargo leptos build && cargo leptos serve
U=http://127.0.0.1:8080

# 主题在服务端就决定了，不存在闪烁窗口
curl -s $U/ | grep -oE '<html[^>]*>'                          # <html class="auto" ...>
curl -s -H 'Cookie: gf_theme=dark' $U/ | grep -oE '<html[^>]*>' # <html class="dark" ...>

# 自定义主色与背景图
curl -s -H 'Cookie: gf_theme=dark; gf_accent=8b85f5' $U/ | grep -oE '<html[^>]*>'
# <html class="dark" style="--accent:#8b85f5;--accent-contrast:#101014;" ...>

# 注入防御：以下必须全部被丢弃
curl -s -H 'Cookie: gf_accent=red' $U/ | grep -oE '<html[^>]*>'          # 无 style
curl -s -H 'Cookie: gf_bg=javascript%3Aalert%281%29' $U/ | grep -oE '<html[^>]*>'
curl -s -H 'Cookie: gf_bg=%2F%2Fevil%2Ecom%2Fx%2Ejpg' $U/ | grep -oE '<html[^>]*>'
```

**完成判据**：带不同主题 cookie 请求首页，返回的 HTML 中主题标记不同——即主题在
服务端就决定了。✅（含主色、背景图与注入防御）

**踩过的坑**：

- **web-sys 0.3 的 `Document` 没有 `cookie` / `set_cookie`**，这两个方法只挂在已废弃
  的 `HtmlDocument` 上；而 `document instanceof HTMLDocument` 在现代浏览器并不可靠。
  改用 `js_sys::Reflect` 按属性名读写，绕开接口转换。
- **Leptos 的 `class` 属性无论空否都会输出**，所以「不写 class」做不到；`auto` 模式
  改为输出有意义的 `class="auto"`。
- **前景色不能拍亮度阈值**：粗略加权公式给纯绿 `#00ff00` 算出中等亮度，会配出看不清
  的白字。改用 WCAG 相对亮度，分别算黑白两种对比度取高者。
- **`//host/x.jpg` 会绕过「站内相对路径」检查**——它以 `/` 开头，实为协议相对地址。
  校验时必须单独排掉。

---

## 阶段 4：账号与会话（已完成）

**产物**：完整初始 schema（五张表）、启动时自动跑迁移、注册/登录/登出、会话 cookie、
按环境变量初始化管理员、账号级外观偏好、背景图上传、改密码。密码用 argon2id，
令牌只存 SHA-256。

**已完成**：`migrations/0001_init.sql`、`src/db.rs`、`src/state.rs`、`src/auth.rs`、
`src/uploads.rs`、`/login` 与 `/register` 页面、顶栏账号区、`/user/:username`、
`/me` 的外观设置与改密码。

**账号与 cookie 的关系**（一句话）：**账号是真相来源，cookie 是它的缓存**。主题的
读取路径是同步的（服务端从请求头读 cookie），不能为了账号偏好引入一次异步查询，
所以登录时把账号偏好镜像进 cookie，之后每次修改两边都写。这样既保住「首屏不闪」，
又让偏好真正存在账号里。

**仍未做**（不影响阶段目标，留给后面顺手补）：编辑昵称与简介。这两个字段 schema 里
已经预留。

**站长账号怎么来**：不走开放注册，用一次性环境变量初始化：

```bash
ADMIN_USERNAME=felix ADMIN_PASSWORD='至少8位' cargo leptos serve
```

账号已存在时只补 `role = 'admin'`，**不动密码**——否则每次启动都会把改过的密码重置。

**已验证的命令**（本机实跑）：

```bash
cargo leptos build && cargo leptos serve
FN=/api/login10347140739296506902      # server function 的路径带哈希后缀，
RG=/api/register10347140739296506902   # 从 wasm 产物里 strings 出来即可

curl -s -X POST "http://127.0.0.1:8080$RG" \
  -H 'Content-Type: application/x-www-form-urlencoded' \
  --data 'username=testuser&password=test-password-123&display_name=测试用户'
# → {"Ok":null}，响应头带 Set-Cookie: gf_session=<64位十六进制>; HttpOnly; SameSite=Lax

sqlite3 data/site.db "SELECT username, substr(password_hash,1,12) FROM users;"
# → $argon2id$v=   绝无明文
sqlite3 data/site.db "SELECT length(token_hash) FROM sessions;"     # → 64

# 库里的 token_hash 必须等于 sha256(cookie 明文)，且不等于明文本身
printf '%s' "$TOKEN" | sha256sum | cut -d' ' -f1                     # 与库里一致

# 防用户名枚举：两种失败必须是同一句话
curl -s -X POST "$FN" --data 'username=testuser&password=wrong'   # 用户名或密码不对。
curl -s -X POST "$FN" --data 'username=nobody&password=wrong'     # 用户名或密码不对。
```

**完成判据**：数据库里既看不到明文密码，也看不到可用作登录的明文令牌。✅

| 检查项 | 实测结果 |
|---|---|
| 密码入库形态 | `$argon2id$v=19$m=19456,t=2,p=1$…` |
| 令牌入库形态 | `sha256(明文)`，64 位十六进制，与 cookie 明文不同 |
| 明文密码入库 | 全表检索无 |
| 用户名枚举 | 密码错与用户不存在返回同一句话 |
| 封禁即时性 | `status='banned'` 后旧会话立即失效（查询带 `u.status='active'`），无需删会话 |
| 会话过期 | `expires_at <= now` 的会话视为未登录 |
| 登出 | 库中会话行被删除 + cookie 以 `Max-Age=0` 清除 |
| 外键级联 | 删用户后其会话行随之消失（用 `PRAGMA foreign_keys=ON` 实测） |

**账号级偏好与背景图上传的验证**（同样实跑）：

```bash
SSR=/api/save_theme_prefs7717583749426046588   # 路径含哈希后缀，从 wasm 里 strings 出来

# 账号偏好入库；非法值必须被服务端丢弃（客户端送来的东西一律不可信）
curl -s -X POST "$SSR" -H "Cookie: gf_session=$TOKEN" \
  --data 'mode=dark&accent=ff0000&background=https%3A%2F%2Fexample.com%2Fbg.jpg'
sqlite3 data/site.db "SELECT theme_mode,accent,bg_kind,bg_value FROM user_prefs;"
# → dark|ff0000|url|https://example.com/bg.jpg

curl -s -X POST "$SSR" -H "Cookie: gf_session=$TOKEN" \
  --data 'mode=EVIL&accent=notahex&background=javascript%3Aalert%281%29'
# → auto|(null)|none|(null)   非法值全部被丢弃

# 登录把账号偏好镜像进 cookie（响应头里应有三条 gf_* cookie）
curl -s -D - -X POST "$LOGIN" --data 'username=felix&password=…' | grep -i set-cookie
# → gf_session=…; HttpOnly; SameSite=Lax      （会话 cookie 才带 HttpOnly）
# → gf_theme / gf_accent / gf_bg               （主题 cookie 不带，客户端水合要读）

# 背景图上传
curl -s -o /dev/null -w '%{http_code} %{redirect_url}\n' \
  -X POST http://127.0.0.1:8080/api/me/background -H "Cookie: gf_session=$TOKEN" -F 'file=@real.png'
# → 303 http://127.0.0.1:8080/me?e=ok
```

| 检查项 | 实测结果 |
|---|---|
| 合法值入库 | `dark / ff0000 / url / https://example.com/bg.jpg` |
| 非法值 | `mode=EVIL`→回落 `auto`；`accent=notahex`→NULL；`javascript:`→`none` |
| 登录镜像 | 响应带 `gf_theme` / `gf_accent` / `gf_bg` 三条 cookie |
| 首屏外观 | 带上这些 cookie 请求首页，`<html>` 直接是 `class="dark"` + 主色 + 背景图变量 |
| 上传成功 | 303 → `/me?e=ok`，库中 `bg_kind=upload`，磁盘出现随机文件名 |
| 提供图片 | 200，`content-type: image/png`、`x-content-type-options: nosniff`、长缓存 |
| **伪装文件** | 文本改名 `.png` 上传 → `e=type`（按魔数判断，不信 content-type） |
| **路径穿越** | `../../Cargo.toml`、`..%2f..%2f`、别的用户目录、`.svg`、`.php` → **全部 404** |
| 换图清理 | 重传后旧文件被删除，目录只留 1 个文件 |
| 体积上限 | 3 MB → `e=size` |
| 未登录上传 | → `e=auth` |
| 改密码 | 错旧密码 / 与原密码相同 / 过短都被拒；成功改后**其它会话从 2 条清到 1 条**，当前会话仍可用，旧密码失效 |
| 结果提示 | `/me?e=` 七种取值都有对应中文提示 |
| 竞态 | `/me` 连打 8 次，两个登录态区块**每次**都渲染 |
| 清空背景图 | 303 → `e=cleared`，库中 `none`，磁盘文件删除 |

**踩过的坑**：

- **`Resource::new` 与 `Resource::new_blocking` 的区别会咬人**。普通资源在服务端
  「读到就渲染、读不到给 `None`」，位置不对就是**竞态**——同一份代码有时渲染、
  有时不渲染（`/me` 连打 5 次出现 2 次有、3 次无）。`new_blocking` 的语义是
  「阻止 HTTP 响应发出直到数据就绪」。
  **⚠️ 这条结论后来被阶段 5 修正了**：`new_blocking` 单独用**不够**——资源还必须
  落在 `<Suspense>` 边界内，否则服务端照样不等它（评论区的资源就这样渲染成了
  「载入中…」）。准确的规则见阶段 5 的坑。
- **给路由加 `ssr=SsrMode::Async` 不能漏**。阶段 2 只给当时已有的路由加了，后来新加
  的 `/me`、`/login` 等漏掉，于是这些页面悄悄退回默认的流式模式，登录后才该出现的
  区块根本不渲染。**结论：新增路由时必须带上 `ssr=SsrMode::Async`，改完用
  `grep -c 'ssr=SsrMode::Async'` 与 `<Route ` 的数量对一下。**
- **`#[server]` 会按函数名生成同名类型**（`current_user` → `CurrentUser`）。自己定义的
  类型别名一旦同名，宏生成的 `impl` 就会落到别名指向的外部类型上，报一屏 E0117
  「only traits defined in the current crate…」。别名改名 `CurrentUserResource` 即可。
- **旧进程占着端口会让验证得出错误结论**。这轮新构建的服务因
  `Address already in use` 直接退出，而我没注意，测试全打在旧二进制上，一度得出
  「修复无效」的错判。**结论：跑验证前先 `ss -ltnp | grep 8080` 确认只有一个 pid
  在监听。**
- **`<Suspense>` 边界外的资源，服务端不会等**。`SsrMode::Async` 等的是全部
  Suspense 边界，而裸读 `.get()` 只会拿到 `None`。顶栏最初把账号资源直接裸读，
  首屏一直显示占位符「…」；包上 `<Suspense>` 后才正确。
- **`sqlite3` 命令行默认不开外键**（`PRAGMA foreign_keys` 默认 off），手工清理
  数据时级联删除不会触发，容易留下孤儿行；应用侧连接池已显式打开，两边行为不同。

**阶段 4 之后顺手要做的**：`/user/:username` 目前只显示用户名与昵称，
简介与头像等字段已在 schema 里预留，等有编辑入口再补。

---

## 阶段 5：互动（评论与光遇评价）（已完成）

**产物**：文章页可评论（支持回复与二级嵌套），`/sky/boosting` 可提交 1–5 星评价；
两者都**先审后显示**，并给提交者一个「正在等待审核」的提示。

**已完成**：`src/comments.rs`、`src/reviews.rs`、`src/components/comments.rs`、
`src/components/reviews.rs`、`/sky/boosting` 页面。

**几处刻意的设计**：

- 评论列表与「当前用户是否有待审评论」**合在一次请求里返回**（`CommentThread`）。
  分两个 server function 只会多一次往返。有这个提示，提交者刷新后才知道自己的内容
  在排队，而不是以为提交失败而反复重发。
- **父评论必须是同一目标下已批准的评论**。否则可以把回复挂到别的文章下面去。
- **父评论消失时，回复升为顶层显示**（`CommentView.parent_id` 指向的评论不在可见集合
  里）。将来后台把某条已批准评论改成拒绝时，它的回复不会凭空消失——作者会以为自己的
  回复被吞了。
- **评价正文是纯文本**，不走 Markdown：内容短、结构化，多一层渲染没有收益，反而多一个
  注入面。它以文本节点输出，由 Leptos 自动转义。

**已验证的命令**（本机实跑）：

```bash
# server function 路径含哈希后缀，从 wasm 产物里 strings 出来
SC=/api/submit_comment15623006871162035921
SR=/api/submit_sky_review1291691407219071325

# 未登录写不进去
curl -s -X POST "$SC" --data 'target_kind=post&target_slug=site-rewrite&body=未登录'
# → {"Err":"请先登录再评论。"}

# 登录后落 pending
sqlite3 data/site.db "SELECT id,status,body_html FROM comments;"
# → 1|pending|<p>这是 alice 的第一条评论</p>
```

| 检查项 | 实测结果 |
|---|---|
| 待审评论可见性 | 匿名访客、作者本人、其他用户**三方都看不到** |
| 待审提示 | 只有作者看得到「你有评论正在等待审核。」 |
| 审核后 | 出现在页面上，作者链接指向 `/user/<username>` |
| **XSS：裸 HTML** | `<script>alert(1)</script>` 标签被剥离只剩文本；`<img onerror=…>` 整个消失；`<script>alert`、`onerror=`、`javascript:` 残留均为 **0** |
| Markdown 保留 | `**粗体**` → `<strong>`，链接正常 |
| 回复校验 | 不存在的父评论、**跨目标**的父评论都被拒 |
| 二级嵌套 | 回复渲染在 `comment-replies` 里 |
| **父评论被拒后** | 回复**没有消失**，升为顶层（`comment-replies` 归 0，内容仍在） |
| 评价评分校验 | `rating=0` 与 `rating=6` 都被拒；未登录、空正文也被拒 |
| 评价纯文本 | `<b>` 渲染为 `&lt;b&gt;`，页面上没有真的 `<b>` 标签 |
| 评分概览 | 5 星 + 3 星 → 均分 `4.0`、`共 2 条评价` |
| 星标 | `★★★★★` 与 `★★★☆☆` 均正确 |
| 站长回复 | 渲染为独立的「站长回复：」区块 |
| 确定性 | 文章页与代跑页连打 **6 次**，评论/评价每次都在，无「载入中」残留 |
| 全路由回归 | 16 条全部符合预期（14×200、2×404） |

**踩过的坑**：

- **`new_blocking` 单独用不够，资源还必须包在 `<Suspense>` 里**。这是对阶段 4 那条
  结论的**修正**：评论区的资源用了 `new_blocking` 但没包 Suspense，服务端直接不等它，
  页面渲染成「载入中…」（`__PENDING_RESOURCES` 里能看到它挂着）。包上 `<Suspense>`
  后立刻正常。**准确的规则是：凡服务端渲染就要有数据的资源，必须落在 `<Suspense>`
  边界内；`new_blocking` 解决的是另一件事（顶层数据要在响应发出前就绪）。**
- **`grep` 会被 Leptos 插的 `<!>` 标记骗到**。星标 `★★★☆☆` 一度看起来丢了 `☆`，
  实际是 `class="stars">★★★<!>☆☆`，我的 `[^<]*` 在 `<!>` 处截断了。**结论：验证
  渲染结果时先 `sed 's/<!>//g'` 再匹配，否则会追一个不存在的 bug。**
- 顺带纠正一个操作失误：验证时 `curl -o boost.html` 落在了项目目录里，成了未跟踪文件。
  **结论：临时产物一律写到 `/tmp`，别写在仓库里。**

**完成判据**：未登录写不进去；新提交在审核前不出现在页面上。✅

---

## 阶段 6：后台（已完成）

**产物**：`/admin`（概览）、`/admin/comments`、`/admin/sky-reviews`、`/admin/users`。
可审核评论（通过 / 拒绝 / 删除）、管理用户（封禁 / 解封 / 改角色）、回复评价。
**不做**文章增删改：内容走 Markdown 提交（`DESIGN.md` 第五节）。

**已完成**：`src/admin.rs`、`src/pages/admin.rs`、四条路由、顶栏的后台入口。

**三条刻意的设计**：

- **授权在每个 server function 里，不在页面上。** 页面判身份只决定显示什么，
  给出一个像样的 403；接口是公开可达的，把按钮藏起来挡不住直接构造请求。
- **状态与角色取值走白名单**。这些是用户输入，虽然库里有 `CHECK` 约束兜底，
  但先在这里挡掉更干净，也不必让无效请求打一次数据库。
- **管理员不能改自己的状态或角色**（后端强制，前端也把按钮置灰）。否则一次误操作
  就能让唯一的站长账号变成被封禁的普通用户，后台再也进不去。
- 删除评论时界面上会写明「连同 N 条回复」：外键是 `ON DELETE CASCADE`，回复会一起没。

**已验证的命令**（本机实跑）：

```bash
H=1029245475650073899    # 后台 server function 的统一哈希后缀

# 授权边界：三方对比
curl -s -X POST "$U/api/admin_list_comments$H" --data 'status=all'                    # 请先登录。
curl -s -X POST "$U/api/admin_list_comments$H" -H "Cookie: gf_session=$BOB" --data …  # 需要管理员权限。
curl -s -X POST "$U/api/admin_list_comments$H" -H "Cookie: gf_session=$ADM" --data …  # 数据
```

| 检查项 | 实测结果 |
|---|---|
| 页面级授权 | 匿名与普通用户访问四个后台页**全部 403**；管理员 200 |
| 确定性 | 管理员连打 8 次四个页面**全 200**；匿名与普通用户各 3 次**全 403** |
| 读接口授权 | 匿名「请先登录。」／普通用户「需要管理员权限。」／管理员拿到数据 |
| 写接口授权 | 普通用户调用通过评论、封禁他人 **都被拒** |
| 值白名单 | `status=EVIL`、`role=superuser` 都被拒 |
| 自我保护 | 管理员封禁自己、改自己角色**都被拒**，账号仍是 `admin/active` |
| 审核通过 | 评论通过后前台立刻可见；回复通过后出现嵌套 |
| 评价流程 | 通过后出现在代跑页；站长回复渲染为独立区块；改为拒绝后立刻消失 |
| 删除级联 | 删除带回复的评论：2 条 → 0 条，回复一并消失，前台不再有嵌套 |
| 封禁即时性 | 封禁后旧会话立刻被当作未登录（顶栏回到「登录/注册」），登录被拒；管理员自身不受影响 |
| 解封恢复 | 解封后可以重新登录 |
| 概览统计 | 页面上的四个数字与库中 `COUNT` 完全一致 |
| 全路由回归 | 15 条公开路由 + 4 条后台路由全部符合预期 |

**踩过的坑**：

- **App 级资源在路由子树里读会竞态**。后台的守卫最初写成
  `if !current_user_is_admin(&user_state) { return forbidden(); }`，用的是 App 里那个
  「当前用户」资源——结果**管理员自己也被 403**；换成响应式闭包后变成**时好时坏**
  （连打 5 次里 2 次 403）。根因是那个资源建在**路由子树之外**，路由页面的
  `<Suspense>` 等不稳它。改成**页面内部自己的资源**（`am_i_admin`）后，连打 8 次全对。
  **结论：页面级判断要用页面自己拉的数据，别去读 App 级的资源。**
- **组件体在资源解析前就执行了**。所以 `if ... return forbidden()` 这种写法天生不可靠；
  判断必须放在渲染期的响应式闭包里。
- **只在 `#[server]` 函数体里用的常量会被报「从未使用」**，同时白白编进 wasm——因为
  hydrate 构建会把这些函数体整块去掉。加 `#[cfg(feature = "ssr")]` 门禁即可。
- 又一次被 `<!>` 标记骗到：`grep '登录</a>'` 匹配不到，实际是 `登录<!></a>`。

**完成判据**：普通用户无论如何构造请求都进不了后台。✅

---

## 阶段 7：SEO（已完成）

**产物**：`/rss.xml`、`/sitemap.xml`、`/robots.txt`，以及各页的 `meta description`
与文章/项目的 Open Graph 标签。

**已完成**：`src/seo.rs` + 三条 Axum 路由；`pages/mod.rs` 各页的 `<Meta>`。

**两条刻意的设计**：

- **三个端点由 Axum 直接提供，不做成 Leptos 页面**：产出的是 XML 与纯文本，过一遍
  模板层没有收益，反而容易把 HTML 的转义规则带进来。
- **`SITE_URL` 是部署必填项**。RSS 与 sitemap 都要求绝对 URL，没有它只能退回
  `http://127.0.0.1:8080`——本机看着没问题，上线就全是错地址。见 `DESIGN.md` 第十节。

**已验证的命令**（本机实跑）：

```bash
# 产出必须是合法 XML——标题里一个 & 就能让整份 feed 失效
curl -s http://127.0.0.1:8080/rss.xml | python3 -c "import sys,xml.etree.ElementTree as ET; ET.parse(sys.stdin); print('ok')"
curl -s http://127.0.0.1:8080/sitemap.xml | python3 -c "import sys,xml.etree.ElementTree as ET; ET.parse(sys.stdin); print('ok')"

# SITE_URL 必须生效
SITE_URL=https://example.com cargo leptos serve
curl -s http://127.0.0.1:8080/robots.txt | grep Sitemap   # → Sitemap: https://example.com/sitemap.xml
```

| 检查项 | 实测结果 |
|---|---|
| Content-Type | `application/rss+xml`、`application/xml`、`text/plain`，均带 `charset=utf-8` |
| XML 合法性 | RSS 与 sitemap 都能被 XML 解析器解析 |
| RSS 结构 | `version="2.0"`、`atom:link rel=self`、`guid isPermaLink="true"`、`content:encoded` 带全文 |
| pubDate 星期 | `2026-09-20` → `Sun`、`2026-09-19` → `Sat`，与 Python 的 `%A` 一致 |
| **XML 转义** | 标题 `转义测试 & <标签> "引号"` → `<title>转义测试 &amp; &lt;标签&gt; &quot;引号&quot;</title>`，XML 仍合法 |
| sitemap 内容 | 6 个固定页 + 每篇文章（带 `lastmod`）+ 每个项目 + 每个标签（含中文标签） |
| robots.txt | 放行全站，禁 `/admin`、`/me`、`/login`、`/register`、`/api/`、`/uploads/`，附 Sitemap 行 |
| meta description | 8 个主要页面都有，文章与项目页用各自摘要，摘要为空时回落到站点说明 |
| Open Graph | 文章页有 `og:type=article`、`og:title`、`og:description` |
| `SITE_URL` | 设为 `https://example.com` 后，sitemap 与 robots 里的地址全部随之改变 |

**踩过的坑**：

- 没有新坑，但这个阶段最容易出事的地方是**转义**：内容由站长自己写，一个 `&` 就足以
  让订阅器报错，所以 `escape_xml` 不是可选加固。我专门造了一篇标题含
  `&` `<` `"` 的文章验证，XML 仍能解析。
- `pubDate` 的星期是用 `chrono` 算的，与 Python 的 `%A` 交叉核对过——这种「看着对」
  的字段最容易悄悄错。

---

## 收尾：补齐内容模型与「联系」页（已完成）

做这一轮的起因是**对照目标逐项检查时发现两个真空缺**：目标写的是「About+联系」，
但 `/contact` 路由根本不存在；设计文档第三节列的 `content/sky/` 与 `content/pages/`
两个集合也从未实现（`store::load` 只载入 `posts` 与 `projects`）。

**已完成**：`content/pages/`（静态页）与 `content/sky/`（光遇攻略与画廊）两个集合、
`/contact` 页、`/sky/gameplay` 与 `/sky/gallery` 两个分类页、光遇首页入口、
顶栏「联系」导航、sitemap 收录。

**三条刻意的兜底**（都实测过）：

- **静态页文件缺失时给提示而不是 404**。「关于」这类固定入口不该因为少一个文件就整页
  打不开，提示里还直接写了该往哪放文件。
- **光遇内容分类写错时归到 `gameplay` 并告警**，而不是从两个列表里都消失。
- **URL 里的分类与白名单比对**，不在名单里直接 404——「这里还没有内容」和「没有这个
  页面」是两回事。

**已验证的命令**（本机实跑）：

```bash
for p in /contact /sky/gameplay /sky/gallery; do
  curl -s -o /dev/null -w "$p %{http_code}\n" "http://127.0.0.1:8080$p"
done
curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:8080/sky/whatever            # 404
curl -s -o /dev/null -w '%{http_code}\n' 'http://127.0.0.1:8080/sky/../../etc/passwd'  # 404
```

| 检查项 | 实测结果 |
|---|---|
| 新增路由 | `/contact`、`/sky/gameplay`、`/sky/gallery` 均 200 |
| `/sky/boosting` 未被吞 | 仍是代跑页（`:category` 路由没有抢走它） |
| 非法分类 | `/sky/whatever` 与 `/sky/../../etc/passwd` 都是 **404** |
| 静态页来自文件 | `/contact` 渲染出 `content/pages/contact.md` 的三个小节与「最后更新于 2026-09-20」 |
| 文件缺失兜底 | 移走 `about.md` 后 `/about` 仍是 **200**，提示「在 content/pages/about.md 里添加内容即可」 |
| 分类写错兜底 | 分类写成 `typo-here` 的条目出现在攻略页，服务端同时告警 |
| 光遇首页 | 攻略 / 画廊 / 代跑三个入口都在 |
| 顶栏导航 | 博客 / 项目 / 光遇 / 关于 / 联系 |
| sitemap | 14 条，含 `/contact` 与两个分类页 |
| RSS | 仍合法，2 条 |
| 全路由回归 | 22 条全部符合预期 |
| 确定性 | 三个新页面各连打 5 次全 200 |

**踩过的坑**：

- **注释写了行为、代码却没实现**。我在 `load_sky` 里写「分类写错会归到 gameplay，
  而不是从列表里消失」，但代码只在**字段缺失**时兜底，值非法时原样保留——结果那条
  内容两个列表都匹配不上，**静默消失**。是专门写的兜底测试把它抓出来的。
  **教训：注释描述的兜底行为必须被测试覆盖，否则注释只是自我安慰。**
- 又一次是「假设的代码文本与文件实际内容不符」：脚本锚点按记忆写，而 `AboutPage` 在
  阶段 7 已被加过 `<Meta>` 行，断言直接失败。这类失败倒无害（脚本没写入），但提醒我
  改文件前先读取。

---

## 阶段 8：部署（待定）

部署形态未确认，方案见 `DESIGN.md` 第十节。定稿前不动。

---

## 数据库 DDL

阶段 2 之前不进任何内容数据；下表是互动数据的初始 schema，落在
`migrations/0001_init.sql`。

| 表 | 关键列 | 约束与索引 |
|---|---|---|
| `users` | `id`、`username`、`password_hash`、`display_name`、`email`、`bio`、`role`、`status`、`created_at` | `username` 唯一；`role` ∈ (admin, user)；`status` ∈ (active, banned) |
| `sessions` | `id`、`user_id`、`token_hash`、`expires_at`、`created_at` | `token_hash` 唯一；`user_id` 外键级联删除 |
| `comments` | `id`、`target_kind`、`target_slug`、`user_id`、`parent_id`、`body_md`、`body_html`、`status`、`created_at` | `(target_kind, target_slug, status)` 联合索引 |
| `sky_reviews` | `id`、`user_id`、`rating`、`body`、`reply`、`status`、`created_at` | `rating` ∈ 1..5 |
| `user_prefs` | `user_id`、`theme_mode`、`accent`、`bg_kind`、`bg_value`、`updated_at` | `user_id` 主键兼外键 |

`email` 允许为空：本站不发邮件，收邮箱没有意义（`DESIGN.md` 第五节）。

---

## 收尾二：编辑昵称与简介（已完成）

**已完成**：`auth::update_profile`、`/me` 的「昵称与简介」表单、公开资料页显示简介、
`UserView` 增加 `bio` 字段。

**顺带修掉一个更严重的既有缺陷**：`prop:value` **只设 JS 属性，不写进服务端渲染的
HTML**。用它的输入框在首屏全是空的：

| 位置 | 后果 |
|---|---|
| 昵称 / 简介 | 用户看不到自己的资料，可能误以为没设过 |
| 主色 / 背景图 | 明明设过，打开设置页看到的是空框 |
| **后台的回复框** | 最严重：管理员会以为原本没回复过，一保存就**覆盖掉旧回复** |
| 评论 / 评价框 | 这些本来就该是空的，无影响 |

改法是把初值直接写进 HTML：`<input value=初值>`、`<textarea>初值</textarea>`，且
**不绑成响应式**——否则打字时会有东西跟光标抢。保存成功后组件因 `refetch` 重建，
初值随之更新。

**已验证的命令**（本机实跑）：

```bash
# 服务端渲染的 HTML 里必须看得到已有值
curl -s -H "Cookie: gf_session=$ADM" http://127.0.0.1:8080/me \
  | grep -oE '<input type="text" value="[^"]*"'          # value="Felix Homelab"
curl -s -H "Cookie: gf_session=$ADM" http://127.0.0.1:8080/admin/sky-reviews \
  | grep -oE '<textarea[^>]*>[^<]{0,40}'                 # 含已有的站长回复
```

| 检查项 | 实测结果 |
|---|---|
| 表单可见性 | 登录后有「昵称与简介」区块，未登录没有 |
| 更新生效 | 改名后**顶栏立刻显示新昵称**（`refetch` 而非整页刷新），资料页显示简介 |
| 校验 | 空昵称、昵称 >30 字、简介 >300 字都被拒，且提示写明上限 |
| 未登录更新 | 被拒 |
| 清空简介 | 库里存 `NULL`（不是空串），资料页不渲染空段落 |
| 表单回填 | 昵称、简介、主色、背景图、后台回复**五处**都在 SSR 里带上已有值 |
| 全路由回归 | 22 条全部符合预期 |

**踩过的坑**：

- **`prop:value` 与 `value` 的区别是 SSR 与客户端的分界线**。前者只在浏览器端设 DOM
  属性，服务端 HTML 里什么都没有。凡是「要显示已有值」的输入框都必须用 `value`
  或把初值写成子节点，并且**不能绑成响应式**（会跟光标抢）。
  这个缺陷从阶段 3 就存在，直到这一轮做资料表单时才被发现——教训是：**验证要看
  服务端渲染出来的原始 HTML，不能只看浏览器里hydrate之后的样子**。

---

## 阶段 9：Nix 化与 NixOS 部署（已完成）

开发环境是 Docker 上的 NixOS 容器、应用环境是云服务器上的 NixOS，因此工具链与部署
都不该是「照着文档装一遍 / 一堆 shell 脚本」，而是声明式配置。照 `~/项目/AWCC`
既有的结构来：`flake.nix` + `nix/package.nix` + `nix/module.nix`。

**已完成**：`flake.nix`（devShell + packages + nixosModules）、`nix/package.nix`、
`nix/module.nix`、`flake.lock`；`.gitignore` 补 `result`。

**为什么需要 rust-overlay**：nixpkgs 自带的 rustc 不带 `wasm32-unknown-unknown` 的
std，而且**没有** `pkgsCross.wasm32-unknown-unknown`（实测确认），而 Leptos 的前端
产物必需它。工具链只在 `mkToolchain` 定义一次，devShell 与打包共用。

**为什么 wasm-bindgen 锁精确版本**：cargo-leptos 从 `Cargo.lock` 探测
`wasm-bindgen` 版本，再找**同版本**的 CLI；不一致时它要联网自己下载，在构建沙箱里
必然失败。所以把 `wasm-bindgen` / `js-sys` / `web-sys` 一起锁到 nixpkgs 提供的
0.2.127 / 0.3.104，并在 Cargo.toml 注释里写明这条耦合。

**验证环境**：本机是 Arch 容器且无 nix，用 `nixos/nix` 镜像起了一个**持久容器**
（`gf-nix`，项目挂载在 `/work`），Nix 2.35.2。容器内 root 生成的文件最后
`chown 1000:1000` 归还，项目目录里不留 root 属主文件。

**已验证的命令**（本机实跑）：

```bash
# 求值层面
docker exec -w /work gf-nix nix build .#default          # 真构建，见下表
# NixOS 模块在测试系统上求值
docker exec -w /work gf-nix nix eval --impure --expr '…nixosSystem…'

# 端到端：把 store 里的产物真跑起来
OUT=$(readlink -f /work/result)
LEPTOS_SITE_ROOT=$OUT/share/felix-homelab-site/site \
DATABASE_URL=sqlite:///tmp/nixrun/site.db \
CONTENT_DIR=$OUT/share/felix-homelab-site/content \
SITE_URL=https://example.com \
$OUT/bin/felix-homelab-site
```

| 检查项 | 实测结果 |
|---|---|
| `nix build .#default` | **成功**，产物 `/nix/store/h4xlgzv0…-felix-homelab-site-0.1.0` |
| 产物内容 | `bin/` 二进制 33MB、`site/pkg/` 前端三件套、`content/` 七个 Markdown |
| wasm 体积 | **1.43MB**（wasm-opt 生效；debug 版是 4.5MB） |
| 模块求值 | `ExecStart`、`SITE_URL`、`LEPTOS_SITE_ROOT`、`COOKIE_SECURE=1`、`ReadWritePaths`、nginx vhost、备份定时器全部正确 |
| devShell 求值 | 可求值 |
| **Nix 产物真的能跑** | `/healthz` → `{"sqlite":"3.46.0","status":"ok"}` |
| 路由 | 15 条全部 200（含 `/rss.xml`、`/sitemap.xml`、`/sky/gameplay`） |
| `SITE_URL` 生效 | `robots.txt` 里是 `Sitemap: https://example.com/sitemap.xml`，sitemap 里也是该域名 |
| 静态资源 | wasm 1432402B、js 23887B、css 13350B，均 200 |
| 服务端渲染 | 文章列表出现在 HTML 里 |
| 数据落位 | `site.db`（含 WAL/-shm）与 `uploads/` 都落在指定的 `DATABASE_URL` / `UPLOAD_DIR` 下 |

**踩过的坑**：

- **release 构建会因为递归深度直接失败**——这是整轮最有价值的发现。
  `error: queries overflow the depth limit!`，编译器提示加
  `#![recursion_limit = "256"]`。**debug 构建不触发**，而我此前所有验证都只跑
  `cargo leptos build`（debug）。也就是说：**我一直在用不会出问题的那种构建方式验证，
  而部署用的是另一种**。修好后本地 `--release` 也复测通过。
- **release 构建还需要 `wasm-opt`**：cargo-leptos 报
  `wasm-opt is required but was not found`，得把 `binaryen` 加进构建输入。
- **忘了把 `binaryen` 加进 `package.nix` 的函数参数列表**，报
  `undefined variable 'binaryen'`——Nix 的报错位置很准，一次就修好了。
- **Nix 只看得见 git 跟踪的文件**：新增的 `flake.nix` / `nix/` 未跟踪时，报
  「Path 'flake.nix' in the repository is not tracked by Git」，用 `git add -N`
  解决（顺带让它按 `.gitignore` 过滤，不会把几 G 的 `target/` 塞进 store）。
- **容器里 git 的所有权检查**：宿主 uid 1000 的文件被 root 的容器读写时，libgit2
  报「not owned by current user」，加 `safe.directory` 解决。这是容器场景特有的，
  真实开发环境（同为 NixOS、同一用户）不会遇到。
- **`nixosModules` 里不能用求值时的系统**：我最初写了
  `self.packages.${nixpkgs.lib.currentSystem or "x86_64-linux"}`，既用了不存在的
  `currentSystem`，逻辑也错——模块应在**目标系统**的 `pkgs.stdenv.hostPlatform.system`
  上取默认包。改成把 `self` 传给模块。

---

## 阶段 10：开发容器（已完成）

用户指出「本网站在开发阶段运行在 NixOS 容器里，属于 localhost 那种，可以浏览器访问」
——而我之前起的验证容器**没发布端口**，里面跑起来的东西在浏览器里根本看不到。补上
`compose.yaml`：`docker compose up -d` 之后浏览器打开 `http://127.0.0.1:8080`。

**四处关键点**（都是实测撞出来的）：

- **必须覆盖 `LEPTOS_SITE_ADDR`**。`Cargo.toml` 里 `site-addr` 是 `127.0.0.1:8080`，
  容器里绑回环的话**发布出去的端口到不了**——现象就是浏览器连不上而容器日志一切正常。
  cargo-leptos 认这个环境变量（实测：设成 `0.0.0.0:8099` 后确实绑到了 `0.0.0.0`），
  所以 compose 里覆盖成 `0.0.0.0:8080`，本机裸跑仍保持回环。
- **`NIX_CONFIG` 打开 flakes**。全新容器里 `nix-command`/`flakes` 是关闭的，
  `nix develop` 直接报 `experimental Nix feature 'nix-command' is disabled`。用
  `NIX_CONFIG` 而不是改镜像里的 `/etc/nix/nix.conf`——配置随 compose 文件走。
- **`nix store` 与 `target/` 放命名卷**。命名卷首次挂载会被镜像里的 `/nix` 播种
  （实测：空卷挂上去后 store 里有 125 项、nix 版本正常），之后累积；没有它每次 `up`
  都要重下几 GB。`target/` 放卷里还有个额外好处：容器里 cargo 以 root 运行，写进
  宿主机目录的话会留下 root 属主的文件，你之后改不动。
- **`safe.directory`**：容器里 nix 必须以 root 跑（store 要写权限），而 `/work` 是
  宿主 uid 1000 的文件，libgit2 因所有权不符拒绝解析 flake。真实 NixOS 开发机上
  不会遇到。

**已验证**（从宿主机发起，即浏览器会看到的路径）：

```bash
docker compose up -d
curl -s http://127.0.0.1:8080/healthz          # → {"sqlite":"3.46.0","status":"ok"}
for p in / /blog /projects /about /contact /sky /sky/gameplay /sky/gallery \
         /sky/boosting /login /me /rss.xml /sitemap.xml /robots.txt; do
  curl -s -o /dev/null -w "$p %{http_code}\n" "http://127.0.0.1:8080$p"
done
```

| 检查项 | 实测结果 |
|---|---|
| 容器起来 | `felix-homelab-site-dev-1`，端口映射 `127.0.0.1:8080->8080/tcp` |
| 容器内日志 | `Serving at http://0.0.0.0:8080`、数据库就绪、内容已载入 |
| **宿主机访问** | 15 条路由全部 **200** |
| 健康检查 | `{"sqlite":"3.46.0","status":"ok"}` |
| 首页服务端渲染 | `<title>Felix Homelab</title>` 与 `<h1 class="hero-title">` |

**顺带清理**：删掉了我走弯路时起的 `gf-nix` 容器（17.9G）与 `nix-portable` 二进制，
Docker 容器占用从 17.89GB 降到 272MB。

**数据持久化（已实测）**：`/work/data` 挂在命名卷上，SQLite 与上传图片都在里面。
验证方式是**真的销毁重建容器**：

| 数据 | 重建容器后 |
|---|---|
| 用户 `persist-test` | 还在，原密码可登录 |
| 上传的背景图 | 还能访问（HTTP 200、`image/png`） |
| 账号级主题偏好 | 还在 |

注意 `docker compose down -v` 会连卷一起删——那是真的清库，不是「重启一下」。

**顺带修掉一个会让服务起不来的 bug**：`ensure_admin` 原来把**空字符串**当成有效值，
而容器里惯例写法是 `${ADMIN_USERNAME:-}`，没填时正是空串——于是走
`validate_credentials("")` 失败并 `bail!`，表现成「**没配管理员反而服务启动失败**」，
很难往这个方向想。现在空串按「没设置」处理，两种情形都实测过：

- 不设：服务正常启动（日志直接到 `listening on`）
- 设了（写进 `.env`）：日志出现 `已创建管理员账号：felix`，库里 `felix|admin`，可登录

---

## 阶段 11：发布者署名与项目内容补齐（已完成）

**要求**：内容由发布者发布，本站发布者是 Felix；将来别人也能发布，但现在不开源，暂不需要。
**顺带**：按 Forgejo 上的仓库把项目板块的内容补齐。

### 改了什么

| 处 | 内容 |
|---|---|
| `src/content.rs` | 三类 front matter 新增 `author`；`DEFAULT_AUTHOR = "Felix"` 兜底；`resolve_authors` 按去重用户名集合做**一次**批量查询，不每篇查库 |
| `src/pages/mod.rs` | 新增 `AuthorLine`：解析得到显示名就链 `/user/:username`，解析不到只显示用户名、不给链接 |
| `src/seo.rs` | RSS 条目加 `<dc:creator>`，channel 声明 `xmlns:dc` |
| `style/main.css` | `.author` 样式 |

取值规则与边界见 `DESIGN.md` 第三节「发布者：`author`」。

### 内容

- 已有内容（2 篇文章、2 条光遇、本站项目条目）都显式写上 `author: Felix`。
- **项目板块按 Forgejo 上的 8 个仓库补齐**：`Nebula`、`Qaw-Language`、`tianshu-cosmic-hub`、
  `AWCC-G16-7630-Linux`、`dev-rules`、`felix-dev-rules`、`forgejo-deploy`，加上本站。
  `summary` 取各仓库自己的定位语，正文只复述各自 README 里的事实，没有自己编。
  `AWCC-upstream` **没有收录**——它自己的描述就写着「只读镜像，不在此提交」，不是作品。
- 私有仓库一律**不填 `repo`**：Forgejo 只绑回环，链进去对访客就是死链。

### 验证

```bash
docker compose exec dev nix develop -c cargo leptos build   # 必须 exit 0
docker compose restart dev                                  # 内容在启动时载入，必须重启
./scripts/check-links.sh                                    # 验收命令
```

实测输出：

```
== 路由可达性（http://127.0.0.1:8080）==
== 站内链接（27 个去重后的目标）==
全部通过：16 个路由 + 27 个站内链接        # 退出码 0
```

另外单独核过：`/rss.xml` 里 `<dc:creator>Felix</dc:creator>` 出现两次（两篇文章）；
项目列表顺序与 `weight` 一致（nebula → qaw-language → tianshu-cosmic-hub →
awcc-g16-7630-linux → dev-rules → felix-dev-rules → forgejo-deploy → 本站）；
`release` 构建 exit 0。

### 已知问题（未处理，等决策）

1. **用户名不区分大小写这层没有约束**：`users.username` 是大小写敏感的 `UNIQUE`，
   登录与查号都是 `WHERE username = ?1` 精确匹配，所以 `Felix` 与 `felix` 可以是
   **两个不同账号**。作者署名现在绑的是用户名，这就有冒充空间。
   开发库里目前正躺着这两个测试账号（`Felix` 是注册流程测试留下的普通账号，
   `felix` 是管理员测试留下的），**都是我的测试产物，不是你要的账号**。
2. 项目条目只在**仓库可公开访问**时才该填 `repo`。`Nebula` 的 README 里写着从 Gitee
   安装，说明它可能已有公开镜像——要不要给这一个加上链接，等你确认。

---

## 阶段 12：一个人一个账号（已完成）

**要求**（用户原话）：账号合并，每个人只有一个账号；大小写不敏感；不限制昵称字符类型和长度。

### 改了什么

| 处 | 内容 |
|---|---|
| `migrations/0002_username_nocase.sql` | **先合并已有重名账号**，再建 `username COLLATE NOCASE` 唯一索引 |
| `src/auth.rs` | 登录、注册查重、资料页、`ensure_admin` 四处查号全加 `COLLATE NOCASE`；删掉昵称长度上限（`DISPLAY_NAME_MAX`） |
| `src/content.rs` | 署名解析改大小写不敏感，返回值从「显示名」换成 `AuthorAccount`（库里的用户名 + 显示名），链接以库里的写法为准 |
| `src/pages/mod.rs` | `AuthorLine` 改用 `AuthorAccount`；资料页文案不再提「最多 30 个字符」 |

### 合并规则（确定性，不靠人工挑）

留 **id 最小**的那个（最早注册的），用户名写法用它的；会话、评论、评价改挂过去；
外观偏好保留者已有就用保留者的，否则搬过来；**角色取组里更强的**（组里有人是管理员，
保留者就是管理员）；**状态取封禁**（否则重名账号能绕开封禁）；密码哈希只能留保留者的
——哈希不可合并，被合并方的原密码从此不能登录。

### 验证

```bash
docker compose exec dev nix develop -c cargo leptos build   # exit 0
# 新增迁移后必须重编才会被 sqlx::migrate! 嵌进去
touch src/db.rs && docker compose exec dev nix develop -c cargo leptos build
docker compose restart dev                                  # 启动时跑迁移
./scripts/check-migration.sh                                # 迁移回归，21 条断言
./scripts/check-links.sh                                    # 站点验收
```

实测输出：

| 验的东西 | 结果 |
|---|---|
| `scripts/check-migration.sh` | 21 条断言**全部通过**（合并后账号数、保留写法、管理员提升、封禁提升、引用零孤儿、大小写变体被拒、新用户名仍可注册） |
| `scripts/check-links.sh` | 全部通过：16 个路由 + 27 个站内链接 |
| 真实开发库跑完迁移 | `_sqlx_migrations` 里有 `2\|username nocase\|1`；账号只剩 `Felix`(id 3) 且 `role` 升为 `admin`；1 条评论、3 个会话、1 条偏好仍挂在 id 3 上；索引已建 |
| `/user/Felix`、`/user/felix`、`/user/FELIX` | 都是 200，标题同为 `Felix — Felix Homelab` |
| 在库副本上起第二个实例跑注册/登录 | 注册 `CaseTest` → Ok；注册 `casetest` → 「这个用户名已经被用了。」；登录 `CASETEST` → Ok；登录 `CaSeTeSt` → Ok；密码错 → 「用户名或密码不对。」 |

### 两个踩到的坑

1. **新增迁移文件不会触发重编**：`sqlx::migrate!` 是编译期展开的，加文件后要
   `touch src/db.rs` 才会重新展开，否则二进制里根本没有新迁移、启动时静默不跑。
2. **不能用 `cp` 看 WAL 模式下的库**：`docker compose cp` 只拷了 `site.db`，提交过的
   数据还在 `-wal` 里，读出来是几小时前的状态。我第一次就是据此误判「迁移没生效」。
   三个文件一起拷才对，备份请用 `sqlite3 .backup`。两条都写进了 DESIGN 第九节。

### 账号清理（已按你的确认执行）

合并后留下的那个账号（`Felix`）密码是我测试时设的，你无从得知；而 `ensure_admin`
对已存在的账号只补角色、不改密码，所以在 `.env` 里写 `ADMIN_PASSWORD` 也拿不回控制权。
经你确认后，我把开发库清空了：

```bash
docker compose stop dev                      # 先停，别在服务跑着时改库
# 三个文件一起取出（WAL！），checkpoint 后 DELETE，再写回
docker compose up -d --force-recreate dev    # 必须重建，不是 restart
```

清理结果：用户 / 会话 / 评论 / 评价 / 偏好**全部为 0**，两条迁移记录保留，
启动日志里没有「已创建管理员账号」。建你自己的管理员：

```bash
echo 'ADMIN_USERNAME=Felix' >> .env
echo 'ADMIN_PASSWORD=你自己的密码' >> .env
docker compose up -d --force-recreate       # restart 不重新读 .env
```

**第三次踩坑（已写进 DESIGN 第九节）**：`docker compose start` / `restart` 不重新读
`.env`，容器里的环境变量是**创建时**定下的。我第一次删完账号只做了 `restart`，
容器里还留着旧的管理员变量，启动时把账号又建了回来——看起来像删除失败。

### 账号怎么拿回来（已定，等执行）

开发库现在是空的，站长账号走注册流程拿回，然后提权。**提权 SQL 已在库副本上跑通**：

```sql
UPDATE users SET role = 'admin' WHERE username = 'Felix' COLLATE NOCASE;
```

实测（副本上注册 `PromoteTest` → 提权 → 用**同一个 cookie** 再访问）：

| 时刻 | `/admin` |
|---|---|
| 提权前 | **403** |
| 提权后（未重新登录） | **200** |
| 不带 cookie | 403 |

结论：角色是**每次请求现查**的，改完立刻生效，不需要重新登录。
`created_at` / `last_login_at` 会从新注册那一刻重新算，这是唯一代价。

### 遗留：找回密码

**用户已决定暂不加「用环境变量重置密码」**，既定路线是：

1. 账号关联邮箱
2. 邮箱验证
3. 靠邮箱重置密码

做到第 3 步之前，忘了密码**只能删账号重建**（上面那套）。之所以把这条单列，是因为
2026-09-22 真的撞上过一次：当时没有这条路，我判断失误走了删数据，
结果那个账号的密码哈希再也拿不回来（free page、Docker 卷、临时副本都试过，
副本还被我自己清理 `/tmp` 时删掉了）。

---

## 阶段 13：对标 B 站的用户可操作性与私信（**暂缓**，2026-09-22 用户决定先不做）

盘点完现状后用户决定暂停，本轮**不动代码**。这一节留的是盘点的结论与用户已定的策略，
免得下次从头再查一遍。

### 现状盘点

`/me` 一个页面里已经能做的：注册 / 登录 / 登出 / 改密码（会踢掉其它设备）、
**改昵称与改简介**、亮暗与跟随系统 / 自定义主色 / 背景图（外链或上传）。
互动侧已有文章与光遇评论（含楼中楼回复）、光遇代跑评价（1–5 星）。
公开资料页 `/user/:username` 目前只显示显示名、简介、用户名三样。

对标 B 站真正缺的：**头像**（完全没有）、**私信**、**消息通知**（有人回复你或私信你，
你现在完全不知道）、个人主页聚合、关注 / 粉丝、@提及。

刻意不做的两类：等级 / 经验 / 大会员（个人站的内容供给量喂不饱激励体系，做出来只是
个空数字）、收藏夹 / 投币 / 弹幕（依附视频形态，本站的内容形态是 Markdown 文章）。

### 用户已定的私信策略（原话留档）

> 任何人对任何人，未互相关注的限制只能发一条文本消息，超出部分，每超出一条文本消息
> 扣 0.1CNY，每超出一个音视频消息扣 0.3CNY

三处设计后果，开工前必须一起处理：

1. **「互相关注」变成了前置依赖。** 没有关注关系就分不出「未互关只能发一条」，
   所以原本列为「可选」的关注 / 粉丝，在这个策略下是私信的前置。
2. **这是收费设计，不是功能设计。** 连带出支付通道、余额与充值、流水与对账、退款、
   未成年人保护，以及经营性网站的资质问题。具体合规边界需要核实——这超出我该替你
   判断的范围。另外单条 0.1 元这个量级走不了逐笔支付，只能是**预充值余额**扣减。
3. **音视频消息比文本大一个量级**：存储（现在上传只落本地磁盘）、转码、内容审核。
   真要做，应与文本私信分成两期。

### 待定

- 私信对管理员（你）是否可见：治理与隐私的取舍，未答。
- 其余模块（头像、消息通知、关注、个人主页聚合、@提及）同样暂缓。

---

## 社区投稿（UGC）— 已完成

官方内容与社区内容分区：官方内容仍是 `content/` 下的 Markdown，社区内容存 SQLite。
发布即公开（直接发布 + 事后管理），与评论的「先审后发」策略不同。

- 迁移：`migrations/0003_community.sql`：新增 `community_posts` 表（`kind` =
  `post` / `project` / `sky`，`meta` JSON 存类型专属字段），并把
  `comments.target_kind` 的 CHECK 扩展到 `community`
- 实现：
  - `src/community.rs`：输入校验（标题 / 摘要 / 正文 / slug / 标签 / 链接白名单）、
    列表、详情、发布、编辑、删除；正文用 `render_markdown(md, false)` 过滤裸 HTML
  - `src/pages/community.rs`：`/community`（按类型筛选）、`/community/new`、
    `/community/:username/:slug`、`.../edit`
  - `src/components/community.rs`：官方 / 社区分区切换条、内容卡片
  - 后台 `/admin/community`：按状态筛选，下架 / 恢复 / 删除（删除连带清理评论）
  - 首页与官方列表页都有分区入口；sitemap 收录已发布的社区内容
- 验证命令：
  - `cargo check --no-default-features --features ssr`
  - `cargo check --no-default-features --features hydrate --target wasm32-unknown-unknown`
  - 迁移 SQL 在内存库按 0001 → 0002 → 0003 执行：评论数据保留、
    唯一约束与 CHECK 生效（见提交记录中的验证）
  - 运行后 `scripts/check-links.sh`，路由列表已覆盖社区各入口

---

## 统一账户（Kanidm + Tuwunel）— P1 进行中（Kanidm 已常驻，接 Forgejo OIDC）

> 决策记录：因云服务器端口限制无法自建邮箱，改用 **Kanidm（IdP）+ Tuwunel（Matrix
> Homeserver）** 做统一账号。

**目标**：一套账号覆盖统一登录 / 昵称 / 私聊 / 联系人 / 多级管理员与 VIP；
下游 Forgejo、OpenCloud、主站、Matrix 全部走 Kanidm OIDC。

**组件与预算**：Kanidm（OIDC Provider，~80MB）、Tuwunel（Matrix，~200MB，最新
v1.9.3）；自托管 Element Web 另加 ~50MB（也可先用官方托管）。

**核查后的修正（相对初版提案）**：
1. Tuwunel 直接部署**当前最新版**（v1.9.3，2026-09-25）。提案里的 1.4.9 只是修复
   CVE 的下限；该 CVE 编号未在 GitHub Advisory 库查到，部署时对照 release notes。
2. Tuwunel 是**双角色**：对 Matrix 客户端是 OIDC 授权服务器（MSC3861），对上游是
   relying party。需同时配置 `[[global.identity_provider]]`（自定义 brand +
   `issuer_url`/`discovery_url` 指向 Kanidm）与内置 issuer；回调固定为
   `/_matrix/client/unstable/login/sso/callback/<client_id>`。
3. Kanidm 的 `update-claim-map` 命令**未在稳定文档出现**（稳定文档只有
   `update-scope-map`：组→scope）。P0 必须实机 `kanidm system oauth2 --help`
   确认 claim 映射的真实命令与格式。
4. OpenCloud 接外部 IdP 最复杂（需 `PROXY_AUTOPROVISION_ACCOUNTS`、
   `PROXY_USER_OIDC_CLAIM` 等自动供应，内部 IDM 仍是空间/共享事实源），放到 P3；
   失败则接受 OpenCloud 是一个例外。

**P0 验证结果（已完成，本地实测通过）**：
- Kanidm **1.11.2** 临时实例跑通（卷 `kanidm-p0-data`，`127.0.0.1:8443` 自签 TLS，容器 `kanidm-p0`）
- 命令核对：`system oauth2 update-claim-map <client> <claim> <group> [values…]` **存在**；
  合并策略 `update-claim-map-join <client> <claim> csv|ssv|array`（本项目选 `array`）
- 超级管理员是 **`idm_admin`**（不是 `admin`）；`kanidmd recover-account idm_admin`
  **直接输出新随机密码**（无需浏览器）；已写入宿主 `.env` 的 `KANIDM_ADMIN_PASSWORD`
- OIDC discovery 是**按客户端**的：
  `https://id.wraindrock.com/oauth2/openid/<client>/.well-known/openid-configuration`
- 本地已实测：建组 `felix-admins`、public 客户端 `web`、scope 映射、
  claim 映射（`groups`，join=array）+ discovery 200
- CF Tunnel 已加 `id.wraindrock.com → https://localhost:8443`（No TLS Verify）：
  **注意 Tunnel 的 Public Hostname 按顺序匹配，通配 `*` 必须排最后**，否则具体
  主机名会被通配截胡（P0 踩坑：表现为空 200）；加错时删掉 `*` 重加一次即可
- 公网实测：`https://id.wraindrock.com/oauth2/openid/web/.well-known/openid-configuration`
  200 / 1340B / issuer 正确

**落地阶段**：
- P0 验证（已完成）：Kanidm 试实例、claim 命令、按客户端 discovery、公网连通。
- P1（进行中）：
  - ✅ Kanidm 常驻化：quadlet `felix-homelab-kanidm.container`（只发布回环 8443）、
    正式卷 `felix-homelab-kanidm-data`（数据已从试实例迁移）、
    `config/backup/backup-kanidm.sh`（SQLite 在线 .backup + 证书）已接入每日/手动备份、
    install/uninstall/Makefile/`.env.example` 已接线；试实例卷 `kanidm-p0-data` 已删除
    （`kanidm-cli-home` 保留：CLI 登录会话缓存，丢了重新 login 即可）
  - ✅ Forgejo OIDC（已闭环）：机密客户端 `forgejo`；踩坑与结论：
    1) `create` 第三参是 landing URL，redirect 必须 `add-redirect-url` 加；
    2) 需 `warning-insecure-client-disable-pkce`（Forgejo 早期不带 PKCE）；
    3) `groups` 作用域输出组 SPN/UUID，Forgejo `--admin-group felix-admins@id.wraindrock.com`；
    4) 绑定：本地登录后访问 `/user/oauth2/kanidm`，`external_login_user` 记录 sub=Kanidm UUID；
    5) **终态**：本地密码已清空、本地注册关闭（`ALLOW_ONLY_EXTERNAL_REGISTRATION=true`）、
   新用户首次 OIDC 登录经 `/user/link_account` 确认后自动建号（`login_type=6`，已实测）；
   scope map 必须覆盖 `felix-users` 组，否则普通用户 Access Denied
  - ✅ 分组账户策略（实测）：`idm_all_persons` 放宽为 any + 最短 10 位（普通用户密码-only），
    `felix-admins`=mfa（多组取最严）；zxcvbn 4/4 仍强制。开户：`scripts/kanidm-adduser.sh`
    （建号→入组→7 天 onboarding 链接，私发即可，无需邮件）
  - ✅ 主站登录（无感原生，已闭环）：登录页即本站账号登录（用户名/密码/动态验证码），
    服务端内部走统一账号 HTTP 认证会话（init2 → begin → totp → password），用户界面零底层品牌；
    已绑定用户凭据不落本站库，过渡期未绑定老用户回退本站旧密码。
    （最初实现过 OIDC 跳转版，因“不能让用户看到/关联外部账号”而**移除**，站点 OIDC 客户端已删）
  - ✅ Forgejo 品牌清洗：认证源更名 Wraindrock（按钮「使用Wraindrock登录」），Kanidm 侧补新回调；
    其跳转过程仍会显示底层组件登录页，彻底透明化留待后续可选优化
  - ⏳ 注册入口（邀请码 + 管理员审核）：主站后端用 Kanidm 服务账号 api-token 自动建号
    → 生成 onboarding 链接站内展示；Kanidm 组 → 站点角色映射随注册一起定
- P2：Tuwunel 部署（**完全关闭联邦**），Element 接入；数据卷纳入备份。
- P3：OpenCloud 外接 IdP PoC。

**动手前必须先定的不可逆项**：Kanidm 的 SPN 域名（拟 `id.wraindrock.com`）、
Tuwunel `server_name`（拟 `wraindrock.com`，一旦初始化不可改）。

**必须一并补进方案**：Kanidm 数据库与 Tuwunel 数据卷纳入 backup-run/`KEEP_DAYS`
及恢复流程；Kanidm 组 → 站点 roles（super/communitymaster/skymaster/agentmaster）
映射表；VIP 的权限定义（先定义能做什么，再谈同步）；无邮件找回的线下重置流程。

**备用路径**：Tuwunel 由 Conduwuit 衍生，回退选项为 Continuwuity（同源、内存相近）。

---

## 商业化：服务购买（决策已定，待实现）

**商品与定价**（导航「服务购买」已上线目录页，下单/支付待接）：

- **AI Agent（时间池充值，用户确认）**：
  - **不再按周/月/年订阅**；改「充值进时间池 + 自助开启」：Agent 由用户自行选择/创建并随时启停，计费全自动。
  - **单一费率（试运行）**：¥19 = 30 天运行时间（≈¥0.0264/小时、¥0.63/天）；充值任意金额
    （示例：¥6≈9.5 天、¥99≈156 天）。保留按资源情况调整费率的权利（调整前公告）。
  - **扣费档位**：运行 1×、睡眠 0.5×（睡眠仍占用资源）、彻底停止 0×。
  - **数据保留**：时间用完后数据默认保留 **1 天**（免费）；继续保留按 **0.3×**（≈¥0.19/天）从池中扣。
  - **可选功能**：允许透支，最低 −¥5（用于延长使用与数据保存）；欠费超限即停止。
  - **本站维护期间概不计费**（计量暂停，含睡眠与数据保存费用）。
  - 实现：按状态采样累计扣费；启动/唤醒前校验余额（余额≤0 且未开透支则拒绝）；后台看板
    （池余额、各 Agent 消耗、欠费与维护暂停标记）。
  - **旧 Agent 结转（已完成）**：每个实例的剩余有效期按 1:1 计入时长池
    （`time_pool_entries.kind='migrate'`，`agent_subscriptions.pooled_at` 标记幂等）；
    结转后实例卡片不再显示逐实例到期时间，改显示「时间已计入时长池」。
  - **Agent 卡片三行样式（已完成）**：① 用户自定义备注（铅笔可改，40 字内，落库）
    ② Agent 类型 · 运行状态 ③ 链接（打开 ↗）；卡片本体不再是链接，只有第三行可点。
  - **「我的订阅 → 我的 Agent」布局（已完成）**：时长池卡片独占一行，进度条展示
    总时长/剩余时长，支持统计窗口切换：全部 / 本月 / 本周 / 今天（`agent_time_pool` server fn，
    计量系统落地前返回 0 并显示空态提示）。
- **OpenCloud**：5GB ¥5/月（¥49/年）；每 +5GB 加 ¥4.5/月（¥44.1/年，九折且**不累进**——资源受限）；
  单用户上限 30GB；全站上限 200GB。
- **Forgejo**：与 OpenCloud 同价，**容量共享**（一份容量两服务通用）。
- 金额**不取整**，最多保留两位小数。

**临时空间（1GB/人 → 改为动态均分）**：全站预留 50GB 池，动态均分给所有注册用户
（每人份额 = 上限/注册用户数）；池满或某用户占用达其均分额 → 仅限制该用户**上传（写）**，
**不限制下载（读）**。

**支付**：Creem（Merchant of Record，个人可用），checkout + webhook；订单/订阅/续费进后台。

**实施顺序**：主站 OIDC（并联，保留密码登录）→ 邀请码注册 + 管理员审核 →
下单/支付/续费 → 配额落地（OpenCloud + Forgejo）→ OpenCloud 按统一账号重做。

**主站 OIDC 客户端（Kanidm 侧已就绪）**：机密客户端 `site`，
回调 `https://www.wraindrock.com/auth/oidc/callback`，scope map 覆盖
`felix-users` 与 `felix-admins`；密钥在宿主 `.env` 的 `SITE_OIDC_SECRET`。

**后台大改（要求）**：自适应卡片式入口，卡片划分与管理员类型绑定
（super / agentmaster / communitymaster / skymaster），权责分明；
新增「服务」管理（订单审批、订阅、容量与临时池使用情况、200GB 上限看板）。

---

## 站点体验（已完成）

- 首页开场动画（黑底 Logo 飞入顶栏，内容淡入；每会话一次，尊重减少动效偏好）
- 导航调整：「购买订阅」（原「服务购买」）＋「我的订阅」
- 「我的订阅」页：已订阅 Agent（移自首页）＋ 容量订阅（OpenCloud/Forgejo 共用，待订单系统补数据）

---

## 富媒体消息体系（下一步方向，规划中）

现状：本站内容以文本为主。目标：图片、视频、文件、语音消息、语音转文字。

**存储压缩策略（实测数据，zstd 1.5.7，单线程）**：

| 类型 | 样本 | -1 | -3 | -7 | -19 |
| --- | --- | --- | --- | --- | --- |
| 文本/源码（高冗余） | 8.7MB | 25.9% @384MB/s | 2.2% @631MB/s | 2.0% @398MB/s | 1.1% @48MB/s |
| JSON | 5.3MB | 25.0% @341MB/s | 27.0% @232MB/s | 24.5% @81MB/s | 21.8% @1.9MB/s |
| 语音 PCM(WAV) | 3.5MB | 16.2% @735MB/s | 8.9% @525MB/s | 8.9% @368MB/s | 8.5% @51MB/s |
| 截图 PNG（已压缩） | 0.14MB | 97.1% | 95.8% | 95.5% | 94.4% |
| MP3（已压缩） | 0.04MB | 93.4% | 91.7% | 91.1% | 91.0% |

**定稿参数（用户修订版）**：
- **上传一律原样存储**（Range/秒开），压缩由后台按龄执行：
  **>7 天 → `zstd -3`；>1 个月 → `zstd -7`；>3 个月 → `zstd -19`**。
- **热文件例外**：近 7 天被访问过、或累计访问次数 ≥30 的，**跳过压缩与升级**
  （保用户访问体验；如需可再降级或不压缩）。
- 后台任务：站点进程内每 24h 一轮（启动 5 分钟后先跑），每轮每档最多 50 个文件。
- **跳过压缩**：图片（png/jpg/webp/gif）、**全部音频**（mp3/opus/aac/webm 等已压缩；
  WAV 虽可压到 8.9% 但播放需要 Range 拖动，故原样存）、视频、压缩包/PDF——
  实测增益 <5% 或影响播放；兜底规则：压缩后 ≥95% 原大小则丢弃压缩版存原文件。
- **访问加速**：媒体 URL 带原始扩展名（`/media/{id}/{name}.ext`），命中 Cloudflare
  默认按扩展名缓存；配合 `Cache-Control: immutable` 实现边缘缓存（公网链路实测较慢，
  缓存后重复访问走边缘）。
- **配额口径**：用户容量按**原始大小**计（`original_size` 计费/配额，`stored_size` 记实际占用）；
  压缩节省归平台，不作为用户额度。
- **实现**：上传时按 MIME 白名单决定是否压缩（文本类 + PCM/WAV 用 -3）；
  `media` 表记录 `compression`（none/zstd-3/zstd-7/zstd-19）与 `last_accessed_at`；
  重压缩任务按访问时间升级级别。

**分期**：
- **M1 图片**：通用媒体表 + 上传接口（复用现有 uploads 基建：multipart、体积上限）；
  社区帖子/评论内嵌（正文引用 + 上传按钮/拖拽粘贴）；缩略图与 EXIF 清理。
- **M2 文件**：任意文件附件（类型/大小白名单），下载统计。
- **M3 语音消息**：浏览器 MediaRecorder 录音（webm/opus；iOS 兼容 mp4/aac）→ 上传 →
  站内播放条（时长、波形可选）。
- **M4 语音转文字（STT）**：✅ 服务已常驻（faster-whisper large-v3 + NVIDIA GPU，
  `felix-homelab-whisper`，OpenAI 兼容 `/v1/audio/transcriptions`，实测中文 0.77s/5s）。
  站点侧待做：`/api/stt` 代理 + 录音组件（录音→转写→可编辑后发送）。
- **M5 视频**：直传 + 播放（限制大小/时长），转码与封面帧后置。

**已定决策（用户确认）**：
- **STT：站内自托管 + GPU 加速**。方案：whisper.cpp server（OpenAI 兼容
  `/v1/audio/transcriptions`），Podman + NVIDIA CDI（宿主 RTX 4060 8GB，驱动 615/ CUDA 13.4，
  `/var/run/cdi/nvidia.yaml` 已就绪）。模型优先 large-v3-turbo（8GB 可跑、快且质量高），
  备选 medium；模型卷入备份策略；站点通过 `/api/stt` 代理调用。
- **媒体容量：计入现有容量池**（OpenCloud/Forgejo 共用），与订阅体系自然打通。
- **外置云存储（新需求）**：可购买开通「个人外置云存储」——用户绑定自己的 WebDAV（后续可扩 S3 等），
  上传的媒体**优先写入其个人外置存储**，站内保存索引（缩略图可选本地缓存）。需要：
  凭据加密存储、可用性探测、不可用时回退本地并提示、外置容量不计入站内容量池。
- **内容面：全部一起做**——社区帖子、评论、私信（待做）统一走同一套媒体组件。

---

## 容量池与外置存储（本轮新增）

- **容量池**：`capacity_pool_entries`（字节入账）+ 已用 = `SUM(media.original_size)`；
  「我的订阅」新增与「时间池」同款的**容量池卡片**（进度条 + 全部/本月/本周/今天）。
  上传校验：已开通（总额>0）才限制，超额 413；未开通（过渡期）不拦。
- **个人外置云存储**（独立订阅，暂定 ¥9/月、¥29/季、¥119/年）：
  接口优先 **WebDAV**（官方依据：RFC 4918《HTTP Extensions for WebDAV》，
  https://www.rfc-editor.org/rfc/rfc4918，第 4-6 章属性/集合/锁；实现前再按所选服务商
  的官方文档核对认证与能力），后续可扩 S3 等；**默认加密存储、可关闭**；
  独立「存储池」展示，不占用本站容量池。
- **管理员扩容语义（更正）**：管理员可用自有外置存储**扩容本站全站最大可用容量**
  （提升全站上限，而非只给某个服务），以缓解本机硬盘容量限制。
- **私信系统：滞后**，需要审慎设计（身份、隐私、审核、媒体复用），暂不做。

---

## 订单与收款（本轮实现）

- **订单核心**（`orders` + `entitlements`）：三类商品——AI Agent 时间池充值（金额→秒，
  ¥19=30 天）、容量池订阅（GB+周期，价格表服务端校验）、个人外置云存储（¥9/月、¥29/季、¥119/年）。
- **人工通道**（当前默认）：后台「服务订单」页确认收款 → 幂等发放权益
  （时间池写 `time_pool_entries`；容量/外置写 `entitlements` 带到期）。
  实测：¥19 → 时长池 +30 天；5GB/月 → 容量池 5GB；外置存储开通至 +30 天。
- **Creem 通道**（已接入代码，未配置 Key 时自动回退人工）：
  - 官方依据（docs.creem.io，2026-09-30 拉取）：
    - 创建结账 `POST https://api.creem.io/v1/checkouts`，头 `x-api-key`，
      体 `{product_id, success_url, metadata}`，返回 `checkout_url`（跳转收款页）；
    - 回调头 `creem-signature` = **HMAC-SHA256(webhook_secret, 原始请求体)**；
      事件 `checkout.completed` / `subscription.paid`（官方建议后者用于开通）；
    - 金额为整数分；`metadata.order_id` 用于回联订单（失败时回退 checkout.id 匹配）。
  - 需要配置：`CREEM_API_KEY`、`CREEM_WEBHOOK_SECRET`、
    `CREEM_PRODUCT_AGENT_TIME` / `CREEM_PRODUCT_CAPACITY` / `CREEM_PRODUCT_EXTERNAL_STORAGE`，
    并在 Creem 后台把回调指向 `https://www.wraindrock.com/api/payments/creem/webhook`。
  - 开通后建议在测试模式（test-api.creem.io）先跑一笔验证验签与发放。
- **待做**：容量过期后的超额处理提示、外置存储的 WebDAV 绑定界面、订单退款/对账、
  Creem 订阅生命周期（取消/逾期）同步、管理员全站容量扩容核算。

---

## 个人外置云存储（WebDAV）调研结论（实现前必读）

**官方依据**（均 2026-09-30 拉取原文）：
- RFC 4918《HTTP Extensions for WebDAV》：https://www.rfc-editor.org/rfc/rfc4918
  方法：PROPFIND（列目录/取属性，Depth 0/1）、PUT、MKCOL、GET、DELETE；响应 207 Multi-Status。
- Nextcloud 官方用户手册（Accessing files using WebDAV）：
  https://docs.nextcloud.com/server/latest/user_manual/en/files/access_webdav.html
  地址形如 `https://<域名>/remote.php/dav/files/<USERNAME>/`；官方建议使用**应用专用密码**
  （设置 → 安全 → App password，可随时撤销）。
- 坚果云官方帮助（WebDAV 说明）：https://help.jianguoyun.com/?p=2064
  需在「账户信息 → 安全选项 → 第三方应用管理」生成**应用密码**；
  **限制**：单文件 ≤500MB；访问频率免费用户 30 分钟 ≤600 请求（付费 ≤1500）；
  单次请求条目 ≤750（需分页）。

**设计结论（接口与用户习惯）**：
- 认证一律 HTTP Basic（账号 + 应用密码）；多数平台禁止主密码，绑定表单与引导文案必须强调。
- 连接前先 **OPTIONS** 看 `DAV:` 头，再 **PROPFIND Depth:0** 取
  `quota-available-bytes` / `quota-used-bytes` —— 用于「存储池」展示与写入前余额校验。
- 上传用 PUT（媒体大小远低于 500MB 限制；请求数远低于频率限制）；
  XML 解析引入 `quick-xml` 只取必要字段。
- **Range/秒开问题**：外置直读 + 加密会让 Range 变差 → 策略：外置为主存储 + 站内小缓存
  （缩略图/近期访问）；大视频在外置上的拖动播放需整文件下载解密后再切片（UI 需注明）。
- **默认加密可关**：平台托管密钥（AES-GCM/age），密钥随站内备份；关闭加密时直接透传。
- **用户引导**：内置常见平台预设（Nextcloud / 坚果云 / 群晖 / 自建），
  「测试连接」按钮（OPTIONS+PROPFIND）通过后再保存；凭据加密存储、失败自动回退本地并提示。
