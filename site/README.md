# Grant Felix Homepage

> 用 Leptos + Axum + SQLite 写的个人主页：博客、项目、光遇子站与个人介绍，
> 外加一套账号与评论体系。

## 这是什么

一个自持的个人站点。对外展示博客文章、开源与私有项目、光遇的攻略与代跑服务；
对内提供注册登录、评论与代跑评价，以及只有站长能进的后台。

它解决的是「个人内容散落在静态托管与各平台、没有自己的评论与账号」这件事——
整站只有一个 Rust 进程与一个 SQLite 文件，内容以 Markdown 存在仓库里，改内容
就是提交。

## 能力概览

- **内容**：博客（列表 / 正文 / 标签）、项目展示（开源 / 私有 / 团队）、光遇攻略与画廊
- **互动**：注册登录、评论（先审后显示）、光遇代跑评价
- **个性化**：亮暗自动或手动切换、自定义配色、自定义背景图（偏好存在账号里）
- **后台**：审核评论、管理用户、回复评价
- **运行**：`cargo leptos` 一条命令构建，产物是 wasm 包 + 服务端二进制

## 当前状态

**功能已全部实现并逐项验证**：博客、项目、光遇（攻略 / 画廊 / 代跑评价）、账号与
评论、关于与联系、后台审核、SEO，以及主题与账号级个性化。

部署侧已备好 `flake.nix`（开发环境）与 `nix/module.nix`（NixOS 部署），
`nix build` 的产物**实跑验证过**：健康检查连库正常、15 条路由全部 200。

尚未完成的是**内容**：`content/` 下是示例文章与项目条目，等着替换成真的。
各阶段的验证命令与踩过的坑见 [`TODO.md`](TODO.md)。

## 技术栈

| 层 | 选型 |
|---|---|
| 视图 | Leptos 0.8（SSR + 水合，无手写 JS） |
| 服务端 | Axum 0.8 |
| 数据库 | SQLite + sqlx 0.8（只存互动数据） |
| 构建 | cargo-leptos 0.3 |
| 打包与部署 | Nix flake + NixOS 模块 |
| 样式 | 原生 CSS + 设计令牌 |

选型理由见 [`DESIGN.md`](DESIGN.md) 第二节。

## 本地运行

开发环境是 Docker 上的 NixOS 容器，工具链由 `flake.nix` 声明——不需要「照着文档
装一遍」：

```bash
docker compose up -d      # 起来后浏览器打开 http://127.0.0.1:8080
docker compose logs -f    # 看构建日志（首次要下工具链并编译，几分钟）
docker compose down       # 停止；数据在命名卷里，下次还在（down -v 才会清空）
```

数据（账号、评论、上传的图片）都落在命名卷里，重建容器不丢。要在本地进后台，先在
`.env` 里给出站长账号（该文件已被 git 忽略）：

```bash
echo 'ADMIN_USERNAME=你的用户名' >> .env
echo 'ADMIN_PASSWORD=至少8位' >> .env
docker compose up -d
```

在自己的 NixOS 机器上也可以直接 `nix develop` 进同一个环境：

```bash
nix develop
cargo leptos watch      # 开发，改动自动重编
cargo leptos serve      # 起服务
cargo leptos build --release
```

站点端口是 **8080**，不是 3000——3000 被 Forgejo 占着。

验证是否跑通：

```bash
curl -s http://127.0.0.1:8080/healthz
# 期望：{"sqlite":"3.46.0","status":"ok"}
```

不用 Nix 也可以，按 [`DESIGN.md`](DESIGN.md) 第九节装 rustup + wasm 目标 +
cargo-leptos 即可——代价是失去可复现性。

## 部署

目标环境是 NixOS，部署是一个 NixOS 模块而不是一堆脚本：在系统配置里 import
`nixosModules.default`，填 `siteUrl`、`nginx.domain` 等选项，`nixos-rebuild switch`
即完成。选项与说明见 [`DESIGN.md`](DESIGN.md) 第十节。

## 文档分工

同一件事只写一遍，各自的分工如下。

| 文档 | 负责什么 |
|---|---|
| `README.md` | 介绍：是什么、能做什么、当前状态 |
| `DESIGN.md` | 设计：为什么这样切、边界、明确不做什么 |
| `TODO.md` | 实施：拆解、schema、验证命令、完成判据 |
| `content/` | 内容本身（Markdown，随仓库版本化） |

## 仓库

主干托管在本地 Forgejo 的**私有**仓库 `Felix/grant-felix-homepage`。当前实现位于
`leptos` 分支；`rust`（旧的 Axum + Tera 骨架）与 `hexo`（更早的静态站）两个分支
保留作参考，不再维护。

## 许可证

MIT
