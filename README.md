# Felix-Homelab（Podman + Quadlet）

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

基于 **Podman** 的 rootless 工作站：创建一个名为 **Felix-Homelab** 的 Pod，
并在 Pod 内以多容器方式组合运行一整套常用自托管工具。

- **主站（Felix Homelab 社区站）**：Rust/Leptos 社区站：官方博客 / 项目 / 光遇，
  注册用户可投稿社区内容，另有账号、评论与后台；源码在本仓库 `site/`，经 Caddy
  挂在入口根路径；
- **Forgejo 系列**：Forgejo + PostgreSQL + Forgejo Actions Runner；
- **OpenCloud**：Go 单栈私有云盘（文件同步/分享，替代 Nextcloud），经 Caddy 挂 `cloud.<域名>`；
- **Homepage 控制台**：容器状态看板（`dash.localhost`）；
- **AI Agent（多租户 P1 试点）**：每个授权账号一个独立 Agent 容器
  （模板镜像 + 独立数据卷 + `<用户名>.agent.<域名>` 子域，后台开通/计费授权）；
- **公网入口（可选）**：云服务器 frp 中转 + 云侧 Caddy HTTPS（示例域名 `wraindrock.com`）；
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
| 5729     | Caddy (8080) | 统一入口：`/` 主站，`forgejo.localhost` Forgejo，`dash.localhost` 控制台 |
| 5730     | Forgejo (3000) | Git Web / API 直连 |
| 5731     | Forgejo SSH (2222) | Git over SSH |
| 5732     | Homepage (3001) | 控制台直连 |
| 5733     | 主站 (8090) | 站点直连 |
| 3000     | Forgejo (3000) | 仅供 host 网络的作业容器经 `127.0.0.1:3000` 访问 |
| 5735     | 主站 (8090) | 仅供 Agent 网关 forward_auth 调用 |
| 8443     | Kanidm (8443) | 统一账户 IdP 直连（仅回环；公网经 Tunnel） |
| 8085     | STT (8080) | 语音转文字（仅回环；站点经 /api/stt 代理） |
| 5740     | Agent 网关 (Caddy) | 仅宿主回环；<子域>.agent.<域名> 统一入口 |
| 20001+   | AI Agent（动态分配） | 各 Agent 发布到宿主回环；经 5740 网关鉴权后访问 |

> **OpenCloud** 不发布宿主端口：常驻 Pod 内网 `9200`，只经 Caddy
> （`cloud.localhost` / `opencloud.wraindrock.com`）访问。

> **Kanidm** 只发布到宿主回环 `127.0.0.1:8443`，公网经 Cloudflare Tunnel
> （`id.wraindrock.com` → `https://localhost:8443`，需开 No TLS Verify）。

> **Host 网络的代价**：Runner 派发的作业容器使用 `container.network: host`，
> 因此作业内的进程能访问宿主机上仅监听回环的本地服务，并能绑定宿主端口。
> 这是为「作业容器能访问 Forgejo」付出的代价；作业容器并未挂载 `podman.sock`
> （runner 配置 `container.docker_host: "-"`），所以无法直接操作容器运行时。
> 若不接受该代价，可改用「自建 bridge 网络 + 在作业容器内解析宿主网关」的方案。

## 公网访问

### Cloudflare Tunnel（推荐）

在 Cloudflare Zero Trust → Networks → Tunnels 建一个隧道，把**连接器令牌**填进
`~/.config/felix-homelab/.env` 的 `TUNNEL_TOKEN`，再 `make install`：仓库内的
`felix-homelab-cloudflared`（host 网络容器）会自动接入。不需要公网 IP、不需要开
入站端口，也不依赖云服务器；HTTP 服务与 Agent 都经隧道回源。

Public Hostnames（Zero Trust → 该隧道 → Public Hostname）：

| 主机名 | 服务 | 说明 |
| --- | --- | --- |
| `www.wraindrock.com` | `http://localhost:5729` | 主站 |
| `forgejo.wraindrock.com` | `http://localhost:5729` | Forgejo |
| `dash.wraindrock.com` | `http://localhost:5729` | Homepage 控制台 |
| `opencloud.wraindrock.com` | `http://localhost:5729` | OpenCloud |
| `wraindrock.com` | `http://localhost:5729` | 顶点 301 → www（本地 Caddy 处理）|
| `id.wraindrock.com` | `https://localhost:8443` | Kanidm 统一账户（**No TLS Verify**）|
| `*.wraindrock.com` | `http://localhost:5740` | Agent（18 位随机码一级子域）|

> **Tunnel 的 Public Hostname 按列表顺序匹配，通配必须永远排在最后**：
> 新加的具体主机名若落在 `*.wraindrock.com` 之后会被通配截胡
> （表现为 5740 网关回空 200）。加错时的修法：删掉 `*` 再重新添加一次，它就会排到末尾。
>
> 顶点留给邮箱；Agent 域名是**一级子域**（`<18位随机码>.wraindrock.com`），
> Cloudflare 免费版 Universal SSL 正好覆盖，无需付费证书。
>
> **Forgejo SSH 不走 Tunnel**（免费版不转发任意 TCP）：在 Cloudflare 另加一条
> **灰云** A 记录 `ssh` → 云服务器 IP，并把 `FORGEJO__server__SSH_DOMAIN`
> 指向 `ssh.<域名>`；克隆地址即 `ssh://git@ssh.<域名>:20022/...`。

### 备选：云服务器 frp 中转

