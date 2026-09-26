# Grant Felix Homepage — 设计文档

> 用 Leptos + Axum + SQLite 重写的个人主页。本文说明**为什么这样切**、边界在哪、
> 以及明确不做什么。实施拆解与验证命令见 `TODO.md`。

---

## 一、定位与边界

一个自持的个人主页，对外是博客、项目展示、光遇子站与个人介绍；对内是一套只有
站长需要登录的后台。代码托管在本地 Forgejo 私有仓库，不出公开镜像。

明确落在范围内的事：

- 内容区：博客、项目、光遇（攻略 / 画廊 / 代跑评价）、关于、联系
- 互动区：注册登录、评论、光遇代跑评价
- 个性化：亮暗自动 / 手动切换、自定义配色、自定义背景图（**偏好存在账号里**）
- 运维面：单进程 + 单 SQLite 文件，配一条构建命令

明确不在范围内的事，见第十一节。

---

## 二、技术栈与选型理由

整站只有一条构建链：`cargo leptos`。

| 层 | 选型 | 为什么是它 |
|---|---|---|
| 视图 | Leptos 0.8（SSR + 水合） | 组件化与细粒度响应式都在 Rust 里，不写手写 JS；服务端渲染保证 SEO 与首屏 |
| 服务端 | Axum 0.8 | Leptos 官方集成（`leptos_axum`），路由与提取器都与 Leptos 共用一套 |
| 数据库 | SQLite + sqlx 0.8 | 只存互动数据，单文件零运维；`sqlx` 编译期校验 SQL |
| 构建 | cargo-leptos 0.3 | 一条命令同时产出 wasm 包与服务端二进制 |
| 样式 | 原生 CSS + 设计令牌 | 见下方说明 |

关于样式为什么不用 Tailwind：本设计的核心机制是**运行时改变 CSS 自定义属性**
（每个账号自己的配色与背景图），这本来就必须落在 CSS 变量上；站点规模只有十几个
页面，Tailwind 的收益有限，却要在「全栈 Rust」之外再引入一条 Node 构建链。
若后续页面量变大再引入不迟。

**视觉基调**：现代作品集风——大字号排版、克制的留白、以项目卡片为主要视觉单元。
亮暗切换与自定义配色都建立在这套版式之上，只替换颜色令牌，不改变版面结构；
因此第六节的个性化能力不需要为每个主题各写一套版式。

**渲染模式**：所有数据驱动的路由都显式使用 `SsrMode::Async`，即服务端等全部数据
就绪后一次性吐出完整 HTML。Leptos 默认的 `OutOfOrder` 流式渲染虽然 TTFB 更低，
但会把解析好的内容放进 `<template>` 再由内联 JS 搬位——禁用 JS 的访客与爬虫只能
看到 fallback，而且 `<head>` 先于数据发出、写在 `<Suspense>` 里的标题会丢失。
本站是内容站，SEO 与「不依赖 JS 也能读」优先于这点 TTFB。

---

## 三、内容模型：Markdown 随仓库走

文章与项目介绍以 Markdown 文件存在仓库里，**改内容 = 提交**。理由是这个站的作者
是唯一作者，且已经在用 git；把内容放进数据库只会多出一套 CRUD 后台与备份逻辑，
而换不来任何版本化能力。

目录约定如下。

| 目录 | 放什么 | 关键 front matter |
|---|---|---|
| `content/posts/` | 博客文章 | `title`、`date`、`author`、`summary`、`tags`、`draft` |
| `content/projects/` | 项目条目 | `name`、`kind`、`author`、`stack`、`repo`、`demo`、`weight` |
| `content/sky/` | 光遇攻略与画廊 | `title`、`date`、`author`、`category`、`cover` |
| `content/pages/` | 关于、联系等静态页 | `title`、`updated` |

### 发布者：`author`

