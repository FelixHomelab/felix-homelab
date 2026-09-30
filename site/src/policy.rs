//! 资源政策：用户分组、全站免费期、并发上限与启动排队。
//!
//! 站长决策（2026-09-30）：站点暂定为个人/小团队使用，服务运行在个人电脑上，
//! **全站长期暂停收费**（超级管理员可在后台设置起止，默认不限结束）；用户分
//! 组决定资源权益：
//!   - 普通用户（normal）：无特殊权益，仍可购买 OpenCloud / 外置存储；
//!   - 开发者组（developer）：并发实例不限、存储容量不限；
//!   - 管理员组（admin）：最多同时 2 个运行实例、存储容量不限。
//!
//! 资源受限时（全局运行上限），启动/唤醒进入 `agent_queue` 排队；本模块的
//! 后台任务在有空位时按顺序放行并通知用户。

use serde::{Deserialize, Serialize};

/// 全站免费期状态（前台展示用）。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BillingStatus {
    /// 是否处于免费期（暂停计费）
    pub paused: bool,
    pub started_at: Option<String>,
    /// 结束时间；None 表示不限结束
    pub ends_at: Option<String>,
}

/// 排队条目（用户端：显示位置）。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct QueueSpot {
    pub slot: i64,
    /// 前面还有多少人（0 = 队首）
    pub ahead: i64,
    pub created_at: String,
}

/// 后台队列视图。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AdminQueueEntry {
    pub id: i64,
    pub username: String,
    pub slot: i64,
    pub subdomain: String,
    pub kind: String,
    pub created_at: String,
    pub position: i64,
}

/// 后台「资源与计费」设置视图。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResourceSettings {
    pub billing_paused: bool,
    pub pause_started_at: Option<String>,
    pub pause_ends_at: Option<String>,
    pub global_running_cap: i64,
    pub group_cap_normal: i64,
    pub group_cap_admin: i64,
    /// 0 = 不限
    pub group_cap_developer: i64,
}

#[cfg(feature = "ssr")]
pub use ssr::*;

#[cfg(feature = "ssr")]
mod ssr {
    use super::*;
    use crate::agents::AgentRuntime;
    use sqlx::{Row, SqlitePool};
    use std::collections::HashMap;

    /// 读站点设置（不存在返回 None）。
    pub async fn get_setting(pool: &SqlitePool, key: &str) -> Option<String> {
        sqlx::query_scalar::<_, String>("SELECT value FROM site_settings WHERE key = ?1")
            .bind(key)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten()
    }

    /// 写站点设置（upsert）。
    pub async fn set_setting(pool: &SqlitePool, key: &str, value: &str) {
        let _ = sqlx::query(
            "INSERT INTO site_settings (key, value, updated_at) VALUES (?1, ?2, datetime('now')) \
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = datetime('now')",
        )
        .bind(key)
        .bind(value)
        .execute(pool)
        .await;
    }

    /// 全站免费期状态：pause 开关为真且未过结束时间才算生效。
    pub async fn billing_status(pool: &SqlitePool) -> BillingStatus {
        let paused_flag = get_setting(pool, "billing_paused")
            .await
            .map(|v| v == "1")
            .unwrap_or(false);
        let started_at = get_setting(pool, "billing_pause_started_at")
            .await
            .filter(|v| !v.is_empty());
        let ends_at = get_setting(pool, "billing_pause_ends_at")
            .await
            .filter(|v| !v.is_empty());
        let expired = match &ends_at {
            Some(end) => sqlx::query_scalar::<_, i64>(
                "SELECT CASE WHEN ?1 <= datetime('now') THEN 1 ELSE 0 END",
            )
            .bind(end)
            .fetch_one(pool)
            .await
            .unwrap_or(0)
                == 1,
            None => false,
        };
        BillingStatus {
            paused: paused_flag && !expired,
            started_at,
            ends_at,
        }
    }

    /// 用户资源分组（normal / developer / admin）。
    pub async fn res_group(pool: &SqlitePool, user_id: i64) -> String {
        sqlx::query_scalar::<_, String>("SELECT res_group FROM users WHERE id = ?1")
            .bind(user_id)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten()
            .unwrap_or_else(|| "normal".to_string())
    }

    /// 分组存储容量是否不限。
    pub fn is_unlimited_storage(group: &str) -> bool {
        matches!(group, "admin" | "developer")
    }

