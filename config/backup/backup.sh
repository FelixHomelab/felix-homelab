#!/bin/sh
# Felix-Homelab 备份脚本（容器内执行；只负责 Forgejo 与主站）
#
# 备份源与保留天数来自 /config/backup.conf（后台可编辑）：
#   BACKUP_FORGEJO=1 / BACKUP_SITE=1 / KEEP_DAYS=7
#
#
# rootless 说明：容器以 root 运行（映射宿主用户）负责落盘与属主修正；
# `forgejo dump` 用 s6-setuidgid 降权到 git 执行。
set -eu

BACKUP_DIR="${BACKUP_DIR:-/backups}"
CONF="${BACKUP_CONF:-/config/backup.conf}"
CONFIG="${FORGEJO_CONFIG:-/data/gitea/conf/app.ini}"
SITE_DIR="${SITE_DIR:-/site-data}"
PREFIX="felix-homelab-dump-"
SITE_PREFIX="felix-homelab-site-"

# 读取配置（缺失时用安全默认值）
if [ -f "$CONF" ]; then
	# shellcheck disable=SC1090
	. "$CONF"
fi
BACKUP_FORGEJO="${BACKUP_FORGEJO:-1}"
BACKUP_SITE="${BACKUP_SITE:-1}"
KEEP_DAYS="${KEEP_DAYS:-7}"

log() { printf '%s [backup] %s\n' "$(date '+%F %T')" "$*"; }

# 主站（主站）数据：SQLite + 上传图片，用 .backup 保证一致性
backup_site() {
	stamp="$1"
	[ -d "${SITE_DIR}" ] || { log "未挂载主站数据目录，跳过主站备份"; return 0; }
	[ -f "${SITE_DIR}/site.db" ] || { log "主站数据库尚未创建，跳过"; return 0; }

	name="${SITE_PREFIX}${stamp}.tar.gz"
	target="${BACKUP_DIR}/${name}"
	tmp="/tmp/sitebackup"

	log "开始备份主站数据 -> ${target}"
	rm -rf "${tmp}"
	mkdir -p "${tmp}"
	chown git:git "${tmp}"

	if ! s6-setuidgid git sqlite3 "${SITE_DIR}/site.db" ".backup '${tmp}/site.db'"; then
		log "错误：主站数据库备份失败"
		rm -f "${target}"
		return 1
	fi
	if [ -d "${SITE_DIR}/uploads" ]; then
		cp -a "${SITE_DIR}/uploads" "${tmp}/uploads" 2>/dev/null || true
	fi

	tar -czf "${target}" -C "${tmp}" .
	chown 0:0 "${target}" 2>/dev/null || true
	chmod 0644 "${target}"

	if ! tar tzf "${target}" 2>/dev/null | grep -q 'site\.db'; then
		log "错误：主站归档缺少 site.db，判定无效并删除"
		rm -f "${target}"
		return 1
	fi
	log "主站备份完成并校验通过（$(du -h "${target}" | cut -f1)）"

	find "${BACKUP_DIR}" -maxdepth 1 -type f \
		\( -name "${SITE_PREFIX}*.tar.gz" -o -name 'felix-ws-site-*.tar.gz' \) \
		-mtime "+${KEEP_DAYS}" | while read -r old; do
		rm -f "${old}"
		log "已清理过期主站备份 $(basename "${old}")"
	done
}

backup_forgejo() {
	stamp="$1"
	name="${PREFIX}${stamp}.tar.gz"
	target="${BACKUP_DIR}/${name}"
	tmp="/tmp/dump/${name}"

	log "开始备份 Forgejo -> ${target}"

	rm -rf /tmp/dump
	mkdir -p /tmp/dump
	chown git:git /tmp/dump

	# forgejo dump 要求仓库目录存在（全新实例还没有任何仓库）
	mkdir -p /data/git/repositories 2>/dev/null \
		|| s6-setuidgid git mkdir -p /data/git/repositories 2>/dev/null || true
	chown git:git /data/git/repositories 2>/dev/null || true

	if ! s6-setuidgid git forgejo dump \
		--config "${CONFIG}" \
		--file "${tmp}" \
		--type tar.gz \
		--skip-log; then
		log "错误：forgejo dump 执行失败"
		rm -f "${tmp}"
		return 1
	fi

	mv "${tmp}" "${target}"
	chown 0:0 "${target}" 2>/dev/null || true
	chmod 0644 "${target}"

	if ! list="$(tar tzf "${target}" 2>/dev/null)"; then
		log "错误：归档无法解析，判定无效并删除"
		rm -f "${target}"
		return 1
	fi
	for pattern in 'app\.ini' 'forgejo-db\.sql' '^repos/'; do
		if ! printf '%s\n' "${list}" | grep -q "$pattern"; then
			log "错误：归档中缺少 $pattern，判定无效并删除"
			rm -f "${target}"
			return 1
		fi
	done
	log "Forgejo 备份完成并校验通过（$(du -h "${target}" | cut -f1)）"

	find "${BACKUP_DIR}" -maxdepth 1 -type f \
		\( -name "${PREFIX}*.tar.gz" -o -name 'felix-ws-dump-*.tar.gz' \) \
		-mtime "+${KEEP_DAYS}" | while read -r old; do
		rm -f "${old}"
		log "已清理过期备份 $(basename "${old}")"
	done
}

do_backup() {
	stamp="$(date '+%F_%H%M%S')"

	if [ "${BACKUP_FORGEJO}" = "1" ]; then
		backup_forgejo "${stamp}" || log "Forgejo 备份失败"
	else
		log "备份源未启用：Forgejo（跳过）"
	fi

	if [ "${BACKUP_SITE}" = "1" ]; then
		backup_site "${stamp}" || log "主站备份失败"
	else
		log "备份源未启用：主站（跳过）"
	fi

	log "本次备份结束，备份目录共占用 $(du -sh "${BACKUP_DIR}" | cut -f1)"
}

mkdir -p "${BACKUP_DIR}"
do_backup
