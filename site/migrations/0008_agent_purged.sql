-- ============================================================
-- Felix Homelab 社区站 — Agent 彻底删除记录
--
--   purged_at: 彻底删除（容器/数据/域名立即回收）的时间。
--              非空即「彻底删除记录」：行本身保留 30 天供管理回溯，
--              之后在后台列表加载时清理。
--
-- 不加 status='purged'：status 的 CHECK 只允许 active/stopped/revoked，
-- 用单独时间戳表达「已彻底删除」，避免重建被外键引用的表。
-- ============================================================

ALTER TABLE agent_subscriptions ADD COLUMN purged_at TEXT;

CREATE INDEX IF NOT EXISTS idx_agent_subscriptions_purged
    ON agent_subscriptions(purged_at) WHERE purged_at IS NOT NULL;
