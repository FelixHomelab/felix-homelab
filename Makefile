# Felix-Homelab on Podman (Quadlet)
#
# 常用目标：
#   make install     安装并启动 Felix-Homelab pod
#   make register    向 Forgejo 注册 Actions runner 并启动
#   make backup      立即执行一次备份
#   make backup-list 列出备份文件
#   make restore     从备份恢复（默认最新）
#   make status      查看 pod / 容器状态
#   make logs        跟踪 pod 日志
#   make restart     重启 pod
#   make migrate     旧命名迁移（Felix-Workstation → Felix-Homelab）
#   make uninstall   停止并移除所有单元（保留数据卷，除非 make purge）

SHELL := /usr/bin/env bash
REPO_DIR := $(shell pwd)

# 容器服务；runner（注册后）与 frpc（配置中转后）可能不存在，故启动时忽略其错误
SERVICES := felix-homelab-db.service felix-homelab-forgejo.service \
	felix-homelab-site.service \
	felix-homelab-caddy.service felix-homelab-agent-gateway.service \
	felix-homelab-homepage.service \
	felix-homelab-opencloud.service \
	felix-homelab-kanidm.service \
	felix-homelab-whisper.service \
	felix-homelab-runner.service felix-homelab-frpc.service \
	felix-homelab-agent-frpc.service \
	felix-homelab-backup.service felix-homelab-autoheal.service

.PHONY: install register build-images build-site status logs restart stop start \
	backup backup-list sync-backup restore deploy uninstall purge prune migrate help \
	agent-build agent-build-dsh agent-build-kilocode agent-build-pi agent-build-zeroclaw agent-list agent-apply agent-stop agent-remove \
	agent-setkey doctor

help:
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | \
		awk 'BEGIN{FS=":.*?## "}{printf "  \033[36m%-12s\033[0m %s\n", $$1, $$2}'

install: ## 安装并启动 Felix-Homelab
	$(REPO_DIR)/scripts/install.sh

register: ## 注册并启动 Forgejo Actions runner
	$(REPO_DIR)/scripts/register-runner.sh

build-images: ## 构建修复版 Runner 作业镜像（make register 会自动执行）
	$(REPO_DIR)/scripts/build-runner-images.sh

build-site: ## 构建主站（Felix Homelab 社区站）镜像
	$(REPO_DIR)/scripts/build-site.sh

build-whisper: ## 构建语音转文字（STT）镜像
	$(REPO_DIR)/scripts/build-whisper.sh

agent-build: ## 构建多租户 Agent 模板镜像（OpenCode）
	$(REPO_DIR)/scripts/build-agent-image.sh opencode

agent-build-dsh: ## 构建多租户 Agent 模板镜像（DeepSeek Harness）
	$(REPO_DIR)/scripts/build-agent-image.sh dsh

agent-build-zeroclaw: ## 拉取 ZeroClaw 官方 Agent 镜像
	$(REPO_DIR)/scripts/build-agent-image.sh zeroclaw

agent-build-kilocode: ## 构建多租户 Agent 模板镜像（Kilo Code）
	$(REPO_DIR)/scripts/build-agent-image.sh kilocode

agent-build-pi: ## 构建多租户 Agent 模板镜像（Pi）
	$(REPO_DIR)/scripts/build-agent-image.sh pi

agent-list: ## 列出多租户 Agent（用户/端口/状态）
	$(REPO_DIR)/scripts/agent-ctl.sh list

agent-apply: ## 立即处理 Agent 请求并保活（一般由 systemd 自动触发）
	$(REPO_DIR)/scripts/agent-ctl.sh tick

agent-stop: ## 停止某用户的 Agent：ARGS=<用户名>
	$(REPO_DIR)/scripts/agent-ctl.sh stop $(ARGS)

agent-remove: ## 删除某用户的 Agent 容器与子域路由（数据卷保留）：ARGS="<用户名> [slot]"
	$(REPO_DIR)/scripts/agent-ctl.sh remove $(ARGS)

agent-setkey: ## 设置 DSH 实例的 API Key（写入实例卷，不回显）：ARGS="<用户名> [slot] KEY=VALUE"
	$(REPO_DIR)/scripts/agent-ctl.sh setkey $(ARGS)

backup: ## 立即执行一次备份
	$(REPO_DIR)/scripts/backup-now.sh

backup-list: ## 列出备份文件
	@ls -lh "$${XDG_DATA_HOME:-$$HOME/.local/share}/felix-homelab/backups" 2>/dev/null || echo "暂无备份"

sync-backup: ## 把备份同步到异地（rclone/rsync，渠道见 backup.conf）
	$(REPO_DIR)/scripts/sync-backup.sh

restore: ## 从备份恢复（默认最新；可用 ARGS=<归档路径|latest>，附加 --yes 跳过确认）
	$(REPO_DIR)/scripts/restore-backup.sh $(ARGS)

deploy: ## 交互式部署/运维菜单（安装、备份、恢复、卸载）
	$(REPO_DIR)/scripts/deploy.sh

migrate: ## 从旧命名 Felix-Workstation 迁移到 Felix-Homelab（数据无损）
	$(REPO_DIR)/scripts/migrate-rename.sh

status: ## 查看状态
	systemctl --user status felix-homelab-pod.service --no-pager || true
	podman pod ps --filter name=Felix-Homelab
	podman ps --pod --filter pod=Felix-Homelab

logs: ## 跟踪 pod 日志
	journalctl --user -f -u felix-homelab-pod.service

restart: ## 重启所有容器（含 Pod）
	systemctl --user stop $(SERVICES) || true
	systemctl --user start $(SERVICES) || true

stop: ## 停止所有容器与 Pod
	systemctl --user stop $(SERVICES) felix-homelab-pod.service || true

start: ## 启动所有容器（Pod 会自动创建）
	systemctl --user start $(SERVICES) || true

doctor: ## 自检：Pod/单元/端口/Agent/定时器/错误日志
	@echo "=== 1. Pod ==="
	@podman pod ps --format "table {{.Name}}\t{{.Status}}" || true
	@podman ps --filter pod=Felix-Homelab --format "table {{.Names}}\t{{.Status}}" || true
	@echo "=== 2. Agent 网关与隧道 ==="
	@systemctl --user is-active felix-homelab-agent-gateway.service felix-homelab-agent-frpc.service 2>/dev/null || true
	@echo "=== 3. 回环端口 ==="
	@ss -tln 2>/dev/null | grep -E "127.0.0.1:(5729|5730|5731|5732|5733|5734|5735|5740)" || true
	@echo "=== 4. Agent 实例 ==="
	@$(REPO_DIR)/scripts/agent-ctl.sh list || true
	@echo "=== 5. 最近 1 小时错误日志 ==="
	@journalctl --user --since "1 hour ago" -p err --no-pager 2>/dev/null | tail -15 || true
	@echo "=== 6. 磁盘 ==="
	@df -h "$$HOME" | tail -1

uninstall: ## 停止并移除单元（保留数据）
	$(REPO_DIR)/scripts/uninstall.sh

purge: ## 停止、移除单元并删除数据卷（危险）
	$(REPO_DIR)/scripts/uninstall.sh --purge

prune: ## 清理构建残留（悬空镜像、构建工作容器）；**不删任何数据卷**
	-buildah rm --all >/dev/null 2>&1 || true
	-podman image prune -f
	@echo "提示：数据卷（含未使用的命名卷）一律手动确认后再删；本目标不做卷清理。"
