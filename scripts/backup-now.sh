#!/usr/bin/env bash
#
# 立即执行一次备份（复用宿主机 systemd 服务：Forgejo/主站）
#
set -euo pipefail

BACKUP_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/felix-homelab/backups"

systemctl --user start felix-homelab-backup-run.service

echo
echo "备份文件位于: $BACKUP_DIR"
ls -lh "$BACKUP_DIR" 2>/dev/null | tail -5
