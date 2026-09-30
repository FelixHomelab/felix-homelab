-- OIDC 身份关联：站内账号 ↔ 外部 IdP（Kanidm）的稳定标识
--
-- 关联以 (provider, sub) 为唯一键；sub 是 IdP 侧不可变的用户 UUID，
-- 这样站内改用户名也不影响登录。首次登录时按用户名兜底绑定。
CREATE TABLE IF NOT EXISTS oauth_identities (
    user_id    INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    provider   TEXT    NOT NULL,
    sub        TEXT    NOT NULL,
    created_at TEXT    NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (provider, sub)
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_oauth_identities_user_provider
    ON oauth_identities (user_id, provider);
