#!/usr/bin/env bash
#
# Felix-Homelab 交互式部署/运维入口
#
# 用法：
#   scripts/deploy.sh              交互式菜单
#   scripts/deploy.sh install      安装/更新
#   scripts/deploy.sh uninstall    卸载（保留数据）
#   scripts/deploy.sh purge        彻底清除（含数据卷）
#   scripts/deploy.sh backup       立即备份
#   scripts/deploy.sh sync         备份异地同步
#   scripts/deploy.sh restore      从备份恢复
#   scripts/deploy.sh register     注册/重启 Runner
#   scripts/deploy.sh status       查看状态
#   scripts/deploy.sh migrate      旧命名迁移（Felix-Workstation → Felix-Homelab）
#
# 菜单只是入口，实际动作复用 scripts/ 下的幂等脚本，便于自动化调用。
#
set -uo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BACKUP_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/felix-homelab/backups"

C_TITLE=$'\033[1;35m'
C_OK=$'\033[1;32m'
C_WARN=$'\033[1;33m'
C_ERR=$'\033[1;31m'
C_DIM=$'\033[2m'
C_OFF=$'\033[0m'

title() {
	printf '%s' "$C_TITLE"
	printf '\n  Felix-Homelab  部署与运维\n'
	printf '%s' "$C_OFF"
	printf '%s  ──────────────────────────────────────────────%s\n' "$C_DIM" "$C_OFF"
}

pause() {
	printf '\n%s按回车返回菜单...%s' "$C_DIM" "$C_OFF"
	read -r _
}

do_install()  { title; echo "  ▸ 安装/更新"; echo; "$REPO_DIR/scripts/install.sh"; }
do_status() {
	title; echo "  ▸ 状态"; echo
	systemctl --user --no-pager status felix-homelab-pod.service 2>/dev/null | head -n 12 || true
	echo
	podman pod ps --filter name=Felix-Homelab
	podman ps --pod --filter pod=Felix-Homelab --format 'table {{.Names}}\t{{.Status}}'
}
do_backup()   { title; echo "  ▸ 立即备份"; echo; "$REPO_DIR/scripts/backup-now.sh"; }
do_sync()     { title; echo "  ▸ 备份异地同步"; echo; "$REPO_DIR/scripts/sync-backup.sh"; }
do_restore()  { title; echo "  ▸ 从备份恢复（默认最新，接下来会请求确认）"; echo; "$REPO_DIR/scripts/restore-backup.sh" latest; }
do_register() { title; echo "  ▸ 注册/重启 Runner"; echo; "$REPO_DIR/scripts/register-runner.sh"; }
do_migrate()  { title; echo "  ▸ 旧命名迁移（Felix-Workstation → Felix-Homelab）"; echo; "$REPO_DIR/scripts/migrate-rename.sh"; }
do_uninstall(){ title; echo "  ▸ 卸载（保留数据卷与备份）"; echo; "$REPO_DIR/scripts/uninstall.sh"; }
do_purge() {
	title
	printf '  %s⚠ 彻底清除会删除数据卷与配置（备份目录保留）%s\n\n' "$C_ERR" "$C_OFF"
	printf '  输入 DELETE 确认: '
	read -r ans
	[ "$ans" = "DELETE" ] || { echo "已取消"; return 0; }
	"$REPO_DIR/scripts/uninstall.sh" --purge
}

menu() {
	while true; do
		title
		cat <<EOF

   1) 安装 / 更新            2) 查看状态
   3) 立即备份              4) 备份异地同步
   5) 从备份恢复            6) 注册 / 重启 Runner
   7) 卸载（保留数据）      8) 彻底清除（含数据卷）
   9) 旧命名迁移（Felix-Workstation → Felix-Homelab）
   0) 退出
EOF
		printf '\n  选择: '
		read -r choice
		case "$choice" in
			1) do_install; pause ;;
			2) do_status; pause ;;
			3) do_backup; pause ;;
			4) do_sync; pause ;;
			5) do_restore; pause ;;
			6) do_register; pause ;;
			7) do_uninstall; pause ;;
			8) do_purge; pause ;;
			9) do_migrate; pause ;;
			0 | q | Q) printf '\n  再见。\n\n'; exit 0 ;;
			*) printf '\n  %s无效选项%s\n' "$C_WARN" "$C_OFF"; sleep 1 ;;
		esac
	done
}

usage() {
	sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'
}

case "${1:-menu}" in
	install | update) do_install ;;
	status) do_status ;;
	backup) do_backup ;;
	sync) do_sync ;;
	restore) do_restore ;;
	register) do_register ;;
	migrate) do_migrate ;;
	uninstall) do_uninstall ;;
	purge) do_purge ;;
	menu) menu ;;
	help | -h | --help) usage ;;
	*) printf '%s未知命令: %s%s\n\n' "$C_ERR" "$1" "$C_OFF"; usage; exit 1 ;;
esac
