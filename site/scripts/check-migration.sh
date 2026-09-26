#!/usr/bin/env bash
# 迁移回归：0002（用户名大小写不敏感）在**已有重名账号**的库上是否真的合干净。
#
# 为什么值得单独测：这条迁移要做两件都不好回头的事——合并账号、建唯一索引。
# 顺序错了（先建索引后合并）会直接让服务起不来；引用搬漏了会静默丢评论；
# 外键关着跑更糟，什么都看不出来。所以这里**开着 foreign_keys 造一份真重名的库**，
# 走一遍迁移再逐条核对。
#
# 用法：./scripts/check-migration.sh
# 退出码：0 全部断言通过；1 有断言失败。
set -uo pipefail

MIG_DIR="$(cd "$(dirname "$0")/.." && pwd)/migrations"
DB="$(mktemp -u /tmp/gf-mig-XXXXXX.db)"
trap 'rm -f "$DB" "$DB-wal" "$DB-shm"' EXIT

fail=0
# 断言相等：check <说明> <期望> <实际>
check() {
  if [ "$2" = "$3" ]; then
    printf '  ok   %s = %s\n' "$1" "$3"
  else
    printf '  FAIL %s：期望 %s，实际 %s\n' "$1" "$2" "$3"
    fail=$((fail + 1))
  fi
}
q() { sqlite3 "$DB" "$1"; }

echo "== 造数据：0001 之后、0002 之前的状态 =="
sqlite3 "$DB" < "$MIG_DIR/0001_init.sql"
# foreign_keys 必须打开：关着的话「引用搬漏 + 级联删除」会被掩盖成「看起来没问题」
sqlite3 "$DB" <<'SQL'
PRAGMA foreign_keys = ON;
INSERT INTO users (id, username, password_hash, display_name, role, status) VALUES
  (1, 'Felix',  'h1', ' Felix 自己挑的写法', 'user',  'active'),
  (2, 'felix',  'h2', '小写那个',           'admin', 'active'),
  (3, 'NeBula', 'h3', '甲',                 'user',  'active'),
  (4, 'nebula', 'h4', '乙',                 'user',  'banned'),
  (5, 'solo',   'h5', '独苗',               'user',  'active');
INSERT INTO sessions (user_id, token_hash, expires_at) VALUES (2, 'tok', '2030-01-01');
INSERT INTO comments (target_kind, target_slug, user_id, body_md, body_html, status) VALUES
  ('post', 'x', 2, 'a', '<p>a</p>', 'approved'),
  ('post', 'x', 4, 'b', '<p>b</p>', 'approved');
INSERT INTO sky_reviews (user_id, rating, body, status) VALUES (2, 5, '好', 'approved');
INSERT INTO user_prefs (user_id, theme_mode) VALUES (1, 'dark'), (2, 'light'), (3, 'dark');
SQL
check "造出的账号数" 5 "$(q 'SELECT COUNT(*) FROM users;')"

echo "== 跑 0002 =="
# 迁移由 sqlx 在事务里执行；这里显式开一个事务，尽量贴近真实执行方式
{ echo 'PRAGMA foreign_keys = ON;'; echo 'BEGIN;'; cat "$MIG_DIR/0002_username_nocase.sql"; echo 'COMMIT;'; } \
  | sqlite3 "$DB" || { echo "  FAIL 迁移自身报错"; fail=$((fail + 1)); }

echo "== 核对合并结果 =="
check "合并后账号数" 3 "$(q 'SELECT COUNT(*) FROM users;')"
check "Felix 组保留 id 最小者" 1 "$(q "SELECT id FROM users WHERE username COLLATE NOCASE = 'felix';")"
check "保留自己挑的写法" 'Felix' "$(q "SELECT username FROM users WHERE id = 1;")"
check "管理员角色被提升" 'admin' "$(q 'SELECT role FROM users WHERE id = 1;')"
check "封禁状态被提升" 'banned' "$(q "SELECT status FROM users WHERE username = 'NeBula';")"
check "各自的昵称没被换掉" ' Felix 自己挑的写法' "$(q 'SELECT display_name FROM users WHERE id = 1;')"
check "不相关的账号没被动过" 'solo' "$(q "SELECT username FROM users WHERE id = 5;")"

echo "== 核对引用有没有搬干净 =="
check "会话改挂保留者" 1 "$(q 'SELECT COUNT(*) FROM sessions WHERE user_id = 1;')"
check "评论一条没丢" 2 "$(q 'SELECT COUNT(*) FROM comments;')"
# 两组各自搬给自己的保留者：`felix`(2) 名下的评论归 1，`nebula`(4) 名下的归 3
check "评论按组归位到 1" 1 "$(q 'SELECT COUNT(*) FROM comments WHERE user_id = 1;')"
check "评论按组归位到 3" 1 "$(q 'SELECT COUNT(*) FROM comments WHERE user_id = 3;')"
check "评价改挂保留者" 1 "$(q 'SELECT COUNT(*) FROM sky_reviews WHERE user_id = 1;')"
check "偏好：保留者自己那行留下" 'dark' "$(q 'SELECT theme_mode FROM user_prefs WHERE user_id = 1;')"
check "偏好：行数与存活账号匹配" 2 "$(q 'SELECT COUNT(*) FROM user_prefs;')"
check "没有指向已删账号的孤儿引用" 0 "$(q '
  SELECT (SELECT COUNT(*) FROM comments     WHERE user_id NOT IN (SELECT id FROM users))
       + (SELECT COUNT(*) FROM sessions     WHERE user_id NOT IN (SELECT id FROM users))
       + (SELECT COUNT(*) FROM sky_reviews  WHERE user_id NOT IN (SELECT id FROM users))
       + (SELECT COUNT(*) FROM user_prefs   WHERE user_id NOT IN (SELECT id FROM users));')"

echo "== 核对唯一索引真的挡住了大小写变体 =="
check "索引已建立" 1 "$(q "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='idx_users_username_nocase';")"
if sqlite3 "$DB" "INSERT INTO users (username, password_hash, display_name) VALUES ('FELIX', 'h', 'x');" 2>/dev/null; then
  echo "  FAIL 大小写变体 FELIX 竟然插进去了"
  fail=$((fail + 1))
else
  echo "  ok   大小写变体 FELIX 被唯一索引拒绝"
fi
if sqlite3 "$DB" "INSERT INTO users (username, password_hash, display_name) VALUES ('nebula', 'h', 'x');" 2>/dev/null; then
  echo "  FAIL 大小写变体 nebula 竟然插进去了"
  fail=$((fail + 1))
else
  echo "  ok   大小写变体 nebula 被唯一索引拒绝"
fi
check "合法的全新用户名仍可注册" 4 "$(sqlite3 "$DB" "INSERT INTO users (username, password_hash, display_name) VALUES ('Newcomer', 'h', 'x'); SELECT COUNT(*) FROM users;" 2>/dev/null)"

echo
if [ "$fail" -eq 0 ]; then
  echo "全部通过"
  exit 0
fi
echo "不合格项：$fail"
exit 1