不使用 Tunnel 时，可用一台云服务器做 **frp 中转 + 云侧 Caddy HTTPS**
（本仓库早期在阿里云 + 域名实测通过）：

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
`FORGEJO__server__{DOMAIN,ROOT_URL,SSH_DOMAIN,SSH_PORT}`，
以及 homepage / site 单元中的 `HOMEPAGE_ALLOWED_HOSTS` / `SITE_URL`
（本仓库已按 `wraindrock.com` 配好）。

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
│   ├── frpc.toml.example        # 公网中转 frpc 配置模板（可选）
│   ├── cloud/Caddyfile.example  # 云侧 Caddy 模板（frp 中转 + Agent 子域按需 TLS）
│   ├── backup/backup.sh         # 备份脚本（容器内执行 Forgejo/主站）
│   ├── backup/backup.conf.example  # 备份源与渠道配置模板
│   └── homepage/                # Homepage 控制台配置（首次安装植入）
├── site/                        # 主站源码（Felix Homelab 社区站）
├── containers/
│   ├── agent-opencode/Containerfile # 多租户 Agent 模板镜像（OpenCode + 工具链）
│   ├── agent-dsh/                   # 多租户 Agent 模板镜像（DeepSeek Harness）
│   │   ├── Containerfile
│   │   └── dsh-entry.sh
│   ├── runner-image/Containerfile   # 修复 Podman 回归的派生镜像
│   └── site/Containerfile           # 主站构建镜像
└── scripts/
    ├── deploy.sh                # 交互式菜单入口（安装/备份/恢复/卸载）
    ├── install.sh               # 安装并启动
    ├── agent-ctl.sh             # 多租户 Agent 编排（建容器/路由/保活）
    ├── build-agent-image.sh     # 构建 Agent 模板镜像
    ├── migrate-rename.sh        # 从旧命名 Felix-Workstation 迁移到 Felix-Homelab
    ├── register-runner.sh       # 注册/启动 Actions Runner（幂等）
    ├── build-runner-images.sh   # 构建修复版作业镜像
    ├── build-site.sh            # 构建主站镜像
    ├── backup-now.sh            # 立即备份一次
    ├── sync-backup.sh           # 备份异地同步（rclone/rsync）
    ├── restore-backup.sh        # 从备份恢复
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
| Homepage 控制台 | http://dash.localhost:5729/（直连 http://localhost:5732/） |
| Forgejo SSH   | `ssh -p 5731 git@localhost`                   |
| OpenCloud     | http://cloud.localhost:5729/（公网 https://opencloud.wraindrock.com/） |

- 主站首次访问用 `.env` 里的 `ADMIN_USERNAME` / `ADMIN_PASSWORD` 登录后台 `/admin`
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

## OpenCloud（文件同步/分享）

Go 写的单栈私有云盘（OpenCloud，替代早期的 Nextcloud）：一个容器内置 Web、WebDAV 与
OIDC，经 Caddy 挂到 `cloud.localhost` / `opencloud.wraindrock.com`（Pod 内明文 `9200`，
TLS 由主 Caddy 终止）。

- **镜像**：`opencloudeu/opencloud:7.2.4`（固定版本；升级改
  `quadlet/felix-homelab-opencloud.container` 后 `make install`）。
- **管理员**：`admin`，密码在 `~/.config/felix-homelab/.env` 的 `IDM_ADMIN_PASSWORD`
  （首次安装自动生成）。该变量只在配置卷首次初始化时生效；之后改密码走 Web 界面
  （设置 → 密码）。
- **数据**：`felix-homelab-opencloud-data` 卷（文件与 decomposedfs 元数据，依赖 xattr）；
  配置与 IDM 密钥在 `felix-homelab-opencloud-config` 卷。均为命名卷，
  查看挂载点：`podman volume inspect felix-homelab-opencloud-data`。
- **备份**：随每日 03:00 / 后台手动备份一起执行，由宿主脚本
  `config/backup/backup-opencloud.sh` 打包为 `felix-homelab-opencloud-<时间戳>.tar.gz`；
  在 `podman unshare` 内用宿主 GNU tar `--xattrs` 保留 xattr（服务在线打包，
  tar 退出码 1 的「读取期间文件变化」告警可接受），归档校验通过才落盘。
- **恢复**：停服务后把归档解回两个卷（同样用 `podman unshare tar --xattrs`），再启动：
  ```bash
  systemctl --user stop felix-homelab-opencloud.service
  podman unshare tar --xattrs -xzf <归档> \
    -C "$(podman volume inspect -f '{{.Mountpoint}}' felix-homelab-opencloud-data)" . \
    -C "$(podman volume inspect -f '{{.Mountpoint}}' felix-homelab-opencloud-config)" etc-opencloud
  systemctl --user start felix-homelab-opencloud.service
  ```
- **客户端**：官方桌面/手机客户端或任意 WebDAV 客户端，地址填
  `https://opencloud.wraindrock.com`（OIDC issuer 由 `.env` 的 `OC_URL` 决定）。

## 统一账户（Kanidm）

自建 IdP，统一给 Forgejo / 主站 /（后续）Tuwunel 与 OpenCloud 提供 OIDC 登录。
数据在命名卷 `felix-homelab-kanidm-data`（SQLite + TLS 证书），服务经 Cloudflare
Tunnel 暴露为 `https://id.wraindrock.com`。

- **服务**：`quadlet/felix-homelab-kanidm.container`（`kanidm/server:1.11.2`，
  只发布 `127.0.0.1:8443`）。Kanidm **没有 Web 管理界面**，一切用 CLI。
- **管理员**：内置超级用户是 `idm_admin`（不是 `admin`），密码在 `.env` 的
  `KANIDM_ADMIN_PASSWORD`（首次安装自动生成；忘了可
  `podman exec -it felix-homelab-kanidm kanidmd recover-account idm_admin` 重置）。
- **CLI 登录**（需要 TTY，脚本化用 pexpect 转发密码；会话缓存在 `kanidm-cli-home` 卷）：
  ```bash
  podman run --rm -i -t --network host --add-host id.wraindrock.com:127.0.0.1 \
    -v kanidm-cli-home:/tmp/k -e HOME=/tmp/k \
    -e KANIDM_URL=https://id.wraindrock.com:8443 -e KANIDM_NAME=idm_admin \
    -e KANIDM_ACCEPT_INVALID_CERTS=true \
    --entrypoint kanidm docker.io/kanidm/tools:1.11.2 login
  ```
