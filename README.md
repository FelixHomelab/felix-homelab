# Felix-Homelab（Podman + Quadlet）

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

基于 **Podman** 的 rootless 工作站：创建一个名为 **Felix-Homelab** 的 Pod，
并在 Pod 内以多容器方式组合运行一整套常用自托管工具。

- **主站（Felix Homelab 社区站）**：Rust/Leptos 社区站：官方博客 / 项目 / 光遇，
  注册用户可投稿社区内容，另有账号、评论与后台；源码在本仓库 `site/`，经 Caddy
  挂在入口根路径；
- **Forgejo 系列**：Forgejo + PostgreSQL + Forgejo Actions Runner；
- **Homepage 控制台**：容器状态看板（`dash.localhost`）；
- **公网入口（可选）**：云服务器 frp 中转 + 云侧 Caddy HTTPS（示例域名 `grantfelix.top`）；
- **运维**：Caddy 统一入口、定时备份/异地同步/一键恢复、autoheal 自愈。

部署方式选用 **Podman Quadlet**（声明式 systemd 单元），无需 docker-compose，
开机自启、单元即配置，可复现性最高。

---

## 架构

```
                ┌──────────────────── Pod: Felix-Homelab (共享网络命名空间) ────────────────────────┐
                │                                                                                      │
  宿主端口      │   ┌──────────┐      ┌──────────────┐      ┌───────────────────────┐                │
  5729 ─────────┼──▶│  Caddy   │──┬──▶│  主站         │      │      PostgreSQL       │                │
  5730 ─────────┼──▶│ 统一入口 │  │   │ (Leptos)     │      │   (Forgejo 数据库)    │                │
  5731 ─────────┼──▶│  根路径   │  │   │  :8090       │      └───────────────────────┘                │
  5732 ─────────┼──▶│          │  │   └──────────────┘      ┌───────────────────────┐                │
  5733 ─────────┼──▶│          │  │                         │      Forgejo          │◀──┐            │
                │   └────┬─────┘  ├──▶ forgejo.localhost ─▶ │  Web :3000 SSH :2222  │   │            │
                │        │        │                         └───────────┬───────────┘   │            │
                │        ├──▶ dash.localhost ─▶ ┌──────────┐            │               │            │
                │        │                     │ Homepage │            ▼               │            │
                │        │                     │  :3001   │   ┌───────────────────────┐│            │
                │        │                     └──────────┘   │  Forgejo Actions      ││            │
                │        │                                    │  Runner  (CI/CD)      ││            │
                │        │                                    └───────────┬───────────┘│            │
                │        └── backup / autoheal（备份与自愈）               │            │            │
                └────────────────────────────────────────────────────────┼────────────┼────────────┘
                                                                          │ podman.sock │ 作业容器
                                                                          ▼             │
                                                                hosted rootless Podman ◀┘
```

多容器同处一个 Pod，**共享 network namespace**，因此彼此可通过 `127.0.0.1` 或
Pod hostname 直接互访；端口只在 Pod 级别发布一次。
`frpc`（公网中转）为可选组件，未配置时不会加入 Pod；见「公网访问（云服务器中转）」。

## 端口规划

所有端口**只绑定回环 `127.0.0.1`**，局域网/外部无法直连。
需要公网访问时，推荐走「公网访问（云服务器中转）」的 frp 方案（本地服务保持只监听回环），
而不是把端口改成 `0.0.0.0` 直接暴露。

| 宿主端口 | 容器 | 说明 |
| -------- | ---- | ---- |
| 5729     | Caddy (8080) | 统一入口：`/` 主站，`forgejo.localhost` Forgejo，`dash.localhost` 控制台，`cloud.localhost` Nextcloud |
| 5730     | Forgejo (3000) | Git Web / API 直连 |
| 5731     | Forgejo SSH (2222) | Git over SSH |
| 5732     | Homepage (3001) | 控制台直连 |
| 5733     | 主站 (8090) | 站点直连 |
| 5734     | Nextcloud (80) | 云盘直连 |
| 3000     | Forgejo (3000) | 仅供 host 网络的作业容器经 `127.0.0.1:3000` 访问 |

> **Host 网络的代价**：Runner 派发的作业容器使用 `container.network: host`，
> 因此作业内的进程能访问宿主机上仅监听回环的本地服务，并能绑定宿主端口。
> 这是为「作业容器能访问 Forgejo」付出的代价；作业容器并未挂载 `podman.sock`
> （runner 配置 `container.docker_host: "-"`），所以无法直接操作容器运行时。
> 若不接受该代价，可改用「自建 bridge 网络 + 在作业容器内解析宿主网关」的方案。

