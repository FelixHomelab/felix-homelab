-- ============================================================
-- Felix Homelab 社区站 — 管理员角色细分
--
-- `users.role` 保持 ('admin' | 'user')：admin = 超级管理员（Supermaster），
-- 拥有全部权限。细分角色集中在这张表里，可叠加：
--
--   agentmaster     → Agent 管理（开通/续费/启停/删除）
--   communitymaster → 社区与评论管理
--   skymaster       → 光遇评价管理
--
-- 之所以不扩展 users.role 的 CHECK（SQLite 改约束要重建被外键引用的表），
-- 单独建表既可叠加角色，也避免动 users。
-- ============================================================

CREATE TABLE IF NOT EXISTS user_roles (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id    INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role       TEXT    NOT NULL
                       CHECK (role IN ('agentmaster', 'communitymaster', 'skymaster')),
    created_at TEXT    NOT NULL DEFAULT (datetime('now')),
    UNIQUE (user_id, role)
);

CREATE INDEX IF NOT EXISTS idx_user_roles_user ON user_roles(user_id);
