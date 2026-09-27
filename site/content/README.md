# 内容目录说明

这个目录存放**官方内容**（随仓库版本化）。社区用户的投稿存在数据库里，通过站内
`/community/new` 发布，不放在这里。

```
content/
├── posts/      文章（/blog/:slug）
├── projects/   项目（/projects/:slug）
├── sky/        光遇（/sky/:category）
└── pages/      静态页（about / contact）
```

文件名主干（去掉 `.md`）就是 slug，即页面地址里的那段。所有 front matter 字段都是
可选的，缺省值见下表。

## posts

```markdown
---
title: 文章标题
date: 2026-09-27
summary: 列表与 SEO 用的一句话摘要
author: Felix
tags: [自托管, Podman]
draft: false
---
正文用 Markdown。
```

## projects

```markdown
---
name: 项目名
kind: open        # open / private / team
summary: 一句话介绍
author: Felix
stack: [Rust, Leptos]
repo: https://example.com/repo
demo: https://example.com
weight: 0         # 越小越靠前
---
```

## sky

```markdown
---
title: 标题
date: 2026-09-27
category: gameplay    # gameplay / gallery
summary: 摘要
cover: /uploads/1/xxx.webp
author: Felix
---
```

## pages

```markdown
---
title: 页面标题
updated: 2026-09-27
---
```

> 注意：`author` 写站点账号的用户名（不区分大小写），解析得到就会链到其公开主页；
> 解析不到只显示名字。