**内容由发布者发布，发布者写在自己的文件里。** 三类内容（文章、项目、光遇）的
front matter 都认 `author`，值填**站点账号的用户名**；省略时用 `DEFAULT_AUTHOR`
（当前是 `Felix`）兜底——单人站点里内容都归站长，逼每篇抄一行没有意义，将来多人
发布时各自写各自的用户名即可，不用改代码。

页面上显示「发布者：X」，`X` 是 `users` 表里那个账号的**显示名**，链接指向
`/user/:username`。站内没有同名账号时**只显示用户名、不给链接**——把人链到一个
404 比不给链接更糟。解析走一次批量查询（按去重后的用户名集合），不会每篇查一次库。

匹配是**大小写不敏感**的（与用户名本身的规则一致，见第五节），front matter 里写
`Felix` 还是 `felix` 都落到同一个账号；解析回来的 `username` 是**库里存的写法**，
链接用它拼，免得同一个账号出现两种大小写的网址。

`content/pages/` 没有这个字段：那些是站点自己的固定页（关于、联系），不是某个人
的发布物。

**项目条目的 `repo` 只在仓库确实可公开访问时才填。** 私有仓库不填——链到只有本机
能打开的地址（`http://127.0.0.1:3000/…`）对访客就是一个死链。

解析用 `gray_matter` 取 front matter、`pulldown-cmark` 转 HTML。内容在**进程启动时
一次性载入内存并建索引**：站点内容量在数百篇以内，内存占用可忽略，而省掉了每次请求
的文件 IO 与解析。代价是 `git pull` 后必须重启进程才能看到新内容——这与
「提交 → 部署 → 重启」的发布模型本来就是一回事，不算额外负担。

`kind` 取值限定为 `open`（开源）、`private`（私有）、`team`（团队），与旧站的
项目分类保持一致。

**静态页与光遇内容各有一条兜底规则**，都是为了避免「内容悄悄不见」：

- 静态页（`content/pages/`）文件缺失时，页面给出一句「这一页还没写」并指明文件该放哪，
  而不是 404——「关于」这类固定入口不该因为少一个文件就整页打不开。
- 光遇内容（`content/sky/`）的 `category` 会**归一化到白名单**（`gameplay` /
  `gallery`）。只做「字段缺失时给默认值」是不够的：值拼错时原样保留，两个分类页都
  匹配不上，内容就静默消失了。现在写错会归到 `gameplay` 并在启动日志里告警。

分类是从 URL 传来的，因此分类页也先与白名单比对，不在名单里直接 404——「这里还没有
内容」和「没有这个页面」是两回事。

---

## 四、数据库：只存互动数据

数据库里只有「用户产生的东西」，内容一律不进库。表结构如下。

| 表 | 用途 | 关键字段 |
|---|---|---|
| `users` | 账号 | `username`、`password_hash`、`role`、`status` |
| `sessions` | 登录会话 | `token_hash`、`user_id`、`expires_at` |
| `comments` | 评论 | `target_kind`、`target_slug`、`user_id`、`parent_id`、`status` |
| `sky_reviews` | 光遇代跑评价 | `user_id`、`rating`、`body`、`reply`、`status` |
| `user_prefs` | 个性化偏好 | `theme_mode`、`accent`、`bg_kind`、`bg_value` |

几处刻意设计：

`comments.target_kind` 与 `target_slug` 组成多态目标，取值 `post` 与 `sky`。
这样博客与光遇页面共用一张评论表、一套审核逻辑，而不必为每个板块复制一份。

`comments.status` 取值 `pending`、`approved`、`rejected`。新评论一律先落
`pending`，因为开放注册的站点必然会被灌水，先审后显示比事后清理省力。

评论支持二级回复（`parent_id`）。**父评论不存在于可见集合时，回复升为顶层显示**：
将来后台把某条已批准评论改成拒绝，它的回复不该跟着凭空消失——作者会以为自己的回复
被吞了。另外**父评论必须是同一目标下已批准的评论**，否则可以把回复挂到别的文章下面。

