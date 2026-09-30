-- 富媒体存储：图片 / 文件 / 语音 / 视频
--
-- 配额按原始大小（original_size）计；压缩只影响平台占用（stored_size）。
-- 存储为内容寻址（sha256），压缩策略见 site/TODO.md「存储压缩策略」。
CREATE TABLE IF NOT EXISTS media (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    owner_id         INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    kind             TEXT    NOT NULL CHECK (kind IN ('image', 'audio', 'video', 'file')),
    mime             TEXT    NOT NULL,
    original_name    TEXT    NOT NULL DEFAULT '',
    original_size    INTEGER NOT NULL,
    stored_size      INTEGER NOT NULL,
    compression      TEXT    NOT NULL DEFAULT 'none'
                             CHECK (compression IN ('none', 'zstd-3', 'zstd-7', 'zstd-19')),
    sha256           TEXT    NOT NULL,
    created_at       TEXT    NOT NULL DEFAULT (datetime('now')),
    last_accessed_at TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_media_owner ON media (owner_id, created_at);
CREATE INDEX IF NOT EXISTS idx_media_sha ON media (sha256);
