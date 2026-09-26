#!/usr/bin/env bash
#
# 从备份归档恢复 Felix-Workstation 的 Forgejo
#
# 用法：
#   scripts/restore-backup.sh [归档路径|latest] [--yes] [--skip-safety]
#
# 流程：
#   1. （默认）先做一次当前状态的安全备份
#   2. 停止 Forgejo / Runner / 备份容器
#   3. 重建 PostgreSQL 数据库并导入归档中的 forgejo-db.sql
#   4. 还原数据卷中的 data/ 与 repos/
#   5. 启动服务、等待健康、重新注册 Runner
#
# 注意：会覆盖当前 Forgejo 数据，请确认归档无误。
#
set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BACKUP_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/felix-workstation/backups"
FORGEJO_CONTAINER="felix-workstation-forgejo"
DB_CONTAINER="felix-workstation-db"
FORGEJO_IMAGE="codeberg.org/forgejo/forgejo:16"

ARCHIVE=""
ASSUME_YES=0
SKIP_SAFETY=0

for arg in "$@"; do
	case "$arg" in
		--yes | -y) ASSUME_YES=1 ;;
		--skip-safety) SKIP_SAFETY=1 ;;
		*) ARCHIVE="$arg" ;;
	esac
done

log()  { printf '\033[1;36m[felix]\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m[felix]\033[0m %s\n' "$*" >&2; }
die()  { printf '\033[1;31m[felix]\033[0m %s\n' "$*" >&2; exit 1; }

# 1. 选择归档
if [ -z "$ARCHIVE" ] || [ "$ARCHIVE" = "latest" ]; then
	ARCHIVE="$(find "$BACKUP_DIR" -maxdepth 1 -name 'felix-ws-dump-*.tar.gz' -type f | sort | tail -n1)"
fi
[ -n "$ARCHIVE" ] && [ -f "$ARCHIVE" ] || die "找不到备份归档（目录: $BACKUP_DIR）"
log "使用归档: $ARCHIVE"

# 2. 校验归档（用 case 匹配，避免 pipefail 下 printf|grep 的 SIGPIPE 误判）
list="$(tar tzf "$ARCHIVE")" || die "归档无法解析"
case "$list" in *"app.ini"*) ;; *) die "归档缺少 app.ini" ;; esac
case "$list" in *"forgejo-db.sql"*) ;; *) die "归档缺少数据库导出 forgejo-db.sql" ;; esac
case "$list" in
	"repos/"* | *$'\n'"repos/"*) ;;
	*) die "归档缺少 repos/（Git 仓库本体）" ;;
esac
log "归档校验通过（$(printf '%s\n' "$list" | wc -l) 个条目）"

if [ "$ASSUME_YES" -ne 1 ]; then
	warn "此操作会用归档覆盖当前 Forgejo（数据库 + 仓库 + 附件），且不可撤销！"
	printf '输入 yes 继续: '
	read -r reply
	[ "$reply" = "yes" ] || die "已取消"
fi

# 3. 先做当前状态的安全备份
if [ "$SKIP_SAFETY" -ne 1 ]; then
	if podman healthcheck run "$FORGEJO_CONTAINER" >/dev/null 2>&1; then
		log "恢复前先备份当前状态..."
		"$REPO_DIR/scripts/backup-now.sh" >/dev/null || warn "安全备份失败，继续恢复"
	else
		warn "Forgejo 当前不健康，跳过安全备份"
	fi
fi

# 4. 停止写入方
log "停止 Forgejo / Runner / 备份服务"
systemctl --user stop felix-workstation-runner.service felix-workstation-backup.service 2>/dev/null || true
systemctl --user stop felix-workstation-forgejo.service 2>/dev/null || true

if ! podman container exists "$DB_CONTAINER"; then
	die "数据库容器未运行，请先 make install"
fi

# 5. 恢复数据库
log "重建 PostgreSQL 数据库并导入 forgejo-db.sql"
tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT
tar -xzf "$ARCHIVE" -C "$tmpdir" forgejo-db.sql

podman exec "$DB_CONTAINER" psql -U forgejo -d postgres -v ON_ERROR_STOP=1 \
	-c "DROP DATABASE IF EXISTS forgejo WITH (FORCE);"
podman exec "$DB_CONTAINER" psql -U forgejo -d postgres -v ON_ERROR_STOP=1 \
	-c "CREATE DATABASE forgejo OWNER forgejo;"