- **OIDC 接入要点**（实测）：发现地址是**按客户端**的
  `https://id.wraindrock.com/oauth2/openid/<客户端名>/.well-known/openid-configuration`；
  `update-claim-map <客户端> <声明> <组> [值...]`（值以空格分隔，JSON 数组字面量不可用），
  数组声明先 `update-claim-map-join <客户端> <声明> array`；
  内置 `groups` 作用域会输出组 SPN/UUID 列表（自定义 claim 同名时会被覆盖，本项目按 SPN 匹配）。
  口令策略：Kanidm 要求 zxcvbn 4/4（日期样式、用户名/域名词会被重罚），弱口令无法提交；
  密码 + TOTP 才算满足 MFA 提交条件（CLI：`person credential use-reset-token` 交互式设置）。
- **备份**：随每日 03:00 / 后台手动备份执行 `config/backup/backup-kanidm.sh`
  （`podman unshare` 内 SQLite 在线 `.backup` + 证书 → `felix-homelab-kanidm-<时间戳>.tar.gz`）。
- **恢复**：停服务，把归档解回 `felix-homelab-kanidm-data` 卷，再启动：
  ```bash
  systemctl --user stop felix-homelab-kanidm.service
  podman unshare tar -xzf <归档> \
    -C "$(podman volume inspect -f '{{.Mountpoint}}' felix-homelab-kanidm-data)" .
  systemctl --user start felix-homelab-kanidm.service
  ```
- **Forgejo 接入（已完成）**：Kanidm 建机密客户端 `forgejo`（`forgejo.wraindrock.com/user/oauth2/kanidm/callback`），
  Forgejo 侧 `admin auth add-oauth --provider openidConnect --auto-discover-url …/oauth2/openid/forgejo/.well-known/openid-configuration`。
  实测要点：① 目标 redirect URL 必须 `add-redirect-url` 单独加（`create` 的第三参是 landing URL）；
  ② 给客户端 `warning-insecure-client-disable-pkce`（Forgejo 早期不发送 PKCE）；
  ③ `groups` 作用域产出的是组 SPN/UUID 列表，`--admin-group` 用 `felix-admins@id.wraindrock.com`；
  ④ 账号绑定：先本地登录 Forgejo，再访问 `/user/oauth2/kanidm` 完成关联（`external_login_user` 记 sub=Kanidm UUID）。
  **统一账号终态（已落地）**：Felix 的本地密码已清空（本地登录提示“用户名或密码不正确”），
  本地注册关闭（`ALLOW_ONLY_EXTERNAL_REGISTRATION=true`）——Forgejo 只认 Kanidm。
  新用户首次经 Kanidm 登录会进入 `/user/link_account` 确认用户名/邮箱（预填）后自动建号
  （实测 `login_type=6` OAuth2）；scope map 必须覆盖普通用户组 `felix-users`，
  否则普通用户会 `Access Denied`（available_scopes 为空）。
- **账户策略（实测结论）**：内置 `idm_all_persons` 的 `credential-type-minimum` 已从 `mfa` 放宽为 `any`
  （口令最短 10 位），**普通用户仅密码即可**；`felix-admins` 组保持 `mfa`（多组并存取最严，
  实测管理员仍强制 TOTP/Passkey）。注意 zxcvbn 4/4 口令质量始终强制（与 MFA 独立），
  建议开户时用 4 个词（如 `blue-cat-happy-river`）。
- **开户**：`scripts/kanidm-adduser.sh <用户名> "<显示名>" [邮箱]` 一键建号 + 入组 +
  生成 7 天有效的 onboarding 链接（`https://id.wraindrock.com/ui/reset?token=…`），
  私发对方设密码即完成；无需邮件系统。自助注册入口待主站 OIDC（用 Kanidm 服务账号 API token 自动建号）。
- **主站接入（已完成，无感原生）**：主站登录页就是**本站账号**的登录（用户名 + 密码 + 可选动态验证码），
  服务端在内部完成统一账号校验（HTTP 认证会话：init2 → begin → TOTP/密码），用户界面不出现底层组件名。
  已绑定账号的凭据不再落本站库（`oauth_identities` 记录 `sub`）；过渡期未绑定的老用户仍可用本站旧密码。
  Forgejo 的认证源已更名为「Wraindrock」（登录按钮「使用Wraindrock登录」，回调
  `/user/oauth2/Wraindrock/callback`）。
- **状态**：P0 完成；P1 进行中——✅ Forgejo OIDC（已闭环）、✅ 分组账户策略与开户流程、✅ 主站 OIDC。规划见 `site/TODO.md`。

## 首页与导航（订阅相关）

- **开场动画**：首页每会话首次访问时，黑底居中「Wraindrock」→ 飞至顶栏品牌位 → 遮罩淡出、内容淡入。
  纯 CSS + 两段内联脚本实现（见 `app.rs` 的 `SPLASH_*_SCRIPT`），不依赖 wasm 水合；
  尊重 `prefers-reduced-motion`；带 5 秒兜底，脚本异常也不会把内容藏住。
- **导航**：「购买订阅」（`/services`，价格目录）与「我的订阅」（`/subscriptions`）。
- **我的订阅**：展示已订阅的 AI Agent（卡片：打开即用、有效期、状态）与容量订阅
  （OpenCloud / Forgejo 共用容量；订单系统上线后显示已购/已用/剩余/到期）。
  原先放在首页的「我的 Agent」区块已并入此页。

## 语音转文字（STT）

站内自托管、GPU 加速的语音识别服务（faster-whisper / CTranslate2，OpenAI 兼容）：

- **服务**：`quadlet/felix-homelab-whisper.container`（镜像 `localhost/felix-whisper:latest`，
  `scripts/build-whisper.sh` 构建），只发布 `127.0.0.1:8085`；
  经 NVIDIA CDI（`AddDevice=nvidia.com/gpu=all` + `SecurityLabelDisable=true`）使用 GPU，
  无 GPU 时应用自动回退 CPU。