## 公网访问（云服务器中转）

本机端口只绑回环；需要公网访问时，推荐用一台云服务器做 **frp 中转 + 云侧 Caddy HTTPS**
（本仓库在阿里云 + `grantfelix.top` 实测通过）：

```
访客 ──HTTPS──▶ 云 Caddy :443（Let's Encrypt，Host 原样透传）
                     ▼
                frps :8443（强制 TLS + token）──隧道──▶ frpc（本 Pod 内）
                     ├─ 20080 → 本地 Caddy :8080（承载全部 HTTP 服务）
                     └─ 20022 → Forgejo SSH :2222
```

- 本机只发起**出站 TCP**，不改路由/DNS，不影响宿主机的 VPN / Clash / sing-box 等代理环境；
- 本地服务仍只监听回环，frps 的转发端口由云侧 Caddy 经回环调用，无需对外开放；
- 域名规划示例：主站 `example.com`，`forgejo.` / `cloud.` / `dash.` 子域共用同一隧道。

**云侧**（以 Fedora 为例，安全组放行 `80/443/8443/20022`）：

```toml
# /etc/frp/frps.toml（systemd 服务运行 frps -c 本文件）
bindPort = 8443
proxyBindAddr = "0.0.0.0"

auth.method = "token"
auth.token = "<随机 token>"

transport.tls.force = true
allowPorts = [{ start = 20000, end = 20100 }]
```

```caddyfile
# /etc/caddy/Caddyfile
{
	email admin@example.com
}

example.com, forgejo.example.com, cloud.example.com, dash.example.com {
	encode zstd gzip
	reverse_proxy 127.0.0.1:20080
}
```

**本地**：编辑 `~/.config/felix-homelab/frp/frpc.toml`
（模板在 `config/frpc.toml.example`，填 `serverAddr` 与 `auth.token`），
然后 `make install` —— 未填写前安装脚本不会链接并启动 frpc 单元。

切换真实域名时需同步修改：`config/Caddyfile` 的 host 匹配、`.env` 的
`FORGEJO__server__{DOMAIN,ROOT_URL,SSH_DOMAIN,SSH_PORT}`、`nextcloud.env` 的
`NEXTCLOUD_TRUSTED_DOMAINS` / `OVERWRITEHOST` / `OVERWRITEPROTOCOL`，
以及 homepage / site 单元中的 `HOMEPAGE_ALLOWED_HOSTS` / `SITE_URL`
（本仓库已按 `grantfelix.top` 配好）。

## 目录结构

```
.
├── .env.example                 # 环境变量模板（install.sh 复制为 .env 并生成随机密码）
├── LICENSE                      # MIT 开源许可
├── Makefile                     # 常用命令入口
├── quadlet/                     # Podman Quadlet 单元（唯一事实来源）
│   ├── felix-homelab.pod            # Pod、hostname、端口发布
│   ├── felix-homelab-db.container   # PostgreSQL
│   ├── felix-homelab-forgejo.container
│   ├── felix-homelab-site.container # 主站（Felix Homelab 社区站）
│   ├── felix-homelab-nextcloud.container # Nextcloud 云盘
│   ├── felix-homelab-runner.container
│   ├── felix-homelab-frpc.container     # 公网中转客户端（配置 frpc.toml 后启用）
│   ├── felix-homelab-homepage.container
│   ├── felix-homelab-caddy.container
│   ├── felix-homelab-backup.container   # 定时备份（按源执行）
│   ├── felix-homelab-autoheal.container # 健康自愈
│   └── felix-homelab-*.volume        # 命名数据卷
├── config/
│   ├── Caddyfile                # 反向代理入口，新增工具在此加路由
│   ├── registries.conf          # docker.io 镜像加速（国内网络）
│   ├── runner-labels.txt        # Runner 标签与作业镜像定义
│   ├── nextcloud.env.example    # Nextcloud 环境变量模板
│   ├── frpc.toml.example        # 公网中转 frpc 配置模板（可选）
│   ├── backup/backup.sh         # 备份脚本（容器内执行 Forgejo/主站）
│   ├── backup/backup-nextcloud.sh  # Nextcloud 备份（宿主机执行）
│   ├── backup/backup.conf.example  # 备份源与渠道配置模板
│   └── homepage/                # Homepage 控制台配置（首次安装植入）
├── site/                        # 主站源码（Felix Homelab 社区站）
├── containers/
│   ├── runner-image/Containerfile   # 修复 Podman 回归的派生镜像
│   └── site/Containerfile           # 主站构建镜像
└── scripts/
    ├── deploy.sh                # 交互式菜单入口（安装/备份/恢复/卸载）
    ├── install.sh               # 安装并启动
    ├── migrate-rename.sh        # 从旧命名 Felix-Workstation 迁移到 Felix-Homelab
    ├── register-runner.sh       # 注册/启动 Actions Runner（幂等）
    ├── build-runner-images.sh   # 构建修复版作业镜像
    ├── build-site.sh            # 构建主站镜像
    ├── backup-now.sh            # 立即备份一次
    ├── sync-backup.sh           # 备份异地同步（rclone/rsync）
    ├── restore-backup.sh        # 从备份恢复
    ├── ensure-nextcloud-db.sh   # 创建/同步 Nextcloud 数据库与角色
    ├── wait-for-forgejo.sh
    ├── create-admin.sh          # 可选：命令行创建管理员
    └── uninstall.sh             # 卸载（--purge 连数据一起删）
```