`sky_reviews` 的评价正文是**纯文本**，不走 Markdown：内容短、结构化，多一层渲染没有
收益，反而多一个注入面。它以文本节点输出，由 Leptos 自动转义。评论正文则走
`render_markdown(md, false)`——支持 Markdown 但**过滤掉裸 HTML**。

`sessions` 只存令牌的 SHA-256 哈希，不存明文。数据库文件万一泄露，攻击者也无法
直接拿它登录。

`user_prefs` 与 `users` 一对一，单独一张表是为了让「未登录访客的偏好」也能用同一套
结构序列化进 cookie，不必为匿名用户另立模型。

---

## 五、账号与鉴权

注册只需用户名与密码，**不验邮箱**（本站不发邮件，收邮箱没有意义，所以 `email`
字段可空，仅作为用户自愿填写的联系方式）。

**用户名大小写不敏感**：`Felix` 与 `felix` 是同一个人，不能各占一个账号。做法是
存储照旧保留用户自己挑的写法（展示时 `Felix` 比 `felix` 好看），查找统一加
`COLLATE NOCASE`，数据库侧再用 `idx_users_username_nocase` 这个唯一索引兜住并发
注册。`migrations/0002` 负责建索引，**并先把已有的重名账号合并掉**——不先合就建
索引会直接失败，表现成「服务起不来」。合并规则是确定性的：留 id 最小的那个，引用
改挂过去，角色取组里更强的（有人是管理员就是管理员），状态取封禁（否则重名能绕过
封禁）；密码哈希只能留保留者的，被合并方的原密码从此不能登录。

**昵称（`display_name`）不限长度与字符类型**，只有「不能为空」这一条。它是个人表达，
卡在 30 字上只会逼人缩写。代价是它出现在每个带昵称的页面上，属于已知取舍。

密码用 `argon2` 的 Argon2id 变体哈希，入库的是 PHC 串（自带随机盐与参数，将来调参
不影响老密码校验）。会话用 32 字节随机令牌 + `HttpOnly` cookie，`SameSite=Lax`；
**库里只存令牌的 SHA-256**——令牌本身是 256 位随机数，不存在被爆破的可能，只需要
保证库泄露后不能直接拿来登录，而 argon2 每请求算一遍太贵。上线改用 HTTPS 后设
`COOKIE_SECURE=1` 即可加上 `Secure`。

登录失败时不区分「用户不存在」与「密码错」，且用户不存在时也照样做一次哈希校验：
否则「存在」要花几十毫秒而「不存在」立即返回，可以用响应时间枚举出已注册的用户名。

角色只有两个：`admin` 与 `user`。站长账号**不走开放注册**，由一次性的环境变量
初始化（`ADMIN_USERNAME` + `ADMIN_PASSWORD`）；账号已存在时只补角色、不动密码，
否则每次启动都会把改过的密码重置回去。

封禁（`status = 'banned'`）在会话查询里就带 `u.status = 'active'` 条件，因此**立即
生效**，不需要额外删会话行。

后台 `/admin` 只做三件事：审核评论、管理用户状态、回复光遇评价。**不提供文章
增删改**——内容走 Markdown 提交，后台里做编辑器与本设计的前提冲突。

**授权在每个 server function 里，不在页面上。** 后台页面判一次身份，只是为了让非
管理员看到一个像样的 403；接口是公开可达的，把按钮藏起来挡不住直接构造请求。
状态与角色取值一律走白名单，管理员也不能改自己的状态或角色——否则一次误操作就能让
唯一的站长账号变成被封禁的普通用户，后台再也进不去。

页面级判断还有个实现上的约束：**要用页面自己拉的数据，不能去读 App 级的资源**。
App 级资源建在路由子树之外，路由页面的 `<Suspense>` 等不稳它，读到的值时有时无
（实测连打 5 次能冒出 2 次 403）。