- **模型**：`large-v3`（float16，约 3.8G 显存；中文/多语种），缓存在命名卷
  `felix-homelab-whisper-models`，首次启动从 `HF_ENDPOINT=https://hf-mirror.com` 下载（约 3-4GB）。
  模型缓存**不纳入备份**（可重新下载）。
- **接口**：`GET /healthz`；`POST /v1/audio/transcriptions`（multipart `file`，可选 `language`、`response_format`）。
  实测：11 秒英文 1.23s、5 秒中文 0.77s。
- **选型备注**：不用官方 whisper.cpp 镜像——其二进制按构建机 AVX512 编译，本机
  （i7-13650HX）加载模型即 SIGILL；选用 faster-whisper 后无编译、CUDA 开箱即用。
- **安装**：`make install` 在检测到 NVIDIA CDI 时自动构建镜像并启用本服务，否则跳过。

## 富媒体与语音

- **媒体接口**：`POST /api/media`（登录上传，内容寻址 + zstd 策略）、`GET /media/{id}/{name}`
  （Range/按需解压/immutable 缓存）；详见上文「语音转文字」与 `site/TODO.md` 的存储压缩策略。
- **媒体工具条**（社区发布页与评论框）：`图片` 选择上传并按 Markdown 插入正文；
  `语音` 录音（MediaRecorder）→ 上传为语音消息 + 调 `/api/stt` 转写，
  识别文字与 `[🎤 语音](/media/…)` 一并插入正文，发送前可编辑。
- **语音消息渲染**：正文里指向本站 `/media/` 的音频链接会渲染成 `<audio controls>` 播放条
  （`content.rs` 的 Markdown 事件改写；URL 白名单 + 转义，用户内容同样安全）。
- 实测：图片上传→正文插入→发布→详情页渲染（1 图 1 音频）；假麦克风录音转写
  “Thanks for watching!”；视频同一路径（`[🎬 视频]` 链接渲染为 `<video controls>`）。
- **媒体压缩时序**：上传原样存；后台任务按龄重压（>7 天 -3、>1 月 -7、>3 月 -19），
  热文件（近 7 天访问过 / 累计 ≥30 次）跳过；详见 `site/TODO.md`。
- **容量池**：媒体配额按原始大小计入容量池；「我的订阅」显示容量池进度条；
  已开通后超额上传会被拒绝（过渡期不限）。

## 订单与订阅

- **商品**：AI Agent 时间池充值、容量池订阅、个人外置云存储（独立于本站容量池）。
- **通道**：默认人工（后台「服务订单」确认收款即自动发放，幂等）；配置 Creem 后走在线收款
  （`POST /api/payments/creem/webhook`，HMAC-SHA256 验签，事件 `checkout.completed` /
  `subscription.paid`）。官方文档依据见 `site/TODO.md`。
- **页面**：「购买订阅」下单（登录后）；「我的订阅」显示时长池/容量池进度、外置存储状态与订单列表；
  支付回跳 `/subscription/success`。

## 后台运维（/admin）

登录主站后点顶栏「后台」，包含：

- **概览**：待审评论/评价、用户数、容器运行与健康汇总、快捷入口
- **评论 / 社区**：社区管理员（communitymaster）负责
- **评价**：光遇管理员（skymaster）负责
- **Agent**：Agent 管理员（agentmaster）或超级管理员负责
- **用户 / Pod / 备份**：仅超级管理员（`users.role = admin`，即站长）

**管理员角色细分**（`后台 → 用户` 里授予/撤销，可叠加）：

| 角色            | 权限 scope | 可见后台          |
| --------------- | ---------- | ----------------- |
| （站长）admin   | super      | 全部              |
| agentmaster     | agent      | 概览 + Agent      |
| communitymaster | community  | 概览 + 评论 + 社区 |
| skymaster       | sky        | 概览 + 评价       |

导航会按权限自动过滤，越权访问页面与接口都会被拒（服务端逐个校验）。

- **Agent 类型**：OpenCode、DeepSeek Harness、**OpenClaw**（自托管 Gateway，自带控制台）、
  **Kilo Code**、**Pi**（终端型，容器内 ttyd 提供 Web 终端）。
  构建：`make agent-build` / `-dsh` / `-openclaw` / `-kilocode` / `-pi`；
  类型白名单在 `site/src/agents.rs` 与 `agent_subscriptions.kind`（见迁移 0018）。

## AI Agent（多租户，P1 试点）

每个授权账号可以拥有**多个独立 Agent 实例**（slot）：同一模板镜像 + 独立数据卷 +
独立工作区 + 随机化子域。站点负责开通与鉴权，宿主脚本负责容器生命周期（含
**空闲睡眠**与**撤销宽限期回收**）；**用户登录后在首页「我的 Agent」看到自己的
入口**，后台只负责指定用户、指定类型（OpenCode / DeepSeek Harness）、数量与备注。

```
用户首屏「我的 Agent」 ─▶ <18位随机码>.wraindrock.com
      ─▶ Cloudflare（Tunnel 的 `*.wraindrock.com` 公共主机名，橙云 Universal SSL）
      ─▶ Agent 网关（独立容器，host 网络，只监听 127.0.0.1:5740）
            ├─ forward_auth ─▶ 127.0.0.1:5735（主站 /api/agent/auth：会话 + 订阅校验；
            │                   睡眠实例在这里被唤醒，等宿主探测到 ready 才放行）
            └─ reverse_proxy ─▶ 127.0.0.1:20001..（各 Agent 的独立 bridge 网络）
主站/Forgejo 继续走原隧道 20080 → Pod 内 Caddy（与 Agent 完全隔离）
```

**每个 Agent 一个独立 bridge 网络**：彼此不可见，也到不了主 Pod 内的
Postgres/Forgejo/站点内部端口；出站互联网正常（git push 走公网
`https://forgejo.<域名>` / `ssh -p 20022`，无需入站端口）。