podman cp "$tmpdir/forgejo-db.sql" "$DB_CONTAINER:/tmp/felix-restore.sql"
podman exec "$DB_CONTAINER" psql -U forgejo -d forgejo \
	-f /tmp/felix-restore.sql >"$tmpdir/import.log" 2>&1 || true
podman exec "$DB_CONTAINER" rm -f /tmp/felix-restore.sql
errs="$(grep -ciE '^ERROR' "$tmpdir/import.log" || true)"
rows="$(grep -c '^INSERT' "$tmpdir/import.log" 2>/dev/null || true)"
log "数据库导入完成（INSERT 语句 ${rows:-0} 条，ERROR ${errs:-0} 条）"
if [ "${errs:-0}" -gt 0 ]; then
	warn "导入过程中有 $errs 条错误，日志：$tmpdir/import.log（已随临时目录清理）"
fi

# 6. 恢复数据卷文件（data/ 与 repos/）
#    以 root 运行：先把待删目录 chown 给自己以便删除，最后再 chown 回 uid 1000
#    （rootless 下容器 root 映射为宿主用户，具备 user namespace 内的 CAP_CHOWN）
log "还原 data/ 与 repos/ 到数据卷"
podman run --rm --pod Felix-Workstation --user 0 --security-opt label=disable \
	-v felix-workstation-forgejo-data:/data \
	-v "$BACKUP_DIR":/backup:ro \
	--entrypoint sh "$FORGEJO_IMAGE" -c '
		set -e
		rm -rf /tmp/r && mkdir -p /tmp/r
		tar -xzf "/backup/'"$(basename "$ARCHIVE")"'" -C /tmp/r
		chown -R 0:0 /data/gitea /data/git 2>/dev/null || true
		rm -rf /data/gitea /data/git/repositories
		mkdir -p /data/gitea /data/git/repositories
		cp -a /tmp/r/data/. /data/gitea/
		cp -a /tmp/r/app.ini /data/gitea/conf/app.ini
		cp -a /tmp/r/repos/. /data/git/repositories/
		chown -R 1000:1000 /data/gitea /data/git
	'

# 6b. 恢复主站数据（若存在同时间戳的 felix-ws-site-*.tar.gz）
stamp="$(basename "$ARCHIVE")"
stamp="${stamp#felix-ws-dump-}"
stamp="${stamp%.tar.gz}"
site_archive="$BACKUP_DIR/felix-ws-site-${stamp}.tar.gz"
if [ -f "$site_archive" ]; then
	log "还原主站数据（$(basename "$site_archive")）"
	systemctl --user stop felix-workstation-site.service 2>/dev/null || true
	podman run --rm --pod Felix-Workstation --user 1000 --security-opt label=disable \
		-v felix-workstation-site-data:/data \
		-v "$BACKUP_DIR":/backup:ro \
		--entrypoint sh "$FORGEJO_IMAGE" -c '
			set -e
			rm -rf /tmp/s && mkdir -p /tmp/s
			tar -xzf "/backup/'"$(basename "$site_archive")"'" -C /tmp/s
			rm -rf /data/site.db /data/site.db-wal /data/site.db-shm /data/uploads
			cp -a /tmp/s/site.db /data/site.db
			[ -d /tmp/s/uploads ] && cp -a /tmp/s/uploads /data/uploads || true
		'
else
	warn "未找到配对的主站备份 $site_archive，跳过主站数据恢复"
fi

# 7. 启动并等待
log "启动服务"
systemctl --user start felix-workstation-db.service
systemctl --user start felix-workstation-forgejo.service
systemctl --user start felix-workstation-site.service
for _ in $(seq 1 60); do
	podman healthcheck run "$FORGEJO_CONTAINER" >/dev/null 2>&1 && break
	sleep 2
done
podman healthcheck run "$FORGEJO_CONTAINER" >/dev/null 2>&1 \
	|| die "Forgejo 恢复后未能变为健康，请查看 journalctl --user -u felix-workstation-forgejo"

log "重新注册 Runner"
"$REPO_DIR/scripts/register-runner.sh" >/dev/null
systemctl --user start felix-workstation-backup.service

log "恢复完成。数据核对："
podman exec "$DB_CONTAINER" psql -U forgejo -d forgejo -tAc \
	"select '用户 '||count(*) from \"user\" union all select '仓库 '||count(*) from repository union all select 'Actions '||count(*) from action_run;"
