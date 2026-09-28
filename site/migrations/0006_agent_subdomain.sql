-- ============================================================
-- Felix Homelab 社区站 — Agent 域名与回收
--
--   subdomain: 形如 r4nd0m.<用户名>.<agent名>.agent.<域名>
--              （随机数保证同一用户/多个实例域名不重复；创建时生成并永久保留，
--               删除后在宽限期内续期可复用同一域名，宽限期后随行删除再随机分配）
--   revoked_at: 撤销时间；撤销后保留 30 天供续期（数据/域名一并保留），
--              超期在授权或后台列表时清理。
-- ============================================================

ALTER TABLE agent_subscriptions ADD COLUMN subdomain TEXT;
ALTER TABLE agent_subscriptions ADD COLUMN revoked_at TEXT;

CREATE UNIQUE INDEX IF NOT EXISTS idx_agent_subscriptions_subdomain
    ON agent_subscriptions(subdomain) WHERE subdomain IS NOT NULL;
