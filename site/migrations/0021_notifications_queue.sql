-- 通知类型扩展：新增 queue（启动排队：已轮到 / 排队超时 / 被取消）。
-- SQLite 不能改 CHECK，重建表并保留数据与索引。

CREATE TABLE notifications_new (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id    INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    kind       TEXT    NOT NULL CHECK (kind IN ('expiry', 'archive', 'system', 'queue')),
    title      TEXT    NOT NULL,
    body       TEXT    NOT NULL DEFAULT '',
    link       TEXT    NOT NULL DEFAULT '',
    dedupe_key TEXT    NOT NULL DEFAULT '',
    read_at    TEXT,
    created_at TEXT    NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO notifications_new
    (id, user_id, kind, title, body, link, dedupe_key, read_at, created_at)
SELECT id, user_id, kind, title, body, link, dedupe_key, read_at, created_at
FROM notifications;

DROP TABLE notifications;
ALTER TABLE notifications_new RENAME TO notifications;

CREATE UNIQUE INDEX IF NOT EXISTS idx_notifications_dedupe
    ON notifications (user_id, dedupe_key) WHERE dedupe_key != '';
CREATE INDEX IF NOT EXISTS idx_notifications_user_time ON notifications (user_id, created_at);
