-- 订单商品新增「数据保管占用费」：SQLite 无法修改 CHECK，需重建表。
CREATE TABLE orders_new (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id      INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    product      TEXT    NOT NULL CHECK (product IN ('agent_time', 'capacity', 'external_storage', 'archive_hold')),
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

INSERT INTO orders_new (id, user_id, product, option, amount_cents, currency, status, provider, provider_ref, note, created_at, paid_at)
    SELECT id, user_id, product, option, amount_cents, currency, status, provider, provider_ref, note, created_at, paid_at FROM orders;

DROP TABLE orders;
ALTER TABLE orders_new RENAME TO orders;

CREATE INDEX IF NOT EXISTS idx_orders_user_time ON orders (user_id, created_at);
CREATE INDEX IF NOT EXISTS idx_orders_status_time ON orders (status, created_at);
