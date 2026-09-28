---
title: 关于
updated: 2026-09-27
---

**Felix Homelab** 是一个自托管的社区站点：官方内容由站长与管理员维护，社区内容由
注册用户投稿，两类内容分区展示。

## 这里有什么

- **官方内容**：博客、项目与光遇记录，以 Markdown 存在仓库里，改内容就是提交；
- **社区投稿**：注册后即可发布文章、项目与光遇内容，发布即公开，违规会被下架；
- **互动**：登录后可评论（先审后显示），也可以给光遇代跑打分。

## 这个站是怎么搭的

用 **Leptos + Axum + SQLite** 写的：组件与响应式都在 Rust 里，服务端渲染保证首屏与
SEO，浏览器只水合确实需要交互的部分。数据库只存用户产生的东西——账号、会话、社区
投稿、评论、评价，以及每个人自己的外观偏好。

整站跑在 **Felix Homelab**：一个基于 Podman + Quadlet 的 rootless 自托管工作站
（Forgejo / Caddy / 定时备份与自愈），代码与部署细节见
[FelixHomelab/felix-homelab](https://github.com/FelixHomelab/felix-homelab)。

## 技术取舍

设计上的选择与「明确不做什么」都记在仓库的 `DESIGN.md` 里；踩过的坑记在 `TODO.md`。