---

## 六、主题与个性化

主题有三档：`auto`（跟随系统）、`light`、`dark`。配色与背景图是账号级的：

| 偏好 | 存储位置 | 作用范围 |
|---|---|---|
| 主题模式 | `user_prefs.theme_mode`，未登录时存 cookie | 亮暗切换 |
| 主色 | `user_prefs.accent` | 链接、按钮、强调元素 |
| 背景图 | `user_prefs.bg_kind` + `bg_value` | `url`（外链）或 `upload`（本地上传） |

**避免首屏闪烁**是这里唯一的技术难点。做法是服务端渲染时就把偏好读出来：已登录
用户读 `user_prefs`，未登录访客读 cookie，直接把对应的 class 与 CSS 变量写进
`<html>`。这样第一帧就是最终外观，不需要等水合后再改样式。

未登录访客的偏好落在三只 cookie 上，服务端从请求头读、浏览器端从 `document.cookie`
读，两端取同一个值，因此水合时不会不匹配。

| cookie | 存什么 | 校验规则 |
|---|---|---|
| `gf_theme` | `auto` / `light` / `dark` | 白名单，无法识别则回落 `auto` |
| `gf_accent` | 主色，六位十六进制 | 只接受十六进制，其余丢弃 |
| `gf_bg` | 背景图 URL（percent 编码） | 只接受 `http(s)://` 与站内相对路径 |

**这三个值来自客户端并被拼进 HTML 属性，一律当作不可信输入。** 主色限死六位
十六进制；背景图除协议白名单外还要求字符集白名单（不含引号、括号、尖括号），
并且必须排掉 `//host/x.jpg` 这类协议相对地址——它以 `/` 开头、看着像站内路径，
实际会去加载第三方资源。

主色前景色按 WCAG 相对亮度分别算出「配白字」与「配黑字」的对比度，取更高的那个，
而不是拍一个亮度阈值：像纯绿 `#00ff00` 这种通道差异极大的颜色，粗略加权公式会选错。

`auto` 模式也会在 `<html>` 上留下 `class="auto"`。这不是冗余：Leptos 的 `class`
属性无论空否都会输出，与其留下 `class=""`，不如给一个自解释的值；而 `html.auto`
匹配不上 `html.light` / `html.dark`，系统偏好照常生效。

登录用户的偏好以数据库为准；未登录访客改主题时**同时写 cookie 与 localStorage**
——cookie 给服务端渲染用，localStorage 给「cookie 被清掉但浏览器还在」的场景兜底。

背景图上传走一个独立的 Axum 路由（`POST /api/me/background`，multipart），而不是
Leptos server function：文件上传用原生 multipart 更直接，而且普通
`<form enctype="multipart/form-data">` 提交即可使用，不依赖 JS。落盘到
`data/uploads/<user_id>/<随机名>.<ext>`，数据库只存 URL 路径（该目录已在
`.gitignore` 里），由 `GET /uploads/...` 提供。

上传这条路来自完全不可信的输入，因此定了四条：

- **类型只由文件魔数决定**，客户端报的 `content-type` 与扩展名一概不作数。
- **只放行位图**（PNG / JPEG / GIF / WebP）。SVG 是文本、能内嵌脚本，作为用户可
  上传的内容是明确的 XSS 入口，直接不接受。提供文件时附
  `X-Content-Type-Options: nosniff`，避免浏览器改判类型。
- **路径走严格白名单**：`/uploads/<数字>/<字母数字文件名>.<位图扩展名>`，不合形式
  一律 404。`..`、绝对路径、别的用户目录、`.php` 之类从根上进不来。
- **换图先写库、成功后再删旧文件**。顺序反了会在写库失败时把用户原来的图弄丢。

CSRF 靠会话 cookie 的 `SameSite=Lax`：跨站表单提交带不上这个 cookie，因此这些
POST 不需要额外的令牌。

