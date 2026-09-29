-- 光遇板块：代跑页展示设置（公告）与站内可编辑的官方内容（攻略/画廊）。
-- 社区投稿的加精标记放在 community_posts.meta JSON 的 featured 字段，无需改表。

CREATE TABLE IF NOT EXISTS sky_boosting (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL DEFAULT ''
);

INSERT OR IGNORE INTO sky_boosting (key, value) VALUES ('announcement', '');
INSERT OR IGNORE INTO sky_boosting (key, value) VALUES ('featured_reviews', '[]');

CREATE TABLE IF NOT EXISTS sky_official (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    category   TEXT NOT NULL,
    slug       TEXT NOT NULL,
    title      TEXT NOT NULL,
    summary    TEXT NOT NULL DEFAULT '',
    body_md    TEXT NOT NULL,
    body_html  TEXT NOT NULL,
    cover      TEXT NOT NULL DEFAULT '',
    sort       INTEGER NOT NULL DEFAULT 0,
    status     TEXT NOT NULL DEFAULT 'published',
    author_id  INTEGER,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE (category, slug)
);
