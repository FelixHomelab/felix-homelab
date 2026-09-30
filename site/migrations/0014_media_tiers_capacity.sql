-- 媒体分级压缩（按“年龄”重压缩）与容量池
--
-- 压缩时序（用户定稿）：>7 天 zstd-3、>1 月 zstd-7、>3 月 zstd-19；
-- 高访问量（近期访问过或累计访问次数高）跳过压缩/升级，保访问体验。
-- 因此上传时原样存储（可支持 Range/秒开），由后台任务按年龄压缩。

ALTER TABLE media ADD COLUMN access_count INTEGER NOT NULL DEFAULT 0;

-- 容量池流水（字节）：正数=入账（购买/结转/调整）；已用 = SUM(media.original_size)
CREATE TABLE IF NOT EXISTS capacity_pool_entries (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id    INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    bytes      INTEGER NOT NULL,
    kind       TEXT    NOT NULL CHECK (kind IN ('migrate', 'recharge', 'adjust')),
    note       TEXT    NOT NULL DEFAULT '',
    created_at TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_capacity_pool_user_time
    ON capacity_pool_entries (user_id, created_at);