**账号偏好与 cookie 的同步方向是单向的**：登录时把账号里的偏好镜像进 cookie，
之后每次修改两边都写。这样账号是真相来源，而渲染路径仍然只读 cookie、保持同步。

---

## 七、页面与信息架构

路由表如下。

| 路径 | 页面 | 备注 |
|---|---|---|
| `/` | 首页 | 概览与入口 |
| `/blog`、`/blog/:slug`、`/blog/tag/:tag` | 博客 | 列表、正文、标签 |
| `/projects`、`/projects/:slug` | 项目 | 按 `kind` 分组 |
| `/sky`、`/sky/gameplay`、`/sky/gallery` | 光遇 | 攻略与画廊 |
| `/sky/boosting` | 光遇代跑 | 介绍 + 评价区（可提交） |
| `/about`、`/contact` | 关于、联系 | 静态页 |
| `/login`、`/register`、`/logout` | 认证 | |
| `/me` | 个人设置 | 主题、配色、背景图、改密码 |
| `/user/:username` | 公开资料页 | |
| `/admin`、`/admin/comments`、`/admin/users`、`/admin/sky-reviews` | 后台 | 仅管理员 |
| `/rss.xml`、`/sitemap.xml`、`/robots.txt`、`/healthz` | 机器可读 | `healthz` 顺带探测数据库 |

旧站的「图标导航首页」保留：首页三个入口（博客 / 光遇 / 项目）是它的核心识别度。

---

## 八、目录结构

```
.
├── flake.nix             # 开发环境与打包入口（第九节）
├── flake.lock            # 钉住 nixpkgs 与 rust-overlay 的具体修订
├── nix/
│   ├── package.nix       # 构建派生：cargo-leptos 产出 wasm + 服务端二进制
│   └── module.nix        # NixOS 模块：systemd、nginx、备份（第十节）
├── Cargo.toml、Cargo.lock
├── DESIGN.md、README.md、TODO.md
├── content/              # 内容随仓库走（第三节）
│   ├── posts/  projects/  pages/  sky/
├── public/               # 静态资源，cargo-leptos 同步到 target/site
├── style/main.css        # 设计令牌与全局样式
├── src/
│   ├── main.rs           # Axum 入口、非 Leptos 的路由、启动顺序
│   ├── lib.rs            # 水合入口 + recursion_limit
│   ├── app.rs            # 文档外壳 shell() + App + 路由表
│   ├── state.rs          # AppState（Leptos options + 连接池）
│   ├── db.rs             # 连接池与迁移
│   ├── content.rs        # Markdown 四个集合的载入、索引与查询
│   ├── admin.rs          # 后台的 server function（每个自带授权校验）
│   ├── auth.rs           # 账号、会话、资料
│   ├── comments.rs       # 评论
│   ├── reviews.rs        # 光遇评价
│   ├── theme.rs          # 外观偏好（cookie 与账号同步）
│   ├── seo.rs            # RSS、sitemap、robots.txt
│   ├── uploads.rs        # 背景图上传与提供
│   ├── components/       # 布局、评论与评价组件
│   └── pages/            # 各路由页面（含 admin.rs）
├── migrations/           # sqlx 迁移
└── data/                 # 运行时：site.db、uploads/（已忽略，绝不进 store）
```

---

## 九、构建与运行

开发环境是 **Docker 上的 NixOS 容器**，因此工具链由 `flake.nix` 声明，不靠「照着
文档装一遍」。这样「我这儿能编、你那儿编不过」这类问题从根上少一大半。

**起站点（推荐）**：

```bash
docker compose up -d      # 起来后浏览器打开 http://127.0.0.1:8080
docker compose logs -f    # 看构建与运行日志
docker compose exec dev bash
docker compose down       # 停止；卷保留，下次不用重新下载工具链
```

`compose.yaml` 里有四处值得说明：

