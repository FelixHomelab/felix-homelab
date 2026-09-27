-- ============================================================
-- Grant Felix Homepage — 初始化迁移
--
-- 只建「用户产生的东西」：账号、会话、评论、评价、外观偏好。
-- 内容（文章、项目）一律以 Markdown 存在仓库里，不进数据库。
-- 设计见 DESIGN.md 第四节、字段说明见 TODO.md 的 DDL 表。
-- ============================================================

-- 账号。email 允许为空：本站不发邮件，收邮箱没有意义。
CREATE TABLE IF NOT EXISTS users (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    username        TEXT    NOT NULL UNIQUE,
    password_hash   TEXT    NOT NULL,
    display_name    TEXT    NOT NULL,
    email           TEXT,
    bio             TEXT,
    role            TEXT    NOT NULL DEFAULT 'user'
                            CHECK (role IN ('admin', 'user')),
    status          TEXT    NOT NULL DEFAULT 'active'
                            CHECK (status IN ('active', 'banned')),
    created_at      TEXT    NOT NULL DEFAULT (datetime('now')),
    last_login_at   TEXT
);

-- 会话。存的是令牌的 SHA-256，不是明文：库文件万一泄露也无法直接拿来登录。
CREATE TABLE IF NOT EXISTS sessions (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id         INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash      TEXT    NOT NULL UNIQUE,
    expires_at      TEXT    NOT NULL,
    created_at      TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_sessions_user ON sessions(user_id);

-- 评论。target_kind + target_slug 组成多态目标，博客与光遇共用一张表与一套审核逻辑。
-- 新评论一律先落 pending：开放注册的站点必然会被灌水。
CREATE TABLE IF NOT EXISTS comments (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    target_kind     TEXT    NOT NULL CHECK (target_kind IN ('post', 'sky')),
    target_slug     TEXT    NOT NULL,
    user_id         INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    parent_id       INTEGER REFERENCES comments(id) ON DELETE CASCADE,
    body_md         TEXT    NOT NULL,
    body_html       TEXT    NOT NULL,
    status          TEXT    NOT NULL DEFAULT 'pending'
                            CHECK (status IN ('pending', 'approved', 'rejected')),
    created_at      TEXT    NOT NULL DEFAULT (datetime('now'))
);

-- 前台按目标查已批准评论，走这条联合索引
CREATE INDEX IF NOT EXISTS idx_comments_target
    ON comments(target_kind, target_slug, status);

-- 光遇代跑评价
CREATE TABLE IF NOT EXISTS sky_reviews (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id         INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    rating          INTEGER NOT NULL CHECK (rating BETWEEN 1 AND 5),
    body            TEXT    NOT NULL,
    reply           TEXT,
    status          TEXT    NOT NULL DEFAULT 'pending'
                            CHECK (status IN ('pending', 'approved', 'rejected')),
    created_at      TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_sky_reviews_status ON sky_reviews(status);

-- 外观偏好。与 users 一对一；单独一张表是为了让「未登录访客的偏好」
-- 能用同一套结构，不必为匿名用户另立模型。
-- bg_kind: none（无背景图）/ url（外链）/ upload（本地上传）
CREATE TABLE IF NOT EXISTS user_prefs (
    user_id         INTEGER PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    theme_mode      TEXT    NOT NULL DEFAULT 'auto'
                            CHECK (theme_mode IN ('auto', 'light', 'dark')),
    accent          TEXT,
    bg_kind         TEXT    NOT NULL DEFAULT 'none'
                            CHECK (bg_kind IN ('none', 'url', 'upload')),
    bg_value        TEXT,
    updated_at      TEXT    NOT NULL DEFAULT (datetime('now'))
);