## 前置要求

- Linux + systemd 环境（本项目在 Fedora 上开发验证）
- Git 与 make（克隆仓库并执行 Makefile 目标）
- Podman ≥ 5（推荐 5.4+，本项目在 5.8 验证）
- systemd（用户服务 + `linger` 已开启：`loginctl enable-linger "$USER"`）
- 若无法直连 `docker.io`，`config/registries.conf` 已预置 daocloud 镜像加速（可自行修改）

## 快速开始

```bash
git clone https://github.com/FelixHomelab/felix-homelab.git
cd felix-homelab

make deploy         # 交互式菜单（推荐新手）
# 或直接：
make install        # 生成配置、拉取镜像、构建主站镜像、启动 Pod 并注册 Runner
```

> 首次会**编译主站（Rust/Leptos）**，需要几分钟到十几分钟；之后有镜像缓存。
> 站长密码（`ADMIN_PASSWORD`）在安装时随机生成并打印，也可在
> `~/.config/felix-homelab/.env` 查看/修改。

完成后：

| 服务          | 地址                                          |
| ------------- | --------------------------------------------- |
| 主站 | http://localhost:5729/（直连 http://localhost:5733/） |
| Forgejo Web   | http://localhost:5730/（或 http://forgejo.localhost:5729/） |
| Nextcloud     | http://cloud.localhost:5729/（直连 http://localhost:5734/） |
| Homepage 控制台 | http://dash.localhost:5729/（直连 http://localhost:5732/） |
| Forgejo SSH   | `ssh -p 5731 git@localhost`                   |

- 主站首次访问用 `.env` 里的 `ADMIN_USERNAME` / `ADMIN_PASSWORD` 登录后台 `/admin`
- Nextcloud 管理员用 `nextcloud.env` 里的 `NEXTCLOUD_ADMIN_USER` / `NEXTCLOUD_ADMIN_PASSWORD`
- Forgejo 首次打开 http://localhost:5730/ 注册第一个账号即管理员
  （已通过 `INSTALL_LOCK=true` 跳过网页安装向导，自动完成数据库迁移）。
- 若配置了公网中转：经云域名访问（如 `https://forgejo.example.com` 与
  `ssh -p 20022 git@forgejo.example.com`），见「公网访问（云服务器中转）」。

## 从旧命名迁移（Felix-Workstation → Felix-Homelab）

旧部署（pod / 单元 / 卷 / 配置目录名都是 `felix-workstation`）用一条命令迁移；
数据是**复制**而不是搬移，旧卷默认保留：

```bash
make migrate          # 等价于 scripts/migrate-rename.sh [--yes] [--prune-old]
make install          # 以新命名创建 Pod 并启动
make register         # 重新注册 Runner（旧 Runner 会显示离线，可在 Forgejo 后台删除）
```

- 配置目录 `~/.config/felix-workstation` → `~/.config/felix-homelab`；
  数据目录（含备份）`~/.local/share/felix-workstation` → `~/.local/share/felix-homelab`
- 数据卷 `felix-workstation-*` → `felix-homelab-*`（逐个复制，原卷保留）
- 旧备份归档 `felix-ws-*.tar.gz` 继续可被 `make restore` 与备份清理识别
- 确认新服务正常后，可用 `--prune-old` 或 `podman volume rm felix-workstation-…` 清理旧卷

> 全新安装无需迁移。安装脚本检测到旧配置目录且新目录不存在时，会提示先执行迁移。

## 主站（Felix Homelab 社区站）

Rust/Leptos 社区站（Leptos + Axum + SQLite）已**并入本仓库 `site/`**，作为
Felix-Homelab 的主站，经 Caddy 挂在入口根路径 `http://localhost:5729/`。