- **端口只绑回环**（`127.0.0.1:8080:8080`），与 Forgejo 那份一致，局域网其他设备连不上。
- **必须覆盖 `LEPTOS_SITE_ADDR`**。`Cargo.toml` 里 `site-addr` 是 `127.0.0.1:8080`
  ——本机开发的合理默认，但容器里绑回环的话**发布出去的端口到不了**，浏览器里就是连不上。
  compose 把它覆盖成 `0.0.0.0:8080`，本机裸跑时仍保持回环。
- **`nix store` 与 `target/` 都放命名卷**。前者避免每次 `up` 重下几 G 工具链；后者既让
  增量编译跨重启生效，也避免容器里以 root 运行的 cargo 把宿主机目录写成 root 属主。
- **`NIX_CONFIG` 打开 flakes**，而不是去改镜像里的 `/etc/nix/nix.conf`——配置随
  compose 文件走，换机器也一样。

另外容器里以 root 跑 nix、而 `/work` 是宿主 uid 1000 的文件，libgit2 会因所有权不符
拒绝解析 flake，所以命令里先声明 `safe.directory`。这是容器场景特有的，真实 NixOS
开发机上不会遇到。

**数据落在命名卷里**，`docker compose down && up` 之后账号、评论、上传的图片都还在
（已实测）。但要注意 `down -v` 会连卷一起删，那是真的清库。

**环境变量是容器创建时定下的**：`docker compose start` / `restart` 只是把**同一个**
容器再跑一遍，不会重新读 compose 与 `.env`。改完 `.env` 必须
`docker compose up -d --force-recreate` 才生效。这个坑有实际后果：删掉库里的管理员
账号后如果只是 `restart`，容器里还留着旧的 `ADMIN_USERNAME` / `ADMIN_PASSWORD`，
启动时会**默默把账号建回来**，看起来像删除没成功。

**别在 dev 服务跑着的时候做 release 构建。** debug 与 release 两种构建共用同一个
输出目录 `target/site`，`cargo leptos build --release` 会先把 `target/site/pkg/`
清掉再写 release 产物——这期间 dev 站的 `.js` / `.wasm` 是 404，页面能打开但**水合
不起来**（实测踩到过）。要切回 dev，重跑一次 `cargo leptos build` 即可。发布用
Nix 构建（`nix/package.nix`）不受影响，它在自己的构建沙箱里跑。

**验收命令**是这两个脚本，都是打真实对象、退出码非 0 即不合格：

| 脚本 | 验什么 |
|---|---|
| `scripts/check-links.sh` | 打一遍真实服务：16 个代表路由 + 页面里每个站内链接。默认 `http://127.0.0.1:8080`，`BASE_URL` 可指向别处（部署后当冒烟测试用） |
| `scripts/check-migration.sh` | 造一份**带重名账号**的库，跑一遍 `0002` 再逐条核对合并结果与唯一索引 |

项目没有单元测试——这一层全是「渲染出来的 HTML 对不对」与「迁移有没有把引用搬干净」，
写 Rust 断言只会变成脆弱的快照，不如直接验证真实对象。

**别用 `cp` 备份 SQLite 库。** 库跑在 WAL 模式下，提交了的数据可能还只在
`site.db-wal` 里，主库文件是旧的（实测踩到过：`docker compose cp` 只拷了 `site.db`，
读出来的是几小时前的状态，看起来像迁移没生效）。要看真实内容就把
`site.db`、`site.db-wal`、`site.db-shm` 三个一起拷；要备份就用 `sqlite3 .backup`
（模块里的定时备份用的就是它），它自己知道 WAL 该怎么处理。

**站长账号**在开发环境里也要自己给：往项目根目录的 `.env` 写两行即可，compose 会
自动读它，而 `.env` 已在 `.gitignore` 中——密码不该进仓库。

```bash
echo 'ADMIN_USERNAME=felix' >> .env
echo 'ADMIN_PASSWORD=至少8位' >> .env
docker compose up -d
```