- **域名样式**：`r4nd0m.<用户名>.<opencode|deepseekharness>.agent.<域名>`。
  随机段由站点生成并入库（同用户多实例不重复），续期/宽限期内复活时**原样复用**。
  本地测试：主 Caddy 不再承载 Agent 路由，用
  `curl -H "Host: <子域>.agent.localhost" http://127.0.0.1:5740/`（或开发者自行
  hosts 绑定）；生产出口仍是云域名。
- **后台开通（只新建）**：`后台 → Agent` 顶部的「开通新实例」：填用户名、模板、
  数量（1–9）、天数（0 = 长期）、备注——每次都会占用新的 slot 与新的随机域名，
  不影响已有实例。页面写订阅表并落请求文件，宿主
  `felix-homelab-agent-run.service`（`.path` 即时触发、`.timer` 每 2 分钟巡检）
  建容器、写路由、回写状态。
- **逐实例操作**：列表每行都有「续费 + 启停 + 删除」，已撤销行额外有「永久删除」：
  - **续费**：按填写的天数延长有效期（有效期内叠加）；**已撤销的实例点「续费恢复」
    会在 30 天宽限期内连同原域名与数据一起复活**；
  - **启动/暂停**：暂停后网关拒绝访问且不自动唤醒；
  - **删除**（可恢复）：立即停容器、撤子域路由；数据/域名保留 30 天；
  - **永久删除**（不可恢复）：用于版本测试、注销用户数据清理等特殊情况，
    会立即回收容器/数据卷/工作区/子域路由并留下「彻底删除记录」。
    防误触：必须原样手抄 `我确认永久删除<随机码>`（随机码 = 域名的第一段），
    服务端逐字校验。
- **删除记录的留存与显示**：已删除（待宽限）与彻底删除记录都保留 30 天，
  **后台默认隐藏**；勾选「显示已删除 / 彻底删除记录」即可查看与操作
  （彻底删除记录只读展示，30 天后自动清理）。
- **容器加固**：rootless + `no-new-privileges` + 危险能力显式丢弃 + 独立网络 +
  端口只发布宿主回环 + 不挂载 `podman.sock`；数据卷挂载带 `:Z`（SELinux 类别）。
- **定期重建**：容器可写层非持久——超过 `AGENT_RECREATE_DAYS`（默认 7 天）
  由 reconcile 重建容器（数据卷/工作区/域名/登录态保留），清掉潜在的持久化改动。
- **版本固化与回滚**：构建模板镜像时除 `:latest` 外再打一个**不可变版本标签**
  （如 `localhost/felix-agent-dsh:0.1.7-rc.2`），默认保留最近
  `AGENT_IMAGE_KEEP`（3）个版本；`agent-ctl.sh versions` 查看本机留存与在用
  实例，`agent-ctl.sh pin <opencode|dsh> <版本>` 固定版本（写回 `.env`），
  `agent-ctl.sh recreate-all <kind>` 让全部实例换镜像（数据全保留）。
  新版本不稳定或插件不兼容时：`pin dsh <旧版本>` → `recreate-all dsh` 即回滚；
  恢复最新：`pin dsh latest` → `recreate-all dsh`。切换只发生在重建时，
  睡眠实例下次唤醒自然使用固定版本，不会打断用户当前会话。
- **安全边界说明（重要）**：rootless 容器仍是**共享内核**，对“不受信的人”不算
  安全边界（逃逸即宿主用户）。当前方案适合“可信任的朋友/试用”档；正式面向
  陌生人应上 microVM（Kata/gVisor）或独立主机（本仓库暂不包含）。
- **空闲睡眠**：实例无请求超过 `AGENT_IDLE_SECONDS`（默认 300 秒）自动停容器
  （`desired=sleeping`，数据/域名/登录态保留）；用户再次打开时由网关**自动
  唤醒并等端口就绪**再放行——首开多等几秒，之后与常驻无异。
- **删除与回收**：后台「删除」立即停容器、撤子域路由；**数据与域名保留
  `AGENT_GRACE_DAYS` 天（默认 30）**，期间续期原样复活；超期由宿主回收
  （容器/数据卷/工作区/路由/状态条目）并在后台清理订阅行，域名随后可再分配。
- **容器**：`felix-agent-<内部slug>`，独立 bridge 网络（与主 Pod、其他 Agent
  互不可见），端口只发布到宿主回环；资源上限由 `.env` 的
  `AGENT_MEMORY` / `AGENT_CPUS` / `AGENT_PIDS_LIMIT` 控制；带健康检查与
  `autoheal` 标签。
- **数据与账户统一**：订阅在站点 SQLite（`agent_subscriptions`，`UNIQUE(user_id, slot)`）；
  运行数据在 `felix-agent-<slug>-data` 卷，工作区在
  `~/.local/share/felix-homelab/agents/<slug>/workspace`。
- **凭据自备**：平台只收资源费，模型 API Key 由用户在 Agent 内自行配置，
  存在各自数据卷里，互不可见。
- **DeepSeek Harness 镜像（官方方式）**：fnm 管理 Node（官方要求
  `^22.19 || >=24`，固定 24），corepack 开启 yarn/pnpm（pnpm 对齐官方仓库
  11.7.0），`npm i -g @deepseek-ai/dsh@<固定版本>`。注意 npm 11 默认不执行
  依赖安装脚本，构建时显式 `--allow-scripts`（否则 spawn helper / koffi /
  node-pty 缺失，表现为“功能不完整”）。
- **自带插件市场 dsh-market**：镜像构建期按官方方式
  `dsh plugin --profile web add dshmarket@<固定版本>`（默认 1.66.3，
  `.env` 的 `DSHMARKET_VERSION` 可调），用户打开 **设置 → 插件市场** 即可
  浏览/搜索/一键安装社区插件与主题。容器化下市场的一键重启会替换容器主
  进程，因此启动参数带 `--patch /opt/dsh-home/agent-patch.yml` 统一禁用
  （`allowRestart: false`，重启由平台/管理员操作）；`GET /dsh-market/status`
  应显示 `restart: false`。
