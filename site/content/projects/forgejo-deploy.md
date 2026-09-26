---
name: forgejo-deploy
kind: private
author: Felix
summary: 本机 Forgejo 离线部署编排与运维文档。
stack:
  - Shell
weight: 7
---

把 [Forgejo](https://forgejo.org/) 部署到本机 Docker 上，作为**个人自用的离线 Git 托管
平台**：不对外暴露，不依赖任何外部服务，断网也能正常使用。

| 项目 | 值 |
| --- | --- |
| Forgejo 版本 | 16.0.5 |
| 数据库 | SQLite（单文件） |
| 网页访问 | `http://127.0.0.1:3000/` |
| Git over SSH | `ssh://git@127.0.0.1:2222/` |
| 监听范围 | 仅回环，局域网其他设备连不上 |
| 定时备份 | 每天 03:00 `forgejo dump` 到 `./backups/`，保留 7 天 |
| 开机自启 | 容器 `restart: always`，重启后自动恢复 |

**为什么是离线**：一开始试过公网访问（Cloudflare Tunnel + 自有域名），实测本机所处的
热点 / 校园网会封锁 UDP，隧道依赖的 QUIC 协议间歇性失效，公网时通时不通，不适合作为
日常依赖。于是重新定位为纯本机离线平台，公网方案的配置与经验归档在文档第十节。