**在容器里直接开发**：

```bash
docker compose exec dev bash
nix develop -c cargo leptos watch   # 改动自动重编（热重载的 ws 端口未发布，仅自动编译）
```

**不用容器时**（本机已装 Nix）：`nix develop` 进入同一个环境。

`flake.nix` 里有两处不是装饰：

**wasm32 目标必须来自 rust-overlay。** nixpkgs 自带的 rustc 不带
`wasm32-unknown-unknown` 的 std，也没有 `pkgsCross.wasm32-unknown-unknown`；而
Leptos 的浏览器端产物正需要它。工具链只在 `mkToolchain` 里定义一次，devShell 与
打包共用同一份，避免两者悄悄分叉。

**wasm-bindgen 的补丁版本跟 nixpkgs 对齐。** cargo-leptos 会从 `Cargo.lock` 探测
`wasm-bindgen` 版本，再去找**同版本**的 CLI；不一致时它会试图联网自己下载，在 Nix
构建沙箱里必然失败。所以 `Cargo.toml` 里把 `wasm-bindgen` / `js-sys` / `web-sys`
锁成精确版本（三者由 crates.io 强制同进同退），并在注释里写明：升级前先确认目标
nixpkgs 里有对应的 CLI。

**release 构建需要 `#![recursion_limit = "256"]`**，见 `src/lib.rs` 与 `src/main.rs`
的注释：Leptos 的深层嵌套视图类型在 release 下的查询深度会超出默认上限，debug
构建不触发——也就是说，只有发布构建才会暴露，而我们恰恰只该用发布构建来验证。

**不装 Nix 也能编**：按上面的 rustup 三步装好工具链同样可以，只是失去了可复现性。

---

## 十、部署（NixOS）

本站在两种模式下运行，别混在一起：

| 模式 | 用途 | 怎么对外 | 状态 |
|---|---|---|---|
| **开发** | 本地写代码、浏览器里看效果 | 容器把应用端口映射到宿主机回环（`127.0.0.1:8080`），不对外开放 | 已就绪（`compose.yaml`） |
| **对外** | 部署到服务器后用域名访问 | NixOS 主机层绑域名与证书，应用仍只监回环 | 等服务器确定后执行 |

开发这条与 OpenClaw 的容器端做法同型：**把应用自己的端口映射到主机，且只映射到
回环**（`127.0.0.1:8080:8080`），同一局域网的其他设备也连不上。容器里应用绑
`0.0.0.0:8080`（否则映射进不来），但那只在容器网络里，外面看到的是回环。

对外这条走下面这个 NixOS 模块。

目标环境是 **NixOS**，因此部署不是一堆脚本，而是一个 **NixOS 模块**：系统配置里
import 它、填几个选项，`nixos-rebuild switch` 就完成部署与升级。

```nix
{
  inputs.grant-felix-homepage.url = "git+http://127.0.0.1:3000/Felix/grant-felix-homepage.git?ref=leptos";

  # 在 hosts/<你的主机>/default.nix 里
  imports = [ inputs.grant-felix-homepage.nixosModules.default ];

  services.grant-felix-homepage = {
    enable = true;
    siteUrl = "https://example.com";     # 必填
    nginx = {
      enable = true;
      domain = "example.com";
      enableACME = true;                 # 自动申请证书
    };
    backup.enable = true;
    environmentFile = "/run/secrets/grant-felix-homepage.env";
  };
}
```

模块负责的事：

| 部分 | 做法 |
|---|---|
| 服务 | systemd 单元，`Restart=always`，只读文件系统 + `ReadWritePaths` 收拢写权限 |
| 静态资源与内容 | 打包进 store（只读），通过 `LEPTOS_SITE_ROOT` / `CONTENT_DIR` 指定 |
| 可变状态 | 只有 `dataDir`：SQLite 库与上传图片 |
| 反向代理 | 可选 nginx vhost，含 ACME 与 HTTP→HTTPS 跳转 |
| 备份 | 可选 systemd 定时器，用 `sqlite3 .backup` 快照（WAL 下直接 `cp` 可能拿到缺事务的库） |
| Cookie | 检测到 HTTPS 时自动设 `COOKIE_SECURE=1` |

