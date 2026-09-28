-- ============================================================
-- Felix Homelab 社区站 — Agent 多实例（slot）
--
-- 同一用户可以有多个 Agent（slot 1..N），每个实例独立 kind / 备注 / 有效期：
--   后台给指定用户开通 N 个实例，用户首屏展示自己的全部入口。
--
-- 旧表 user_id 唯一约束需要重建表才能放开（SQLite 不能直接改约束）。
-- 旧数据一律视为 slot 1。
-- ============================================================

CREATE TABLE agent_subscriptions_new (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id     INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    slot        INTEGER NOT NULL DEFAULT 1,
    kind        TEXT    NOT NULL DEFAULT 'opencode'
                        CHECK (kind IN ('opencode', 'dsh')),
    status      TEXT    NOT NULL DEFAULT 'active'
                        CHECK (status IN ('active', 'stopped', 'revoked')),
    note        TEXT    NOT NULL DEFAULT '',
    expires_at  TEXT,
    created_at  TEXT    NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT    NOT NULL DEFAULT (datetime('now')),
    UNIQUE (user_id, slot)
);

INSERT INTO agent_subscriptions_new
    (id, user_id, slot, kind, status, note, expires_at, created_at, updated_at)
SELECT id, user_id, 1, kind, status, note, expires_at, created_at, updated_at
FROM agent_subscriptions;

DROP TABLE agent_subscriptions;
ALTER TABLE agent_subscriptions_new RENAME TO agent_subscriptions;

CREATE INDEX IF NOT EXISTS idx_agent_subscriptions_status
    ON agent_subscriptions(status);
CREATE INDEX IF NOT EXISTS idx_agent_subscriptions_user
    ON agent_subscriptions(user_id);
