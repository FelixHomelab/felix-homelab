-- Agent 类型扩展：新增 openclaw / kilocode / pi
-- SQLite 不能修改 CHECK：重建 agent_subscriptions（保留全部列与索引）。
CREATE TABLE agent_subscriptions_new (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id     INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    slot        INTEGER NOT NULL DEFAULT 1,
    kind        TEXT    NOT NULL DEFAULT 'opencode'
                        CHECK (kind IN ('opencode', 'dsh', 'openclaw', 'kilocode', 'pi')),
    status      TEXT    NOT NULL DEFAULT 'active'
                        CHECK (status IN ('active', 'stopped', 'revoked')),
    note        TEXT    NOT NULL DEFAULT '',
    expires_at  TEXT,
    created_at  TEXT    NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT    NOT NULL DEFAULT (datetime('now')),
    subdomain   TEXT,
    revoked_at  TEXT,
    purged_at   TEXT,
    pooled_at   TEXT,
    UNIQUE (user_id, slot)
);

INSERT INTO agent_subscriptions_new
    (id, user_id, slot, kind, status, note, expires_at, created_at, updated_at,
     subdomain, revoked_at, purged_at, pooled_at)
SELECT id, user_id, slot, kind, status, note, expires_at, created_at, updated_at,
       subdomain, revoked_at, purged_at, pooled_at
FROM agent_subscriptions;

DROP TABLE agent_subscriptions;
ALTER TABLE agent_subscriptions_new RENAME TO agent_subscriptions;

CREATE INDEX IF NOT EXISTS idx_agent_subscriptions_status ON agent_subscriptions(status);
CREATE INDEX IF NOT EXISTS idx_agent_subscriptions_user ON agent_subscriptions(user_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_agent_subscriptions_subdomain
    ON agent_subscriptions(subdomain) WHERE subdomain IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_agent_subscriptions_purged
    ON agent_subscriptions(purged_at) WHERE purged_at IS NOT NULL;