**秘密绝不进 Nix store。** store 是全库可读的，任何进 store 的值对所有本地用户可见，
而且会永久留在系统世代里。所以 `ADMIN_PASSWORD` 之类的秘密只经 `environmentFile`
传入（用 sops-nix / agenix 生成，或手工放 `/run` 下）。模块里也刻意没有
`environment` 选项来放秘密。

**内容更新 = 重新构建 + 切换世代。** Markdown 在构建时固定进 store，因此「线上跑的
到底是哪一版内容」是可追溯的，回滚也是 `nixos-rebuild switch --rollback`。

**上线必须确认的两项**（都有本机默认值，本机测试不会暴露问题）：

| 项 | 作用 | 不配的后果 |
|---|---|---|
| `siteUrl` | RSS 与 sitemap 里的绝对地址 | 产出里全是本机地址，订阅器与搜索引擎拿到错链接 |
| HTTPS（`nginx.enableACME`） | 自动开启 `COOKIE_SECURE` | 令牌可能在明文连接上被回传 |

若真的是「从别的发行版迁到 NixOS」，迁移本身（分区、引导、`nixos-anywhere`）不在
本模块范围内；模块只管**系统起来之后**这一站怎么跑。

---

## 十一、明确不做的事

这一节是为了防止范围悄悄扩大，也是给未来的自己看的边界。

- **不做富文本编辑器**：内容走 Markdown 提交，后台不碰内容
- **不做多语言**：只做中文，但在文案取值处预留 i18n 位置，不实现
- **不做第三方登录**：不接 OAuth，账号只有用户名 + 密码
- **暂时不做邮件**：不验邮箱、不发通知，`email` 字段仍可空且没人用。
  但这**不是永久决定**：账号找回的既定路线是「先关联邮箱 → 邮箱验证 → 靠邮箱重置
  密码」，做到那一步时邮件才会真正进来。在此之前**没有任何找回密码的路子**，
  忘了密码只能删账号重建
- **不做全文搜索**：内容量在数百篇以内，标签与归档够用
- **不把秘密入库**：令牌与密钥只放本地 `.env` 或环境变量，`.env` 已在 `.gitignore` 中
- **不做公开镜像**：仓库保持私有

---

## 十二、里程碑

| 阶段 | 内容 | 状态 |
|---|---|---|
| 1 | 骨架：SSR + 水合 + SQLite 打通 | 进行中 |
| 2 | 内容层：Markdown 载入与索引、博客与项目页 | 待办 |
| 3 | 主题系统：亮暗 + 自定义配色 + 背景图 | 待办 |
| 4 | 账号与会话：注册、登录、登出 | 待办 |
| 5 | 互动：评论与光遇评价 | 待办 |
| 6 | 后台：审核评论、管理用户、回复评价 | 待办 |
| 7 | SEO：RSS、sitemap、meta | 待办 |
| 8 | 部署：按第十节定稿后实施 | 待办 |

每个阶段的**完成判据与可复现验证命令**写在 `TODO.md`，以那里为准。

**表单的初值必须出现在服务端渲染的 HTML 里。** Leptos 的 `prop:value` 只在浏览器端
设 DOM 属性，SSR 输出里是空的——用户明明设过昵称、主色或背景图，首屏看到的却是空框；
后台的回复框更严重：管理员会以为原本没回复过，一保存就覆盖掉旧回复。所以凡是「要
显示已有值」的输入框，都用 `value` 属性（textarea 用子文本）写初值，并且**不绑成
响应式**，否则打字时会有东西跟光标抢。保存成功后组件因 `refetch` 重建，初值随之更新。
