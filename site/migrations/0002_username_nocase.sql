-- ============================================================
-- 用户名大小写不敏感：一个人只能有一个账号
--
-- 为什么不在写入时把用户名统一转小写：原始大小写是用户自己挑的写法，展示时
-- 该保留。约束唯一性与查找只需要一个「大小写折叠后」的索引与比较，存储照旧。
--
-- NOCASE 只折叠 ASCII 的 A–Z——这对本站够用，因为用户名的字符集本来就限定在
-- ASCII（见 auth.rs 的 validate_credentials）。哪天放开非 ASCII 用户名，这里
-- 必须跟着换，否则「Ä」与「ä」又会变成两个账号。
-- ============================================================

-- ---------- 一、先合并已有的重名账号 ----------
--
-- 不先合并就直接建唯一索引会失败，而迁移失败的表现是「服务起不来」。
-- 合并规则是确定性的，不需要人工挑：
--   * 保留 id 最小的那个（最早注册的），用户名写法用它的；
--   * 会话、评论、评价都改挂到保留者名下；
--   * 外观偏好一人一行：保留者已有就用保留者的，否则把被合并方的搬过来；
--   * 角色取「组里更强的」——组里有人是管理员，保留者就是管理员；
--   * 状态同理取封禁——否则重名账号能绕开封禁；
--   * 密码哈希只能留保留者的（哈希不可合并），被合并方的原密码从此不能登录。
CREATE TEMP TABLE _merge_dup AS
SELECT lower(username) AS uname, MIN(id) AS keep_id
FROM users
GROUP BY lower(username)
HAVING COUNT(*) > 1;

CREATE TEMP TABLE _merge_drop AS
SELECT u.id AS drop_id, d.keep_id
FROM users u
JOIN _merge_dup d ON lower(u.username) = d.uname
WHERE u.id <> d.keep_id;

UPDATE sessions
   SET user_id = (SELECT keep_id FROM _merge_drop WHERE drop_id = sessions.user_id)
 WHERE user_id IN (SELECT drop_id FROM _merge_drop);

UPDATE comments
   SET user_id = (SELECT keep_id FROM _merge_drop WHERE drop_id = comments.user_id)
 WHERE user_id IN (SELECT drop_id FROM _merge_drop);

UPDATE sky_reviews
   SET user_id = (SELECT keep_id FROM _merge_drop WHERE drop_id = sky_reviews.user_id)
 WHERE user_id IN (SELECT drop_id FROM _merge_drop);

DELETE FROM user_prefs
 WHERE user_id IN (SELECT drop_id FROM _merge_drop)
   AND (SELECT keep_id FROM _merge_drop WHERE drop_id = user_prefs.user_id)
       IN (SELECT user_id FROM user_prefs);

UPDATE user_prefs
   SET user_id = (SELECT keep_id FROM _merge_drop WHERE drop_id = user_prefs.user_id)
 WHERE user_id IN (SELECT drop_id FROM _merge_drop);

UPDATE users
   SET role = 'admin'
 WHERE id IN (
     SELECT d.keep_id FROM _merge_dup d
     JOIN users u ON lower(u.username) = d.uname
     WHERE u.role = 'admin'
 );

UPDATE users
   SET status = 'banned'
 WHERE id IN (
     SELECT d.keep_id FROM _merge_dup d
     JOIN users u ON lower(u.username) = d.uname
     WHERE u.status = 'banned'
 );

DELETE FROM users WHERE id IN (SELECT drop_id FROM _merge_drop);

DROP TABLE _merge_drop;
DROP TABLE _merge_dup;

-- ---------- 二、大小写折叠后唯一 ----------
--
-- users.username 上原来那个 UNIQUE 仍在（大小写敏感的那个）。它更弱、成了冗余，
-- 但 SQLite 要拿掉列上的约束得重建整张表，为这点冗余不值当。
CREATE UNIQUE INDEX IF NOT EXISTS idx_users_username_nocase
    ON users(username COLLATE NOCASE);