    /// 分组并发运行上限（0 = 不限）。
    pub async fn group_cap(pool: &SqlitePool, group: &str) -> i64 {
        let key = match group {
            "admin" => "group_cap_admin",
            "developer" => "group_cap_developer",
            _ => "group_cap_normal",
        };
        get_setting(pool, key)
            .await
            .and_then(|v| v.parse().ok())
            .unwrap_or(match group {
                "admin" => 2,
                "developer" => 0,
                _ => 1,
            })
    }

    /// 全局并发运行上限。
    pub async fn global_cap(pool: &SqlitePool) -> i64 {
        get_setting(pool, "global_running_cap")
            .await
            .and_then(|v| v.parse().ok())
            .unwrap_or(5)
    }

    /// 全局运行数（status.json 里 state == running）。
    pub fn global_running_count(status_map: &HashMap<String, AgentRuntime>) -> i64 {
        status_map
            .values()
            .filter(|r| r.state == "running")
            .count() as i64
    }

    /// 某用户的运行实例数（其未撤销实例中 state == running 的数量）。
    pub async fn user_running_count(
        pool: &SqlitePool,
        user_id: i64,
        status_map: &HashMap<String, AgentRuntime>,
    ) -> i64 {
        let subs = sqlx::query(
            "SELECT LOWER(subdomain) AS subdomain FROM agent_subscriptions \
             WHERE user_id = ?1 AND status != 'revoked' AND purged_at IS NULL \
               AND subdomain IS NOT NULL",
        )
        .bind(user_id)
        .fetch_all(pool)
        .await
        .unwrap_or_default();
        subs.iter()
            .filter(|row| {
                let sub: String = row.get("subdomain");
                status_map
                    .get(&sub)
                    .map(|r| r.state == "running")
                    .unwrap_or(false)
            })
            .count() as i64
    }

