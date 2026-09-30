-- 资源政策：用户分组、全站免费期、并发上限与启动排队。
--
-- 背景（站长决策，2026-09-30）：站点暂定为个人/小团队使用，服务运行在个人
-- 电脑上，全站长期暂停收费（超级管理员可在后台手动设置起止）；新用户分组：
--   普通用户（normal）      —— 无特殊权益，仍可购买 OpenCloud/外置存储
--   开发者组（developer）   —— 并发实例不限、存储容量不限
--   管理员组（admin）       —— 最多同时 2 个运行实例、存储容量不限
-- 资源受限时启动/唤醒按「全局运行上限」排队。

ALTER TABLE users ADD COLUMN res_group TEXT NOT NULL DEFAULT 'normal';
UPDATE users SET res_group = 'admin' WHERE role = 'admin';

CREATE TABLE IF NOT EXISTS site_settings (
    key        TEXT PRIMARY KEY,
    value      TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- 默认：免费期从今天开始、不限结束；全局并发上限 5（宿主 31G 内存，实例 2G）。
INSERT OR IGNORE INTO site_settings (key, value) VALUES
    ('billing_paused', '1'),
    ('billing_pause_started_at', datetime('now')),
    ('billing_pause_ends_at', ''),
    ('global_running_cap', '5'),
    ('group_cap_normal', '1'),
    ('group_cap_admin', '2'),
    ('group_cap_developer', '0');

CREATE TABLE IF NOT EXISTS agent_queue (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id    INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    slot       INTEGER NOT NULL DEFAULT 1,
    subdomain  TEXT    NOT NULL,
    kind       TEXT    NOT NULL,
    status     TEXT    NOT NULL DEFAULT 'waiting'
                       CHECK (status IN ('waiting', 'started', 'cancelled', 'expired')),
    created_at TEXT    NOT NULL DEFAULT (datetime('now')),
    started_at TEXT,
    note       TEXT    NOT NULL DEFAULT ''
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_agent_queue_waiting
    ON agent_queue(user_id, slot) WHERE status = 'waiting';
CREATE INDEX IF NOT EXISTS idx_agent_queue_status ON agent_queue(status, id);
