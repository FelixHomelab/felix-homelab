# Felix-Workstation on Podman (Quadlet)
#
# 常用目标：
#   make install     安装并启动 Felix-Workstation pod
#   make register    向 Forgejo 注册 Actions runner 并启动
#   make backup      立即执行一次备份
#   make backup-list 列出备份文件
#   make restore     从备份恢复（默认最新）
#   make status      查看 pod / 容器状态
#   make logs        跟踪 pod 日志
#   make restart     重启 pod
#   make uninstall   停止并移除所有单元（保留数据卷，除非 make purge）

SHELL := /usr/bin/env bash
REPO_DIR := $(shell pwd)

# 容器服务；runner（注册后）与 frpc（配置中转后）可能不存在，故启动时忽略其错误
SERVICES := felix-workstation-db.service felix-workstation-forgejo.service \
	felix-workstation-site.service felix-workstation-nextcloud.service \
	felix-workstation-caddy.service felix-workstation-homepage.service \
	felix-workstation-runner.service felix-workstation-frpc.service \
	felix-workstation-backup.service felix-workstation-autoheal.service

.PHONY: install register build-images build-site status logs restart stop start \
	backup backup-list sync-backup restore deploy uninstall purge help

help:
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | \
		awk 'BEGIN{FS=":.*?## "}{printf "  \033[36m%-12s\033[0m %s\n", $$1, $$2}'

install: ## 安装并启动 Felix-Workstation
	$(REPO_DIR)/scripts/install.sh

register: ## 注册并启动 Forgejo Actions runner
	$(REPO_DIR)/scripts/register-runner.sh

build-images: ## 构建修复版 Runner 作业镜像（make register 会自动执行）
	$(REPO_DIR)/scripts/build-runner-images.sh

build-site: ## 构建个人主页（主站）镜像
	$(REPO_DIR)/scripts/build-site.sh

backup: ## 立即执行一次备份
	$(REPO_DIR)/scripts/backup-now.sh

backup-list: ## 列出备份文件
	@ls -lh "$${XDG_DATA_HOME:-$$HOME/.local/share}/felix-workstation/backups" 2>/dev/null || echo "暂无备份"

sync-backup: ## 把备份同步到异地（rclone/rsync，渠道见 backup.conf）
	$(REPO_DIR)/scripts/sync-backup.sh

restore: ## 从备份恢复（默认最新；可用 ARGS=<归档路径|latest>，附加 --yes 跳过确认）
	$(REPO_DIR)/scripts/restore-backup.sh $(ARGS)

deploy: ## 交互式部署/运维菜单（安装、备份、恢复、卸载）
	$(REPO_DIR)/scripts/deploy.sh

status: ## 查看状态
	systemctl --user status felix-workstation-pod.service --no-pager || true
	podman pod ps --filter name=Felix-Workstation
	podman ps --pod --filter pod=Felix-Workstation

logs: ## 跟踪 pod 日志
	journalctl --user -f -u felix-workstation-pod.service

restart: ## 重启所有容器（含 Pod）
	systemctl --user stop $(SERVICES) || true
	systemctl --user start $(SERVICES) || true

stop: ## 停止所有容器与 Pod
	systemctl --user stop $(SERVICES) felix-workstation-pod.service || true

start: ## 启动所有容器（Pod 会自动创建）
	systemctl --user start $(SERVICES) || true

uninstall: ## 停止并移除单元（保留数据）
	$(REPO_DIR)/scripts/uninstall.sh

purge: ## 停止、移除单元并删除数据卷（危险）
	$(REPO_DIR)/scripts/uninstall.sh --purge
