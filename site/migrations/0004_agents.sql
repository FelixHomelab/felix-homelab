-- ============================================================
-- Felix Homelab 社区站 — AI Agent 订阅（多租户试点）
--
-- 一个账号 = 一个独立 Agent 容器（模板镜像 + 独立卷 + 独立子域）。
-- 站点只存「授权状态」，容器生命周期由宿主脚本 scripts/agent-ctl.sh 执行：
--   后台授权 → 写请求文件 → systemd .path 触发脚本 → 建容器 / 路由 / 状态。
--
--   kind:   模板类型（P1 仅 opencode；dsh 预留）
--   status: active（享有且运行中）/ stopped（享有但暂停）/ revoked（已撤销）
--   expires_at: NULL 表示长期有效；否则到期后网关拒绝访问（402）
-- ============================================================

CREATE TABLE IF NOT EXISTS agent_subscriptions (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id     INTEGER NOT NULL UNIQUE REFERENCES users(id) ON DELETE CASCADE,
    kind        TEXT    NOT NULL DEFAULT 'opencode'
                        CHECK (kind IN ('opencode', 'dsh')),
    status      TEXT    NOT NULL DEFAULT 'active'
                        CHECK (status IN ('active', 'stopped', 'revoked')),
    note        TEXT    NOT NULL DEFAULT '',
    expires_at  TEXT,
    created_at  TEXT    NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_agent_subscriptions_status
    ON agent_subscriptions(status);
