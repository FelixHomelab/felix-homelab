---
name: Grant Felix Homepage
kind: private
author: Felix
summary: 本站本身——用 Leptos + Axum + SQLite 写的个人主页，内容以 Markdown 随仓库维护。
stack:
  - Rust
  - Leptos
  - Axum
  - SQLite
weight: 8
---

这个站是我自己的主页，也是这套技术选型的试验田。

## 做了什么

对外是博客、项目展示与光遇子站；对内是一套只有自己需要登录的后台，用来审核评论
与回复评价。整站只有一个 Rust 进程与一个 SQLite 文件。

## 几个刻意的选择

**内容不进数据库。** 文章与项目介绍都是仓库里的 Markdown，改内容等于提交。数据库
只存用户产生的东西：账号、会话、评论、评价和主题偏好。

**服务端渲染优先。** 用 Leptos 的 SSR 加水合，首屏与 SEO 不依赖浏览器端脚本；
浏览器只接管确实需要交互的部分。

**主题在服务端就定下来。** 亮暗与自定义配色写在服务端渲染的 HTML 里，避免首屏
闪一下再变色。

代码托管在本地 Forgejo 的私有仓库，不对外。
