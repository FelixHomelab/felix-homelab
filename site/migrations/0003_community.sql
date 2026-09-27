-- ============================================================
-- Felix Homelab 社区站 — 社区投稿（UGC）
--
-- 与仓库里的官方内容（Markdown）分开：社区内容由注册用户直接发布，
-- 默认 published（直接发布 + 事后管理），管理员可 hidden 或删除。
--
--   kind: post（文章）/ project（项目）/ sky（光遇）
--   meta: 按 kind 存放附加字段（JSON 对象）
--         project -> {"project_kind","repo","demo"}
--         sky     -> {"category","cover"}
-- ============================================================

CREATE TABLE IF NOT EXISTS community_posts (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    kind        TEXT    NOT NULL CHECK (kind IN ('post', 'project', 'sky')),
    slug        TEXT    NOT NULL,
    title       TEXT    NOT NULL,
    summary     TEXT    NOT NULL DEFAULT '',
    tags        TEXT    NOT NULL DEFAULT '[]',
    body_md     TEXT    NOT NULL,
    body_html   TEXT    NOT NULL,
    meta        TEXT    NOT NULL DEFAULT '{}',
    author_id   INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    status      TEXT    NOT NULL DEFAULT 'published'
                        CHECK (status IN ('published', 'hidden')),
    created_at  TEXT    NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT    NOT NULL DEFAULT (datetime('now')),
    UNIQUE (author_id, slug)
);

CREATE INDEX IF NOT EXISTS idx_community_kind
    ON community_posts(kind, status, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_community_author
    ON community_posts(author_id);

-- 评论目标类型扩展到 community。SQLite 不能直接修改 CHECK 约束，
-- 需要重建表并搬运数据（保留 id 与外键关系）。
CREATE TABLE comments_new (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    target_kind     TEXT    NOT NULL CHECK (target_kind IN ('post', 'sky', 'community')),
    target_slug     TEXT    NOT NULL,
    user_id         INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    parent_id       INTEGER REFERENCES comments_new(id) ON DELETE CASCADE,
    body_md         TEXT    NOT NULL,
    body_html       TEXT    NOT NULL,
    status          TEXT    NOT NULL DEFAULT 'pending'
                            CHECK (status IN ('pending', 'approved', 'rejected')),
    created_at      TEXT    NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO comments_new
    (id, target_kind, target_slug, user_id, parent_id, body_md, body_html, status, created_at)
SELECT id, target_kind, target_slug, user_id, parent_id, body_md, body_html, status, created_at
FROM comments;

DROP TABLE comments;
ALTER TABLE comments_new RENAME TO comments;

CREATE INDEX IF NOT EXISTS idx_comments_target
    ON comments(target_kind, target_slug, status);