- **第三方修改政策**：凡是修改过的第三方插件/组件，一律 fork 到
  `FelixHomelab` 下、从 fork 固定提交部署（避免上游更新覆盖修复），详见
  [`FORKS.md`](FORKS.md)。
- **OpenCode 模板镜像默认内置 agent-cache-optimizer**
  （`AGENT_CACHE_OPTIMIZER_REF`，固定 fork `felix/opencode-v2` 提交）：上游 0.6.1
  只支持 OpenCode 1.x 插件 API，2.0.18 上会 “Plugin must export a default
  definition…”，**核心重排完全失效**。按平台规则在 fork 里加了 v2 适配器
  （`ctx.session.hook("context")` 重排 `output.system` 后再写回，复用原有分类/
  重排核心；Anthropic 的 chat.headers 在 v2 暂无对应注册点）。镜像把插件源码
  烤进 `/opt/agent-cache-optimizer`，入口脚本在 `opencode.json` 里以**绝对路径**
  幂等注入（不覆盖用户配置），无需 npm 安装、离线可用。作用：稳定系统提示块
  前置，保住 provider 的前缀 KV 缓存，提升命中率、降低使用成本。
- **自带插件守护 dsh-my-guardian**（`DSHGUARDIAN_REF`，固定 fork 提交）：
  候选区 + 失败隔离，防止用户装坏插件把容器卡死。守护是看门狗，必须最先
  加载——构建期把它的 bundle 调到名册第一位（每次加新插件后都会校验）。
- **自带费用统计 dsh-cost-meter**（`DSHCOSTMETER_VERSION`，默认 1.7.40）：
  会话/模型成本、预算、官方余额与 Coding Plan 额度查询，中英双语。
- **自带侧边栏工作台 dsh-better-sidebar**（`DSHBETTERSIDEBAR_VERSION`，默认
  0.22.1，要求 DSH 0.1.7+）：文件树与可编辑编辑器、文件变动（Git + 本轮 AI
  改动）、任务/子代理拓扑、侧边对话、底部工作台；并向所有插件开放
  `ctx.betterSidebar` 服务（`registerTab` / `registerFileViewer`）。
- **自带开发规则 dsh-dev-rules**（`DSHDEV_RULES_REF`，git 分发、固定提交
  SHA，默认 `83c5ff32`；npm 无可用包名，官方推荐 github/gitee 来源）：右侧栏
  「开发规则」面板可视化维护（全局 + 按项目、追加/覆盖），每个会话自动把生效
  规则注入系统提示；另带 `dev_rules` 工具，对话里也能直接记规则。
- **平台默认规则（网络搜索）**：装了 dev-rules 的实例首次启动时，入口脚本会
  把一条「平台默认」规则播种进 `$DSH_HOME/dev-rules.json`：**联网搜索默认用
  无头浏览器（ego_* 工具）+ Bing**（国内 Google/DuckDuckGo 不可用；不要用
  curl/wget 抓搜索结果页；遇登录/验证码提示用户接管）。只填空文档、不覆盖
  用户内容；用户在面板里删掉该规则（保留其它规则）后不会再被加回。
- **模型协议边界原生（Go）化：felix-llm-relay**：容器自带纯标准库的 OpenAI
  兼容中转（`127.0.0.1:3160`）。DSH 里用「自定义 OpenAI 提供商」接入
  `http://127.0.0.1:3160/v1`：多厂商路由、超时、SSE 透传都在这个原生进程里，
  上游真实 API Key 只存在 `/data/llm-relay.json`（0600，可直接在 DSH 文件树里
  编辑），不进 DSH 设置与凭据存储。选它的原因：插件/会话/设置这些内部契约仍在
  高速迭代（本平台本周就适配了 3 处破坏性变更），而「OpenAI HTTP + SSE +
  Bearer」是已经固化的行业协议，适合原生实现。
- **自带 ego 浏览器 dsh-ego-browser**（`DSHEGOBROWSER_REF`，固定 fork 提交）：
  30+ 个 `ego_*` 浏览器自动化工具 + 实时观察窗（装了 better-sidebar 时注册为
  侧边栏原生 Tab，否则浮动观察球）。**双引擎**：
  - **默认无头 → Obscura**（Rust + CDP 的轻量无头引擎，`OBSCURA_VERSION`
    固定版本）：实测载入页面时容器内存增量约 **7MB**、引擎进程约 62MB；
    截图 / 串流（screencast）/ DOMSnapshot / Input / Storage 等 ego 依赖的
    CDP 域覆盖完整，观察窗在无头模式下也能看画面。
  - **按需有头 → Chromium**（点观察窗「弹出窗口」）：跑在容器虚拟显示
    （Xvfb + openbox + x11vnc + noVNC）上，实测容器增量约 **336MB**；
    缺会话 D-Bus / mesa GL 时会有头起不来（实测），镜像已内置。
    任何设备打开 **`https://<你的Agent子域>/vnc/vnc.html`** 即可操作这个
    真实 Chromium 窗口（登录、验证码、弹窗、下载）；不依赖用户桌面，
    多租户下也不挂宿主桌面套接字。
  `felix-ego-chrome.sh` 负责协议适配（Obscura 是 CDP 服务形态，运行时是
  Chrome 命令行形态）并在 Obscura 不可用时自动回退 Chromium 无头。
  fork 里带一处 runtime 补丁：内部同步表达式改走 `evaluateSync()`
  （`awaitPromise:false`）——否则 Obscura 在 Bing 等重页面上 `page.info()`
  会因 awaitPromise 排队 >15s 超时（实测修复后 info 5ms）。
  **反爬回退开关**：`felix-ego-engine chromium` 切到有头 Chromium、
  `felix-ego-engine obscura` 切回省内存（默认）——百度百科/知乎等站点的
  安全验证只有有头能过（无头 Chromium 同样会被识破），平台默认规则已让
  Agent 在遇到「安全验证/快照为空」时自动执行切换并重开任务空间。
  **两套引擎存储独立**：Obscura 用 `--storage-dir` 的 `cookies.json`，
  Chromium 用自己的 Profile——想让日常无头会话带登录态，就在无头观察窗里
  登录。**镜像内置 Noto CJK / Emoji 字体**（否则观察窗里中文全是方框）；
  每次启动入口脚本会补齐插件 wrapper 可执行位。
