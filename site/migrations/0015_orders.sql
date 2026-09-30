-- 订单与权益
--
-- provider：manual（管理员确认）| creem（Creem 托管收单，见 site/TODO.md 的官方文档引用）
-- 权益发放（grant）与订单一一对应，重复回调/重复确认通过 status 幂等。
CREATE TABLE IF NOT EXISTS orders (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id      INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    product      TEXT    NOT NULL CHECK (product IN ('agent_time', 'capacity', 'external_storage')),
    option       TEXT    NOT NULL,
    amount_cents INTEGER NOT NULL,
    currency     TEXT    NOT NULL DEFAULT 'CNY',
    status       TEXT    NOT NULL DEFAULT 'pending'
                         CHECK (status IN ('pending', 'paid', 'cancelled')),
    provider     TEXT    NOT NULL DEFAULT 'manual'
                         CHECK (provider IN ('manual', 'creem')),
    provider_ref TEXT    NOT NULL DEFAULT '',
    note         TEXT    NOT NULL DEFAULT '',
    created_at   TEXT    NOT NULL DEFAULT (datetime('now')),
    paid_at      TEXT
);

CREATE INDEX IF NOT EXISTS idx_orders_user_time ON orders (user_id, created_at);
CREATE INDEX IF NOT EXISTS idx_orders_status_time ON orders (status, created_at);

-- 权益：时间池充值（秒，无到期）/ 容量（字节，带到期）/ 外置存储（天数，带到期）
CREATE TABLE IF NOT EXISTS entitlements (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id    INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    order_id   INTEGER REFERENCES orders (id),
    kind       TEXT    NOT NULL CHECK (kind IN ('time_pool', 'capacity', 'external_storage')),
    amount     INTEGER NOT NULL,
    expires_at TEXT,
    note       TEXT    NOT NULL DEFAULT '',
    created_at TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_entitlements_user_kind ON entitlements (user_id, kind);
