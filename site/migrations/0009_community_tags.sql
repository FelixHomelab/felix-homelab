-- 社区标签元数据：预置分类（preset）与管理状态（display_name 重命名、pinned 置顶）。
-- 标签本身仍以 JSON 数组存在 community_posts.tags；这里只存“有名字/有状态”的标签。
CREATE TABLE IF NOT EXISTS community_tags (
    tag          TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    preset       INTEGER NOT NULL DEFAULT 0,
    pinned       INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT OR IGNORE INTO community_tags (tag, display_name, preset) VALUES
    ('公告', '公告', 1),
    ('技术', '技术', 1),
    ('创作', '创作', 1),
    ('闲聊', '闲聊', 1),
    ('资源', '资源', 1),
    ('问答', '问答', 1);