- **自带 OpenCode Zen 免费模型 @opencode2dsh/dsh-plugin**
  （`OPENCODE2DSH_REF`，固定 fork 提交 `felix/configforms`）：匿名免费通道，无需 API key，
  在模型选择器里以 `opencode2dsh` 分组出现；需要出站 HTTPS 访问
  `opencode.ai` 与 `models.dev`（Agent 独立网络默认允许出站）。
  **DSH 0.1.7 兼容修复随 fork 走**：0.1.7 移除了客户端 `settingsScope`
  服务（上游 issue #20/#24，修复 PR #23 尚未发版），而插件 0.3.3 仍硬性
  依赖它，网页会卡在 `pending (waiting for service: settingsScope)` 起不来。
  修复做在 fork 的分支 `felix/configforms`（源码级迁到 `configForms`，构建
  产物随库提交），**镜像直接从 fork 固定提交安装**——从插件市场装别的插件
  触发 pnpm 重装时，也会按 lockfile 回到同一个 fork 提交，修复不会被上游
  覆盖。同步方法见 [`FORKS.md`](FORKS.md)。
- **用户自装插件/主题升级不丢**：镜像升级的 profile 合并对
  `dsh.profile.bundles` 取并集——镜像自带的在前、用户市场装的追加在后
  （守护 `dsh-my-guardian` 强制置顶），`dependencies` 同样并集保留。
- **pnpm store 一致性（运行时装插件的前提）**：pnpm 把 store 路径写进
  `node_modules/.modules.yaml`，构建期与运行期 HOME 必须一致（`/data`）；
  镜像升级时 `dsh-merge-profile.py` 会随镜像刷新该文件，否则用户从插件
  市场安装会报 `ERR_PNPM_UNEXPECTED_STORE`。
- **镜像升级保数据**：版本戳涵盖 DSH 与市场版本；升级镜像时入口脚本用
  `dsh-merge-profile.py` **非破坏合并**刷新 profile——官方文件更新，用户
  自己装的插件、收藏/分组/备注（`state.json`）与手工改过的
  `cordis.patch.yml` 全部保留；凭据/会话/设置也在数据卷里不受影响。
  工作区 bind mount 必须带 `:Z`（SELinux 重打标签），否则容器内读不到工作区。
- **DeepSeek Harness 登录**：DSH 自带令牌登录（每次启动在日志里打印带 token 的
  URL，约 30 天 Cookie）。宿主脚本从容器日志提取当前令牌写入 `status.json`，
  首页卡片与后台都显示**带令牌的入口链接**，点开即完成登录。DSH 默认拒绝
  绑定 `0.0.0.0`；镜像用 `DSH_ALLOW_NON_LOOPBACK` 环境变量门控放行（仅本
  平台容器设置），绑 `0.0.0.0` 但端口只发布到宿主回环、外面还有网关鉴权。
- **信任栅栏必须用实例子域**：启动参数 `--trusted-host` 要传浏览器实际访问的
  域名（`<18位随机码>.<域名>`，实例的真实子域）。若错传内部 slug，
  DSH 自己的 `/api`（设置、模型、插件清单等）会全部 403，表现为“模型提供商
  不可用 / 设置页打不开”。

### 首次启用

```bash
make agent-build        # 构建 OpenCode 模板镜像（版本固定 OPENCODE_VERSION）
make agent-build-dsh    # 构建 DeepSeek Harness 模板镜像（版本固定 DSH_VERSION）
make install            # 装 agents 目录、systemd 触发单元与 Caddy 路由挂载
# 后台 → Agent 开通；用户登录后首页出现入口（本地域名 <用户名>.agent.localhost）
```

生产公网访问需两步：

1. **会话 cookie 跨子域**：`COOKIE_DOMAIN=wraindrock.com` 已默认写入 `.env`，
   主站按请求 Host 自适应——`localhost` 不加 Domain（本地登录照常），
   `*.wraindrock.com` 才加，非回环 Host 自动带 `Secure`。无需额外配置。
2. **云侧 Caddy（一次性，之后全自动）**：套用
   [`config/cloud/Caddyfile.example`](config/cloud/Caddyfile.example)：

   ```caddyfile
   {
       on_demand_tls {
           ask http://127.0.0.1:20080/api/agent/tls-ask
       }
   }
   # 多级 Agent 子域：通配只匹配一级，改用 catch-all + on-demand TLS
   :443 {
       tls { on_demand }
       reverse_proxy 127.0.0.1:20080
   }
   ```

   ```bash
   scp config/cloud/Caddyfile.example root@<云机>:/etc/caddy/Caddyfile
   ssh root@<云机> 'caddy validate --config /etc/caddy/Caddyfile && systemctl reload caddy'
   ```

   之后新增用户/实例**不必再改云配置**：子域首次访问时云侧 Caddy 先问主站
   `/api/agent/tls-ask`（经 20080 隧道，只接受回环 Host），仅对有效订阅按需
   签发证书（TLS-ALPN-01，不需要 DNS API，也不需要通配证书）。

### DeepSeek Harness：远程与本地体验一致（服务商、模型自选）

上游把「设置 / 提供商编辑」限定为 loopback：浏览器地址不是 `localhost`/`127.x`
时，模型页报 `settings are unavailable in this browser`。该限制**只在客户端一个
布尔值**（`dsh-client-connection` 的 `isLoopback`），服务端没有特权校验。

