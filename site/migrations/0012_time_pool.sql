-- AI Agent 时长池：充值/结转/消耗流水
--
-- 余额 = SUM(seconds)；正数为入账（充值、旧有效期结转、人工调整），
-- 负数为消耗（运行/睡眠/数据保存，计量系统落地后写入）。
CREATE TABLE IF NOT EXISTS time_pool_entries (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id    INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    seconds    INTEGER NOT NULL,
    kind       TEXT    NOT NULL CHECK (kind IN ('migrate', 'recharge', 'usage', 'retention', 'adjust')),
    note       TEXT    NOT NULL DEFAULT '',
    agent_id   INTEGER,
    created_at TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_time_pool_user_time
    ON time_pool_entries (user_id, created_at);

-- 标记「该实例的剩余有效期已折算进时长池」，结转例程据此幂等执行
ALTER TABLE agent_subscriptions ADD COLUMN pooled_at TEXT;