    /// 当前用户在该实例上的排队位置（无排队返回 None）。
    pub async fn queue_spot(pool: &SqlitePool, user_id: i64, slot: i64) -> Option<QueueSpot> {
        let row = sqlx::query(
            "SELECT id, created_at FROM agent_queue \
             WHERE user_id = ?1 AND slot = ?2 AND status = 'waiting'",
        )
        .bind(user_id)
        .bind(slot)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()?;
        let id: i64 = row.get("id");
        let ahead: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM agent_queue WHERE status = 'waiting' AND id < ?1",
        )
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap_or(0);
        Some(QueueSpot {
            slot,
            ahead,
            created_at: row.get("created_at"),
        })
    }

    /// 入队（幂等：同用户同实例已有 waiting 则保留原位）。
    pub async fn enqueue(
        pool: &SqlitePool,
        user_id: i64,
        slot: i64,
        subdomain: &str,
        kind: &str,
        note: &str,
    ) {
        let _ = sqlx::query(
            "INSERT OR IGNORE INTO agent_queue (user_id, slot, subdomain, kind, note) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind(user_id)
        .bind(slot)
        .bind(subdomain)
        .bind(kind)
        .bind(note)
        .execute(pool)
        .await;
    }

    /// 有坑位/正常启动时撤销排队条目。
    pub async fn dequeue(pool: &SqlitePool, user_id: i64, slot: i64) {
        let _ = sqlx::query(
            "UPDATE agent_queue SET status = 'cancelled' \
             WHERE user_id = ?1 AND slot = ?2 AND status = 'waiting'",
        )
        .bind(user_id)
        .bind(slot)
        .execute(pool)
        .await;
    }

    /// 写一条站内通知（dedupe_key 相同只留一条）。
    pub async fn notify(
        pool: &SqlitePool,
        user_id: i64,
        kind: &str,
        title: &str,
        body: &str,
        link: &str,
        dedupe: &str,
    ) {
        let _ = sqlx::query(
            "INSERT OR IGNORE INTO notifications (user_id, kind, title, body, link, dedupe_key) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .bind(user_id)
        .bind(kind)
        .bind(title)
        .bind(body)
        .bind(link)
        .bind(dedupe)
        .execute(pool)
        .await;
    }

    /// 队列处理器：有空位时按顺序放行；超时条目作废。
    ///
    /// 并发上限在每轮开始时取一次；本地计数随放行递增，避免一轮内超发。
    pub async fn process_queue(pool: &SqlitePool) {
        let waiting = sqlx::query(
            "SELECT id, user_id, slot, subdomain, kind, created_at FROM agent_queue \
             WHERE status = 'waiting' ORDER BY id ASC LIMIT 50",
        )
        .fetch_all(pool)
        .await
        .unwrap_or_default();
        if waiting.is_empty() {
            return;
        }

        let status_map = crate::agents::read_agent_status();
        let mut global_running = global_running_count(&status_map);
        let global_limit = global_cap(pool).await;
        let mut user_cache: HashMap<i64, (String, i64, i64)> = HashMap::new();

        for row in &waiting {
            let id: i64 = row.get("id");
            let user_id: i64 = row.get("user_id");
            let slot: i64 = row.get("slot");
            let subdomain: String = row.get("subdomain");
            let created: String = row.get("created_at");

            // 排队超时（24 小时）作废
            let stale: i64 = sqlx::query_scalar(
                "SELECT CASE WHEN datetime(?1, '+24 hours') <= datetime('now') THEN 1 ELSE 0 END",
            )
            .bind(&created)
            .fetch_one(pool)
            .await
            .unwrap_or(0);
            if stale == 1 {
                let _ = sqlx::query(
                    "UPDATE agent_queue SET status = 'expired' WHERE id = ?1 AND status = 'waiting'",
                )
                .bind(id)
                .execute(pool)
                .await;
                notify(
                    pool,
                    user_id,
                    "queue",
                    "排队已超时",
                    "资源一直繁忙，你的启动请求已自动取消，可稍后再试。",
                    "/subscriptions",
                    &format!("queue-expired-{id}"),
                )
                .await;
                continue;
            }

            // 订阅仍有效？
            let sub = sqlx::query(
                "SELECT status, kind, subdomain FROM agent_subscriptions \
                 WHERE user_id = ?1 AND slot = ?2 AND purged_at IS NULL",
            )
            .bind(user_id)
            .bind(slot)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten();
            let Some(sub) = sub else {
                let _ = sqlx::query(
                    "UPDATE agent_queue SET status = 'cancelled' WHERE id = ?1",
                )
                .bind(id)
                .execute(pool)
                .await;
                continue;
            };
            let sub_status: String = sub.get("status");
            if sub_status != "active" {
                let _ = sqlx::query("UPDATE agent_queue SET status = 'cancelled' WHERE id = ?1")
                    .bind(id)
                    .execute(pool)
                    .await;
                continue;
            }
            let sub_subdomain: Option<String> = sub.try_get("subdomain").ok();
            let real_subdomain = sub_subdomain.unwrap_or(subdomain);

            // 已经跑起来了（可能用户自己等到空位）→ 直接出队
            let running_now = status_map
                .get(&real_subdomain.to_ascii_lowercase())
                .map(|r| r.state == "running")
                .unwrap_or(false);
            if running_now {
                let _ = sqlx::query(
                    "UPDATE agent_queue SET status = 'started', started_at = datetime('now') \
                     WHERE id = ?1",
                )
                .bind(id)
                .execute(pool)
                .await;
                continue;
            }

            // 全局满员 → 本轮到此为止
            if global_limit > 0 && global_running >= global_limit {
                break;
            }

            // 用户分组上限
            if !user_cache.contains_key(&user_id) {
                let group = res_group(pool, user_id).await;
                let cap = group_cap(pool, &group).await;
                let group_used = user_running_count(pool, user_id, &status_map).await;
                user_cache.insert(user_id, (group, cap, group_used));
            }
            let (group, cap, mut used) = user_cache.get(&user_id).cloned().unwrap_or_default();
            if cap > 0 && used >= cap {
                continue;
            }

            // 放行：写 start 请求 + 出队 + 通知
            let kind: String = sub.try_get("kind").unwrap_or_else(|_| row.get("kind"));
            crate::agents::write_agent_request(
                "start",
                user_id_to_username(pool, user_id).await.as_str(),
                slot,
                &kind,
                Some(&real_subdomain),
                None,
            );
            let _ = sqlx::query(
                "UPDATE agent_queue SET status = 'started', started_at = datetime('now') \
                 WHERE id = ?1",
            )
            .bind(id)
            .execute(pool)
            .await;
            notify(
                pool,
                user_id,
                "queue",
                "已轮到你：Agent 正在启动",
                &format!("实例 #{}（{real_subdomain}）已获得资源，正在启动，稍后自动进入。", slot),
                "/subscriptions",
                &format!("queue-started-{id}"),
            )
            .await;
            used += 1;
            user_cache.insert(user_id, (group, cap, used));
            global_running += 1;
        }
    }

    async fn user_id_to_username(pool: &SqlitePool, user_id: i64) -> String {
        sqlx::query_scalar::<_, String>("SELECT username FROM users WHERE id = ?1")
            .bind(user_id)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten()
            .unwrap_or_default()
    }

    /// 后台任务：每 10 秒处理一次队列。
    pub fn spawn_queue_task(pool: SqlitePool) {
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(std::time::Duration::from_secs(10));
            loop {
                ticker.tick().await;
                process_queue(&pool).await;
            }
        });
    }
}