本平台每个用户都是**独立容器**、且网关（Caddy forward_auth + 主站会话）已完成
账户级鉴权，因此模板镜像在构建期精确放开该门控：
`containers/agent-dsh/unlock-remote-settings.py`（精确字符串替换，找不到目标会
构建失败，强制升级 DSH 时人工复核）。实测（无头 Chromium 真实点击）远程访问
「设置 → 模型」与本地完全一致：可自选服务商（DeepSeek 官方 / 自定义 OpenAI
兼容端点）、填 API Key、选模型，无需管理员介入。

管理员仍有应急/批量的命令行通道（直接写官方凭据文件，DSH 热加载即时生效）：

```bash
make agent-setkey ARGS="Felix 2 DEEPSEEK_API_KEY=sk-..."   # 写入（值不回显）
make agent-setkey ARGS="Felix 2"                            # 只看已设置的名称
```

### 运维

```bash
make agent-list                    # 状态总览（用户/slot/类型/端口/运行态/健康/子域）
make agent-stop ARGS="alice 2"     # 暂停 alice 的 2 号实例
make agent-remove ARGS="alice 2"   # 删除容器与子域路由（数据卷/工作区保留）
make agent-setkey ARGS="alice 2 DEEPSEEK_API_KEY=sk-..."  # 设置 DSH 密钥
make agent-apply                   # 手动触发一次请求处理 + 保活
systemctl --user status felix-homelab-agent-run.service   # 处理日志
```

> P1 隔离边界：受信用户场景的容器级隔离（rootless、独立卷、独立工作区）。
> 所有 Agent 与主 Pod 共享网络命名空间（已知取舍）；面向不受信用户应迁移到独立
> `Felix-Agents` Pod 或每用户 microVM。Agent 容器不挂载 `podman.sock`。

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
make agent-build # 构建多租户 Agent 模板镜像（OpenCode）
make agent-build-dsh # 构建多租户 Agent 模板镜像（DeepSeek Harness）
make agent-list  # 列出 Agent（用户/slot/类型/端口/运行态）
make agent-apply # 立即处理 Agent 请求并保活（一般由 systemd 自动触发）
make agent-stop ARGS="<用户名> [slot]"   # 暂停某用户的 Agent 实例
make agent-remove ARGS="<用户名> [slot]" # 删除实例容器与子域路由（数据卷保留）
make agent-setkey ARGS="<用户名> [slot] KEY=VALUE"  # 设置 DSH 密钥（写入实例卷）
make migrate    # 从旧命名 Felix-Workstation 迁移到 Felix-Homelab
make uninstall  # 停止并移除单元，保留数据
make purge      # 连数据卷、配置一起删除（危险）
make prune      # 清理构建残留（悬空镜像、构建工作容器；不删数据卷）
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
- **多租户 Agent**：`.env` 的 `AGENT_*`（镜像、资源上限、子域）；
  `~/.config/felix-homelab/agents/` 下 `state.json`（宿主状态）、`requests/`（后台请求）、
  `caddy/`（每用户路由）、`status.json`（回写后台）。这些文件含每用户网关口令，
  权限已收紧，不要提交到 Git。
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
| `felix-homelab-db-data`      | PostgreSQL 数据（Forgejo 库） |
| `felix-homelab-forgejo-data` | 仓库、附件、app.ini  |
| `felix-homelab-site-data`    | 主站 SQLite 与上传图片 |
| `felix-homelab-runner-data`  | Runner 注册与缓存    |
| `felix-homelab-caddy-*`      | Caddy 证书与配置     |
| `felix-homelab-opencloud-config` | OpenCloud 配置与 IDM 密钥 |
| `felix-homelab-opencloud-data`   | OpenCloud 文件与 decomposedfs 元数据（依赖 xattr） |
| `felix-homelab-kanidm-data`      | Kanidm 数据库（SQLite）与 TLS 证书 |
| `felix-agent-<用户名>-data`  | 各用户 Agent 运行数据（凭据/会话，每个授权账号一个） |

配置目录（含 `.env`、`Caddyfile`、`homepage/`、`runner-config.yml`、`runner.secret`）
位于 `~/.config/felix-homelab/`，其中机密文件不入库；备份源与渠道配置在
`sync/backup.conf`，同步状态在 `backup/sync.status`。

备份目录：`~/.local/share/felix-homelab/backups/`（归宿主用户所有，可直接拷贝到外部存储）。

## 备份与恢复

备份分**备份源**与**备份渠道**两层，全部配置集中在
`~/.config/felix-homelab/sync/backup.conf`（可在后台「备份」页图形化编辑）：

- **备份源**（各自独立开关）：Forgejo、主站；
- **备份渠道**（异地，各自独立开关）：rclone / rsync 随意添加多个，也可全部关闭；
- **保留天数**：统一控制本机归档清理。

### 自动备份

调度由**宿主机 systemd user timer** 负责（不在容器里跑 cron）：

```
felix-homelab-backup.timer  (OnCalendar=03:00, Persistent=true)
  └─ felix-homelab-backup-run.service
       └─ podman exec felix-homelab-backup sh /usr/local/bin/backup.sh once   # Forgejo + 主站
```

- `Persistent=true`：机器在 03:00 关机/休眠时，**开机后立刻补跑**错过的备份；
- 备份容器常驻空闲，实际动作由 timer 触发（也便于手工 `make backup`）；
- 归档与校验（默认保留 **7 天**，`KEEP_DAYS` 可调）：
  - `felix-homelab-dump-<时间>.tar.gz`：`forgejo dump`（数据库 + 仓库 + 附件 + LFS + 配置），
    必须含 `app.ini`、`forgejo-db.sql`、`repos/`；
  - `felix-homelab-site-<时间>.tar.gz`：主站 `site.db`（SQLite `.backup`）与上传图片，
    必须含 `site.db`；
  - 任一校验不通过即删除该归档，避免“假绿灯”。

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