- **两类内容**：**官方内容**（博客、项目、光遇、静态页）是 `site/content/` 下的
  Markdown，随仓库版本化；**社区投稿**（文章 / 项目 / 光遇）由注册用户在站内发布，
  存 SQLite，发布即公开，作者可编辑删除、管理员可下架删除。两者在页面上有
  「官方内容 / 社区投稿」分区切换。
- **架构**：SSR + wasm（`cargo leptos`），单进程 + 单 SQLite 文件。
- **构建**：`containers/site/Containerfile` 两阶段构建（rust 编译 + debian-slim 运行），
  产物镜像 `localhost/felix-homelab-site:latest`；`make build-site`；
  `make install` 在镜像缺失时自动构建。
- **数据**：`felix-homelab-site-data` 卷挂到 `/app/data`（`site.db`、社区投稿与上传图片）。
- **路由**：`/` 主站；`/community` 社区入口；`forgejo.localhost` → Forgejo；
  `dash.localhost` → Homepage 控制台。
- **Pod 管理**：登录主站后 `http://localhost:5729/admin/pod` 可查看本 Pod 内容器状态并重启
  （与站点后台同款版式）。站点容器因此挂载了 rootless `podman.sock` 并以宿主用户运行——
  这是明确的权限扩大，但页面本身仅管理员可访问；不接受可停用该页并移除挂载。
- **备份**：随每小时/每日备份一起打包（`felix-homelab-site-<时间戳>.tar.gz`，用 SQLite `.backup`
  保证一致性），恢复时按相同时间戳与 Forgejo 归档配对还原。
- **更新官方内容**：改 `site/content/*.md` 后 `make build-site && make restart`；
  社区内容不需要重建，站内直接发布。

## Nextcloud（私有云盘）

- 官方 `nextcloud:apache` 镜像，复用同一个 PostgreSQL 容器中的独立库 `nextcloud`
  （角色/库由 `scripts/ensure-nextcloud-db.sh` 幂等创建，`make install` 会自动执行）
- 数据卷 `felix-homelab-nextcloud-data` 挂到 `/var/www/html`（含 config、apps、data）
- 入口：http://cloud.localhost:5729/ 或直连 http://localhost:5734/
- 管理员账号在 `~/.config/felix-homelab/nextcloud.env`
  （`NEXTCLOUD_ADMIN_USER` / `NEXTCLOUD_ADMIN_PASSWORD`，安装时随机生成并打印）
- **备份注意**：备份源可在后台「备份」页按需开关（Forgejo / 主站 / Nextcloud），
  Nextcloud 归档约 300MB（含数据卷与 `pg_dump`），默认保留 7 天。

## 后台运维（/admin）

登录主站后点顶栏「后台」，包含：

- **概览**：待审评论/评价、用户数、容器运行与健康汇总、快捷入口
- **评论 / 评价 / 用户**：审核与管理
- **Pod**：容器状态与一键重启
- **备份**：备份源开关与保留天数、备份渠道（rclone / rsync，可多个、可全关）、
  最近同步结果、备份归档列表、**立即备份 / 立即同步**，全部在页面上完成

## 常用命令

```bash
make deploy     # 交互式菜单（安装/状态/备份/恢复/卸载）
make status     # 查看 pod / 容器状态
make logs       # 跟踪 pod 日志
make restart    # 重启所有容器（含 Pod）
make stop       # 停止所有容器与 Pod
make start      # 启动所有容器
make backup     # 立即备份一次
make backup-list # 列出备份
make sync-backup # 备份异地同步（rclone/rsync）
make restore    # 从最新备份恢复（ARGS="<归档|latest> --yes" 非交互）
make register   # 重新注册并启动 Runner（幂等，含构建修复镜像）
make build-images # 仅重建修复版作业镜像
make build-site # 重建主站镜像
make migrate    # 从旧命名 Felix-Workstation 迁移到 Felix-Homelab
make uninstall  # 停止并移除单元，保留数据
make purge      # 连数据卷、配置一起删除（危险）
```

> 修改了 `quadlet/*`、`.env` 或 `Caddyfile` 后：
> - `.env` 改动：`make restart`
> - 单元/Caddyfile 改动：`make install`（会同步配置并重启相关服务）

## 配置

