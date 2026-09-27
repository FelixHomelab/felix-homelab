# Felix Homelab 社区站

> 用 Leptos + Axum + SQLite 写的自托管社区站：官方内容（博客、项目、光遇）随仓库
> 版本化，社区内容由注册用户直接发布；外加账号、评论与后台。

## 这是什么

一个自持的社区站点。**官方内容**由站长与管理员维护，以 Markdown 存在仓库里，
改内容就是提交；**社区投稿**由注册用户发布，存 SQLite，发布即公开、违规会被下架。
两类内容分区展示（「官方内容 / 社区投稿」切换条），互不混淆。

它解决的是「个人内容散落在静态托管与各平台、没有自己的评论与账号」这件事——
整站只有一个 Rust 进程与一个 SQLite 文件，官方内容随 Git 版本化，社区内容
随账号体系管理。

## 能力概览

- **官方内容**：博客（列表 / 正文 / 标签）、项目展示（开源 / 私有 / 团队）、光遇攻略与画廊
- **社区投稿**：注册用户发布文章 / 项目 / 光遇内容，作者可编辑删除，管理员可下架删除
- **互动**：注册登录、评论（先审后显示，官方与社区内容共用）、光遇代跑评价
- **个性化**：亮暗自动或手动切换、自定义配色、自定义背景图（偏好存在账号里）
- **后台**：审核评论、社区内容管理、管理用户、回复评价
- **运行**：`cargo leptos` 一条命令构建，产物是 wasm 包 + 服务端二进制

## 当前状态

**功能已全部实现并逐项验证**：官方内容、社区投稿、账号与评论、光遇、后台、
SEO，以及主题与账号级个性化。

部署侧已备好 `flake.nix`（开发环境）与 `nix/module.nix`（NixOS 部署）。
本仓库的主站运行在 Felix Homelab 工作站上（Podman 容器 + Caddy），见仓库根
`README.md`。

`content/` 下的示例内容已清空，目录结构与格式说明见
[`content/README.md`](content/README.md)；社区内容通过站内 `/community/new` 发布。

## 技术栈

| 层 | 选型 |
|---|---|
| 视图 | Leptos 0.8（SSR + 水合，无手写 JS） |
| 服务端 | Axum 0.8 |
| 数据库 | SQLite + sqlx 0.8（账号、社区投稿、评论等用户数据） |
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

数据（账号、社区投稿、评论、上传的图片）都落在命名卷里，重建容器不丢。要在本地进
后台，先在 `.env` 里给出站长账号（该文件已被 git 忽略）：

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
| `content/` | 官方内容本身（Markdown，随仓库版本化） |

## 仓库

开源在 GitHub 的 [`FelixHomelab/felix-homelab`](https://github.com/FelixHomelab/felix-homelab)
的 `site/` 目录；本地 Forgejo 上的同名仓库作为镜像存在，可继续用于内网构建。

## 许可证

MIT
