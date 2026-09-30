-- 容量过期后的数据保管流程 + 站内通知
--
-- 生命周期：
--   订阅到期前 7/3/1 天：通知提醒
--   到期当天：进入宽限期（案例 status='grace'，宽限天数按订阅容量与周期计算）
--   宽限结束：超出当前容量的媒体移出个人存储（media.archived_at），
--             通知用户 3 天内确认是否需要（status='notified'）
--   用户需要：支付“额外空间占用费”（订阅最低价半价，¥2.5/月）后可下载并继续保管
--   3 天未答复或回复不需要：留存一周后删除；容量紧张时可提前删除
ALTER TABLE media ADD COLUMN archived_at TEXT;

CREATE TABLE IF NOT EXISTS archive_cases (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id     INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    entitlement_id INTEGER,
    bytes       INTEGER NOT NULL DEFAULT 0,
    media_count INTEGER NOT NULL DEFAULT 0,
    status      TEXT    NOT NULL DEFAULT 'grace'
                        CHECK (status IN ('grace', 'notified', 'held', 'claimed', 'deleted')),
    grace_until TEXT,
    notified_at TEXT,
    respond_by  TEXT,
    delete_after TEXT,
    note        TEXT    NOT NULL DEFAULT '',
    created_at  TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_archive_cases_user ON archive_cases (user_id, status);
CREATE INDEX IF NOT EXISTS idx_archive_cases_status ON archive_cases (status, delete_after);

CREATE TABLE IF NOT EXISTS notifications (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id    INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    kind       TEXT    NOT NULL CHECK (kind IN ('expiry', 'archive', 'system')),
    title      TEXT    NOT NULL,
    body       TEXT    NOT NULL DEFAULT '',
    link       TEXT    NOT NULL DEFAULT '',
    dedupe_key TEXT    NOT NULL DEFAULT '',
    read_at    TEXT,
    created_at TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_notifications_dedupe
    ON notifications (user_id, dedupe_key) WHERE dedupe_key != '';
CREATE INDEX IF NOT EXISTS idx_notifications_user_time ON notifications (user_id, created_at);