- **机密与开关**：`~/.config/felix-homelab/.env`（首次由 `.env.example` 生成，
  数据库密码随机生成）。命名规则 `FORGEJO__<section>__<KEY>` 会映射为 Forgejo 的
  `app.ini` 配置，详见
  [配置备忘单](https://forgejo.org/docs/latest/admin/config-cheat-sheet/)。
  数据库为 **PostgreSQL**（容器 `felix-homelab-db`，Pod 内 `127.0.0.1:5432`）。
- **反向代理**：`~/.config/felix-homelab/Caddyfile`（源文件在仓库中）。
- **首页**：`~/.config/felix-homelab/homepage/*.yaml`（源文件在 `config/homepage/`，
  首次安装植入后即归你所有，后续 `make install` 不会覆盖）。
- **Runner**：由 `scripts/register-runner.sh` 依据当前 Runner 镜像自动生成
  `runner-config.yml`，并注入 Forgejo 地址、uuid、token 与标签。
- **公网中转**：`~/.config/felix-homelab/frp/frpc.toml`（模板
  `config/frpc.toml.example`；云侧 frps/Caddy 配置见「公网访问（云服务器中转）」）。

## 关键设计说明

### 1. 作业容器的网络可达性

Runner 在 Pod 内，由它派发的**作业容器**却由宿主机 rootless Podman 在 Pod **之外**
创建。为了让 Runner 与作业容器能使用**同一个** Forgejo 地址，采用如下方案：

- Forgejo 在 Pod 内监听 `:3000`，Pod 同时发布 `3000:3000` 与 `5730:3000`；
- Runner 连接地址固定为 `http://127.0.0.1:3000/`：
  - Runner 在 Pod 内 → 直接命中 Forgejo；
- 作业容器通过 `container.network: host` 运行（`register-runner.sh` 注入）：
  - 作业在宿主机网络命名空间内 → `127.0.0.1:3000` 命中 Pod 发布到宿主机的端口。

这样 `actions/checkout` 等动作拿到的 `GITHUB_SERVER_URL` = `http://127.0.0.1:3000`，
在两类容器里都可达。

> 注意：`GITHUB_SERVER_URL` 由 Runner 依据连接地址强制设置，**无法**用
> `runner.envs` 覆盖；也不能依赖 Pod 内“发夹”访问 `host.containers.internal:5730`
> （在部分宿主网络环境下不可达）。因此选择 host 网络方案。

> 代价：作业容器与宿主机共享网络命名空间，作业监听的端口会直接占用宿主端口。
> 对本机个人 CI 场景可接受；若需隔离，可自行改用其它网络方案。

### 2. Podman archive-copy 回归的规避

Podman 5.8.7 / buildah 1.43.4 为修复 CVE 引入了沙箱化路径校验，会拒绝容器内的
**绝对符号链接**。于是 act/Runner 通过 Docker API 把 action 复制到
`/var/run/act/...` 时报错：

```
statat var/run/act/actions/<hash>/.eslintignore: path escapes from parent
```

上游 issue：<https://github.com/podman-container-tools/podman/issues/29805>

规避方式：基于官方镜像派生一层，把 `/var/run` 从绝对链接改为**相对链接**
（`../run`），相对链接不受该沙箱限制。`scripts/build-runner-images.sh` 会按
`runner-labels.txt` 自动构建这些本地修复版镜像（`localhost/felix-runner:*`），
`register-runner.sh` 会改用 `runner-labels.resolved.txt`。

```bash
make build-images   # 手动重建；make register / make install 会自动执行
```

待 Podman 修复后，设 `FELIX_NO_RUNNER_FIX=1` 即可跳过并直接使用原始镜像。
`full-latest` 因镜像自身 history 元数据缺陷暂无法重构建，已默认注释。

### 3. 数据库启动等待

`After=` / `Requires=` 只保证容器进程启动，不保证 PostgreSQL 已可接受连接。
Forgejo 容器的 systemd 单元增加了：

```ini
ExecStartPre=/usr/bin/podman wait --condition=healthy felix-homelab-db
```

即等数据库 `healthy` 后再启动 Forgejo，消除启动竞态。

> 迁移自 SQLite：先用空 PostgreSQL 启动 Forgejo 建好表结构，再用
> `pgloader`（`WITH data only, reset sequences`）把 SQLite 数据灌入，
> 最后切回 PostgreSQL 启动。社区验证可行，重复键告警可忽略。

### 4. Podman API 服务常驻

默认 `podman system service` 空闲 5 秒即退出，socket 文件随之被删除；而 Runner 与
Homepage 以 bind mount 方式挂载了该 socket，一旦 inode 重建就会失效。
安装脚本会写入 systemd drop-in，令其常驻：

```ini
# ~/.config/systemd/user/podman.service.d/10-felix-homelab.conf
[Service]
ExecStart=
ExecStart=/usr/bin/podman $LOGGING system service --time=0
```

### 5. Pod hostname 用小写

Pod 名称是 `Felix-Homelab`，但 Pod 内部 `HostName=felix-homelab`（小写）：
部分运行时（musl/Node）无法解析含大写的 `/etc/hosts` 名称，而 Homepage 会以
hostname 作为监听地址。

### 6. Caddy 与证书

本机 Caddy 只提供明文 HTTP（Pod 内 `:8080`，宿主回环 `5729`），不涉及本机证书。
公网访问时 HTTPS 由**云侧 Caddy** 终止（自动 Let's Encrypt，本仓库实测走 TLS-ALPN-01），
再经 frp 隧道回源到本机 Caddy；见「公网访问（云服务器中转）」。

## 数据与持久化

所有数据存放在 Podman 命名卷（`podman volume ls`）：

| 卷                               | 内容                 |
| -------------------------------- | -------------------- |
| `felix-homelab-db-data`      | PostgreSQL 数据（Forgejo + Nextcloud 两个库） |
| `felix-homelab-forgejo-data` | 仓库、附件、app.ini  |
| `felix-homelab-site-data`    | 主站 SQLite 与上传图片 |
| `felix-homelab-nextcloud-data` | Nextcloud 程序、配置与文件 |
| `felix-homelab-runner-data`  | Runner 注册与缓存    |
| `felix-homelab-caddy-*`      | Caddy 证书与配置     |

配置目录（含 `.env`、`Caddyfile`、`homepage/`、`runner-config.yml`、`runner.secret`）
位于 `~/.config/felix-homelab/`，其中机密文件不入库；备份源与渠道配置在
`sync/backup.conf`，同步状态在 `backup/sync.status`。

备份目录：`~/.local/share/felix-homelab/backups/`（归宿主用户所有，可直接拷贝到外部存储）。

## 备份与恢复

备份分**备份源**与**备份渠道**两层，全部配置集中在
`~/.config/felix-homelab/sync/backup.conf`（可在后台「备份」页图形化编辑）：

- **备份源**（各自独立开关）：Forgejo、主站、Nextcloud；
- **备份渠道**（异地，各自独立开关）：rclone / rsync 随意添加多个，也可全部关闭；
- **保留天数**：统一控制本机归档清理。

### 自动备份

调度由**宿主机 systemd user timer** 负责（不在容器里跑 cron）：

```
felix-homelab-backup.timer  (OnCalendar=03:00, Persistent=true)
  └─ felix-homelab-backup-run.service
       ├─ podman exec felix-homelab-backup sh /usr/local/bin/backup.sh once   # Forgejo + 主站
       └─ ~/.config/felix-homelab/backup/backup-nextcloud.sh                 # Nextcloud
```

- `Persistent=true`：机器在 03:00 关机/休眠时，**开机后立刻补跑**错过的备份；
- 备份容器常驻空闲，实际动作由 timer 触发（也便于手工 `make backup`）；
- 归档与校验（默认保留 **7 天**，`KEEP_DAYS` 可调）：
  - `felix-homelab-dump-<时间>.tar.gz`：`forgejo dump`（数据库 + 仓库 + 附件 + LFS + 配置），
    必须含 `app.ini`、`forgejo-db.sql`、`repos/`；
  - `felix-homelab-site-<时间>.tar.gz`：主站 `site.db`（SQLite `.backup`）与上传图片，
    必须含 `site.db`；
  - `felix-homelab-nextcloud-<时间>.tar.gz`：`pg_dump`（自定义格式）与数据卷打包，必须含
    `nextcloud-db.dump`；
  - 任一校验不通过即删除该归档，避免“假绿灯”。
- Nextcloud 备份放在宿主机侧执行（备份容器里没有 `pg_dump`，也不便直接读数据卷）：
  先用 db 容器 `pg_dump nextcloud`，再用一次性 alpine 容器打包 Nextcloud 数据卷。

```bash
make backup        # 立即备份一次（走上面同一个 systemd 服务，含已启用的源）
make backup-list   # 列出备份
systemctl --user list-timers felix-homelab-backup.timer   # 查看下次触发
```

> rootless 说明：备份容器以 root 运行（映射为宿主用户）负责落盘并修正属主，
> `forgejo dump` 则通过 `s6-setuidgid` 降权到 git 执行。

### 异地容灾（多渠道）

每日 04:00 由 `felix-homelab-backup-sync.timer` 把备份目录同步到**所有已启用**的渠道；
每个渠道可单独开关，也可以全部关闭（只保留本机备份）。渠道配置示例：

```
CHANNEL_1_NAME="WebDAV 网盘"
CHANNEL_1_KIND="rclone"
CHANNEL_1_TARGET="webdav:我的网盘/felix-backups"
CHANNEL_1_ENABLE=1
```

- **rclone**（推荐，覆盖 40+ 后端：Cloudflare R2 / 阿里云 OSS / S3 / WebDAV /
  OneDrive / Google Drive…）：凭据放 `~/.config/felix-homelab/rclone/rclone.conf`，
  脚本用官方 rclone 容器执行；
- **rsync**：目标填外置硬盘挂载点 / NAS / `user@host:path`。

```bash
make sync-backup                  # 同步所有已启用渠道
make sync-backup ARGS=--dry-run   # 预演
# 后台「备份」页 →「立即同步」+「刷新状态」可查看每个渠道的结果
```

每次同步的结果写入 `~/.config/felix-homelab/backup/sync.status`，后台页面直接展示。

> 云端私有仓库（GitHub/Gitee）方案注意：单文件 100MB 限制，而归档通常 >100MB，
> 不适合直接托管；推荐对象存储或 WebDAV。

### 从备份恢复

```bash
make restore                 # 用最新 Forgejo 归档（会交互确认）
make restore ARGS="<归档路径|latest> --yes"   # 非交互
```

恢复流程（`scripts/restore-backup.sh`）：

1. 先自动做一次当前状态的安全备份（可用 `--skip-safety` 跳过）；
2. 校验归档 → 停止 Forgejo / Runner / 备份容器；
3. 重建 PostgreSQL 数据库并导入归档中的 `forgejo-db.sql`；
4. 还原数据卷中的 `data/` 与 `repos/`（保留 SSH 主机密钥 `/data/ssh`）；
5. 若存在**同时间戳**的主站归档，则一并还原 `site.db` 与上传图片；
6. 启动服务、等待健康、重新注册 Runner，并打印用户/仓库/Actions 数量。

> 已实测：完整恢复 916 条 INSERT、0 错误，**1 用户 / 10 仓库 / 35 次 Actions**，
> 主站 `site.db`（含站长账号）与上传图片一并还原。

Nextcloud 归档需要手工恢复（脚本暂未自动化）：

```bash
# 1) 停服务
systemctl --user stop felix-homelab-nextcloud.service
# 2) 恢复数据卷
podman volume rm felix-homelab-nextcloud-data
podman volume create felix-homelab-nextcloud-data
podman run --rm -v felix-homelab-nextcloud-data:/data:Z \
  -v ~/.local/share/felix-homelab/backups:/backup:ro \
  docker.io/library/alpine:3.20 \
  tar -xzf /backup/felix-homelab-nextcloud-<时间>.tar.gz -C /data nextcloud-files.tar.gz
podman run --rm -v felix-homelab-nextcloud-data:/data:Z \
  docker.io/library/alpine:3.20 \
  tar -xzf /data/nextcloud-files.tar.gz -C /data && rm -f /data/nextcloud-files.tar.gz
# 3) 恢复数据库
systemctl --user start felix-homelab-db.service
podman exec felix-homelab-db psql -U forgejo -d postgres \
  -c 'DROP DATABASE IF EXISTS nextcloud;' -c 'CREATE DATABASE nextcloud OWNER forgejo;'
podman run --rm -v ~/.local/share/felix-homelab/backups:/backup:ro \
  docker.io/library/alpine:3.20 \
  tar -xzf /backup/felix-homelab-nextcloud-<时间>.tar.gz -C /tmp nextcloud-db.dump
podman cp /tmp/nextcloud-db.dump felix-homelab-db:/tmp/  # 视宿主机路径调整
podman exec felix-homelab-db pg_restore -U forgejo -d nextcloud --no-owner /tmp/nextcloud-db.dump
# 4) 启动
systemctl --user start felix-homelab-nextcloud.service
```

### 迁移到别的机器

把 `~/.local/share/felix-homelab/backups/` 与仓库一起拷走，在新机器 `make install`
后用 `make restore ARGS="<归档> --yes"` 即可。备份归档含全部密钥与配置，
注意离线妥善保管。

## 崩溃自修复（保活）

| 故障情形 | 自动恢复 | 靠什么 |
| -------- | -------- | ------ |
| 机器重启 | ✓ | linger + 全部单元挂 `default.target.wants` |
| Forgejo 进程崩溃 | ✓ | 镜像内 s6 进程监督器 |
| 容器整体退出 | ✓ | systemd `Restart=always` |
| podman 服务重启 | ✓ | 单元依赖 + `Restart=always` |
| 进程卡死但未退出 | ✓ | `felix-homelab-autoheal` 监视健康检查并重启 |
| 磁盘写满 / 数据损坏 | ✗ | 无法自动恢复，靠备份（见上节） |

`autoheal` 与旧方案一致：监视带 `Label=autoheal=true` 的容器，发现 **unhealthy** 就重启。
给 db / forgejo / caddy / homepage / runner 都打了该标签；`AUTOHEAL_START_PERIOD=120`
避免容器刚启动就被误判。它挂载了 rootless `podman.sock`（权限扩大），
如不需要可停用 `felix-homelab-autoheal.service` 并移除各单元的 `Label=`。

> 已实测：构造一个健康检查恒失败的容器，autoheal 在数秒内将其重启（`StartedAt` 变化）。

## Forgejo Actions Runner

- Runner 以容器内 `root` 运行；在 rootless 下该用户映射为**宿主机当前用户**，
  因此可以访问挂载进来的 rootless `podman.sock`。
- 作业容器通过 `DOCKER_HOST=unix:///var/run/docker.sock` 由宿主的 rootless Podman 派发。
- **标签与镜像**定义在 `config/runner-labels.txt`（首次安装植入到
  `~/.config/felix-homelab/runner-labels.txt`，改完执行 `make register`）。
  默认追求“能力完整”而非轻量：

  | 标签 | 镜像（原始） | 说明 |
  | ---- | ---- | ---- |
  | `ubuntu-latest` | `ghcr.io/catthehacker/ubuntu:js-24.04` | GitHub runner 等价 + Node 24/JS 工具链 |
  | `ubuntu-24.04` / `ubuntu-22.04` | `ghcr.io/catthehacker/ubuntu:act-24.04` / `act-22.04` | 工具齐全的通用镜像 |
  | `rust` / `go` / `java` / `dotnet` | `catthehacker` 对应语言增强版 | 语言专用 |
  | `node24` | `docker.io/library/node:24-bookworm` | 官方 Node 24 LTS |
  | `debian` / `alpine` | 官方镜像 | 通用 / 轻量诊断 |
  | `full` | `ghcr.io/catthehacker/ubuntu:full-latest` | 默认注释（见“archive-copy 回归”） |

  实际运行时使用 `scripts/build-runner-images.sh` 生成的本地修复版
  `localhost/felix-runner:*`，写入 `runner-labels.resolved.txt`。

- 采用 Forgejo 官方**离线注册**（`forgejo-cli actions register`），复用一个 40 位
  十六进制共享密钥，重复执行不会产生重复 Runner。
  - 若 Forgejo 数据库被重置：重新执行 `make register` 即可。
  - 若升级 Runner 镜像：`make register` 会基于新镜像重新生成配置。

## 扩展：新增一个工具容器

以添加一个 Web 工具（监听 `:8888`）为例：

1. 新增 `quadlet/felix-homelab-<tool>.container`：

   ```ini
   [Unit]
   Description=Felix-Homelab: <tool>
   After=felix-homelab-pod.service
   Requires=felix-homelab-pod.service

   [Container]
   ContainerName=felix-homelab-<tool>
   Image=docker.io/example/tool:1
   Pod=felix-homelab.pod
   Volume=felix-homelab-<tool>-data.volume:/data

   [Service]
   Restart=always

   [Install]
   WantedBy=default.target
   ```

2. 若需持久化，新增对应的 `*.volume` 文件。
3. 在 `config/Caddyfile` 中追加路由（例如 `/tool/*` → `127.0.0.1:8888`）。
4. 在 `.pod` 中为需要的端口追加 `PublishPort`。
5. 在 `config/homepage/services.yaml` 添加卡片（可选）。
6. `make install`（重新软链单元、reload、启动）。

> 端口发布只能写在 `.pod` 文件里；容器之间通过 `127.0.0.1:<port>` 互访
> （若某容器监听 Pod hostname 而非回环，请用 `felix-homelab:<port>`）。

## 为什么用 Quadlet

- **声明式**：单元文件即基础设施，进 Git 后可完整复现。
- **systemd 原生**：开机自启、依赖编排（`After`/`Requires`）、失败重启。
- **rootless 友好**：整个 Pod 以普通用户运行，无需 root 守护进程。

## 开源许可

本项目采用 [MIT License](LICENSE) 开源，欢迎 fork 自用、提交 Issue 与 PR。

> 注意：`site/content/` 下的个人博文、图片等内容版权归作者所有，不在 MIT 授权范围内。

