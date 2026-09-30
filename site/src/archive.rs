//! 容量过期后的「数据保管」流程与站内通知。
//!
//! 规则（用户定稿）：
//! - 到期前 7/3/1 天提醒；到期当天进入宽限期；
//! - 宽限天数按过期订阅的**容量与周期**计算：
//!   `clamp(3, 30, round(周期天数 × 10%) + (GB−5)/5 × 2)`；
//! - 宽限结束：超出当前有效容量的媒体移出个人存储（`media.archived_at`，
//!   不再计入个人用量；公开内容照常展示），并通知用户 3 天内答复；
//! - 用户需要：支付“额外空间占用费”（订阅最低价半价，¥2.5/月）→ 可下载并继续保管；
//! - 3 天未答复或不需要：留存一周后删除（容量紧张可提前）。

use leptos::prelude::*;
use serde::{Deserialize, Serialize};

use crate::orders::format_cents;

/// 额外空间占用费（分/月）：订阅最低价（¥5/月）的半价。
pub const ARCHIVE_HOLD_FEE_CENTS: i64 = 250;

/// 通知视图。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NotificationView {
    pub id: i64,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub link: String,
    pub read: bool,
    pub created_at: String,
}

/// 保管案例视图。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArchiveCaseView {
    pub id: i64,
    pub status: String,
    pub bytes: i64,
    pub media_count: i64,
    pub grace_until: Option<String>,
    pub respond_by: Option<String>,
    pub delete_after: Option<String>,
    pub fee_cents: i64,
}

/// 宽限天数：容量 × 周期（见模块注释）。
pub fn grace_days(gb: i64, period_days: i64) -> i64 {
    let base = (period_days as f64 * 0.1).round() as i64;
    let extra = ((gb - 5).max(0) / 5) * 2;
    (base + extra).clamp(3, 30)
}

/// 我的保管案例（未删除的）。
#[server]
pub async fn my_archive_cases() -> Result<Vec<ArchiveCaseView>, ServerFnError> {
    use crate::state::AppState;
    use sqlx::Row;

    let Some(identity) = crate::auth::current_identity().await else {
        return Ok(Vec::new());
    };
    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");

    let rows = sqlx::query(
        "SELECT id, status, bytes, media_count, grace_until, respond_by, delete_after \
         FROM archive_cases WHERE user_id = ?1 AND status != 'deleted' \
         ORDER BY id DESC LIMIT 20",
    )
    .bind(identity.id)
    .fetch_all(&app.pool)
    .await
    .map_err(|e| ServerFnError::new(format!("查询保管案例失败: {e}")))?;

    Ok(rows
        .iter()
        .map(|row| ArchiveCaseView {
            id: row.get("id"),
            status: row.get("status"),
            bytes: row.get("bytes"),
            media_count: row.get("media_count"),
            grace_until: row.get("grace_until"),
            respond_by: row.get("respond_by"),
            delete_after: row.get("delete_after"),
            fee_cents: ARCHIVE_HOLD_FEE_CENTS,
        })
        .collect())
}

/// 我的通知（最近 20 条）。
#[server]
pub async fn my_notifications() -> Result<Vec<NotificationView>, ServerFnError> {
    use crate::state::AppState;
    use sqlx::Row;

    let Some(identity) = crate::auth::current_identity().await else {
        return Ok(Vec::new());
    };
    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");

    let rows = sqlx::query(
        "SELECT id, kind, title, body, link, read_at, created_at \
         FROM notifications WHERE user_id = ?1 ORDER BY id DESC LIMIT 20",
    )
    .bind(identity.id)
    .fetch_all(&app.pool)
    .await
    .map_err(|e| ServerFnError::new(format!("查询通知失败: {e}")))?;

    Ok(rows
        .iter()
        .map(|row| {
            let read_at: Option<String> = row.get("read_at");
            NotificationView {
                id: row.get("id"),
                kind: row.get("kind"),
                title: row.get("title"),
                body: row.get("body"),
                link: row.get("link"),
                read: read_at.is_some(),
                created_at: row.get("created_at"),
            }
        })
        .collect())
}

/// 未读通知数量（顶栏提示用）。
#[server]
pub async fn unread_notification_count() -> Result<i64, ServerFnError> {
    use crate::state::AppState;

    let Some(identity) = crate::auth::current_identity().await else {
        return Ok(0);
    };
    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM notifications WHERE user_id = ?1 AND read_at IS NULL",
    )
    .bind(identity.id)
    .fetch_one(&app.pool)
    .await
    .unwrap_or(0);
    Ok(count)
}

/// 标记我的全部通知为已读。
#[server]
pub async fn mark_notifications_read() -> Result<(), ServerFnError> {
    use crate::state::AppState;

    let Some(identity) = crate::auth::current_identity().await else {
        return Ok(());
    };
    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");
    let _ = sqlx::query(
        "UPDATE notifications SET read_at = datetime('now') \
         WHERE user_id = ?1 AND read_at IS NULL",
    )
    .bind(identity.id)
    .execute(&app.pool)
    .await;
    Ok(())
}

/// 用户答复保管案例：`need = true` 需要（后续支付占用费），`false` 不需要。
#[server]
pub async fn respond_archive_case(case_id: i64, need: bool) -> Result<String, ServerFnError> {
    use crate::state::AppState;

    let Some(identity) = crate::auth::current_identity().await else {
        return Err(ServerFnError::new("请先登录"));
    };
    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");

    if need {
        // 生成“额外空间占用费”订单（复用订单系统；支付后延长保管并可下载）
        let order = crate::orders::create_order(
            "archive_hold".to_string(),
            format!("case:{case_id}"),
        )
        .await?;
        return Ok(if let Some(url) = order.checkout_url {
            format!("已生成支付链接：{url}")
        } else {
            format!("已生成订单 #{}：{}", order.order_id, order.message)
        });
    }

    let done = sqlx::query(
        "UPDATE archive_cases SET status = 'held', \
             delete_after = datetime('now', '+1 day') \
         WHERE id = ?1 AND user_id = ?2 AND status = 'notified'",
    )
    .bind(case_id)
    .bind(identity.id)
    .execute(&app.pool)
    .await;

    match done {
        Ok(result) if result.rows_affected() > 0 => Ok("已记录：不需要该数据，将在留存一周后删除。".to_string()),
        Ok(_) => Ok("该保管案例不存在或已处理。".to_string()),
        Err(e) => {
            tracing::error!("更新保管案例失败: {e}");
            Err(ServerFnError::new("操作失败，稍后再试"))
        }
    }
}

/// 启动保管流程维护任务（首轮 60s 后执行，之后每 24h）。
#[cfg(feature = "ssr")]
pub fn spawn_maintenance_task(pool: sqlx::SqlitePool) {
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        loop {
            match process_archive_flow(&pool).await {
                Ok(()) => tracing::info!("保管流程一轮完成"),
                Err(e) => tracing::warn!("保管流程执行失败: {e}"),
            }
            tokio::time::sleep(std::time::Duration::from_secs(24 * 3600)).await;
        }
    });
}

/// 每日处理：到期提醒 → 进入宽限 → 移出个人存储并通知 → 到期删除。
#[cfg(feature = "ssr")]
pub async fn process_archive_flow(pool: &sqlx::SqlitePool) -> anyhow::Result<()> {
    process_expiry_warnings(pool).await?;
    process_expired_entitlements(pool).await?;
    process_grace_end(pool).await?;
    process_pending_deletions(pool).await?;
    Ok(())
}

/// 到期前 7/3/1 天提醒。
#[cfg(feature = "ssr")]
async fn process_expiry_warnings(pool: &sqlx::SqlitePool) -> anyhow::Result<()> {
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT e.id, e.user_id, e.expires_at, \
                CAST(julianday(e.expires_at) - julianday('now') AS INTEGER) AS days_left \
         FROM entitlements e \
         WHERE e.kind = 'capacity' AND e.expires_at > datetime('now') \
           AND e.expires_at <= datetime('now', '+7 days')",
    )
    .fetch_all(pool)
    .await?;

    for row in rows {
        let entitlement_id: i64 = row.get("id");
        let user_id: i64 = row.get("user_id");
        let expires_at: String = row.get("expires_at");
        let days_left: i64 = row.get("days_left");
        let threshold = match days_left {
            d if d <= 1 => 1,
            d if d <= 3 => 3,
            _ => 7,
        };
        let dedupe = format!("expiry:{entitlement_id}:{threshold}");
        let _ = sqlx::query(
            "INSERT OR IGNORE INTO notifications (user_id, kind, title, body, link, dedupe_key) \
             VALUES (?1, 'expiry', ?2, ?3, '/subscriptions', ?4)",
        )
        .bind(user_id)
        .bind(format!("容量订阅将在 {days_left} 天内到期"))
        .bind(format!("到期时间：{expires_at}。到期后将进入数据保管宽限期，请及时续费。"))
        .bind(dedupe)
        .execute(pool)
        .await;
    }
    Ok(())
}

/// 已到期且尚无案例：进入宽限期（按容量 × 周期计算）。
#[cfg(feature = "ssr")]
async fn process_expired_entitlements(pool: &sqlx::SqlitePool) -> anyhow::Result<()> {
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT e.id, e.user_id, e.amount, e.expires_at, o.option AS order_option \
         FROM entitlements e LEFT JOIN orders o ON o.id = e.order_id \
         WHERE e.kind = 'capacity' AND e.expires_at <= datetime('now') \
           AND NOT EXISTS (SELECT 1 FROM archive_cases c WHERE c.entitlement_id = e.id)",
    )
    .fetch_all(pool)
    .await?;

    for row in rows {
        let entitlement_id: i64 = row.get("id");
        let user_id: i64 = row.get("user_id");
        let amount: i64 = row.get("amount");
        let order_option: Option<String> = row.get("order_option");
        let period_days = order_option
            .as_deref()
            .and_then(|option| option.rsplit(':').next())
            .map(|period| if period == "year" { 365 } else { 30 })
            .unwrap_or(30);
        let gb = amount / crate::orders::GIB;
        let grace = grace_days(gb, period_days);
        let _ = sqlx::query(
            "INSERT INTO archive_cases (user_id, entitlement_id, status, grace_until) \
             VALUES (?1, ?2, 'grace', datetime('now', ?3))",
        )
        .bind(user_id)
        .bind(entitlement_id)
        .bind(format!("+{grace} days"))
        .execute(pool)
        .await;
    }
    Ok(())
}

/// 宽限结束：超出当前有效容量的媒体移出个人存储，并通知用户 3 天内答复。
#[cfg(feature = "ssr")]
async fn process_grace_end(pool: &sqlx::SqlitePool) -> anyhow::Result<()> {
    use sqlx::Row;
    let cases = sqlx::query(
        "SELECT id, user_id FROM archive_cases \
         WHERE status = 'grace' AND grace_until <= datetime('now')",
    )
    .fetch_all(pool)
    .await?;

    for case in cases {
        let case_id: i64 = case.get("id");
        let user_id: i64 = case.get("user_id");

        // 当前有效容量（含其它订阅与人工额度）
        let allowed = crate::storage::user_capacity_total(pool, user_id).await;
        let used: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(original_size),0) FROM media \
             WHERE owner_id = ?1 AND archived_at IS NULL",
        )
        .bind(user_id)
        .fetch_one(pool)
        .await
        .unwrap_or(0);

        let mut excess = used - allowed;
        let mut archived_bytes = 0_i64;
        let mut archived_count = 0_i64;

        if excess > 0 {
            // 最旧的先移出（created_at 升序），直到不再超出
            let media = sqlx::query(
                "SELECT id, original_size FROM media \
                 WHERE owner_id = ?1 AND archived_at IS NULL \
                 ORDER BY created_at ASC",
            )
            .bind(user_id)
            .fetch_all(pool)
            .await?;
            for item in media {
                if excess <= 0 {
                    break;
                }
                let media_id: i64 = item.get("id");
                let size: i64 = item.get("original_size");
                let _ = sqlx::query("UPDATE media SET archived_at = datetime('now') WHERE id = ?1")
                    .bind(media_id)
                    .execute(pool)
                    .await;
                excess -= size;
                archived_bytes += size;
                archived_count += 1;
            }
        }

        let _ = sqlx::query(
            "UPDATE archive_cases SET status = 'notified', bytes = ?1, media_count = ?2, \
                 notified_at = datetime('now'), respond_by = datetime('now', '+3 days'), \
                 delete_after = datetime('now', '+10 days') \
             WHERE id = ?3",
        )
        .bind(archived_bytes)
        .bind(archived_count)
        .bind(case_id)
        .execute(pool)
        .await;

        let _ = sqlx::query(
            "INSERT OR IGNORE INTO notifications (user_id, kind, title, body, link, dedupe_key) \
             VALUES (?1, 'archive', '容量到期：数据已移出个人存储', ?2, '/subscriptions', ?3)",
        )
        .bind(user_id)
        .bind(format!(
            "共 {archived_count} 个文件（{}）已转入平台保管；公开内容展示不受影响。\
             请在 3 天内确认是否需要这部分数据：需要可支付额外占用费（{} / 月）后下载；\
             不需要或未答复将在一周后删除。",
            crate::orders::format_bytes(archived_bytes),
            crate::orders::format_cents(ARCHIVE_HOLD_FEE_CENTS)
        ))
        .bind(format!("archive:{case_id}"))
        .execute(pool)
        .await;
    }
    Ok(())
}

/// 到期删除：宽限/答复期结束后删除移交数据（容量紧张时可提前）。
#[cfg(feature = "ssr")]
async fn process_pending_deletions(pool: &sqlx::SqlitePool) -> anyhow::Result<()> {
    use sqlx::Row;
    let cases = sqlx::query(
        "SELECT id, user_id FROM archive_cases \
         WHERE status = 'held' AND delete_after <= datetime('now')",
    )
    .fetch_all(pool)
    .await?;

    for case in cases {
        let case_id: i64 = case.get("id");
        let user_id: i64 = case.get("user_id");
        let media = sqlx::query(
            "SELECT id, sha256, compression FROM media \
             WHERE owner_id = ?1 AND archived_at IS NOT NULL",
        )
        .bind(user_id)
        .fetch_all(pool)
        .await?;
        for item in media {
            let media_id: i64 = item.get("id");
            let sha: String = item.get("sha256");
            let compression: String = item.get("compression");
            let _ = sqlx::query("DELETE FROM media WHERE id = ?1")
                .bind(media_id)
                .execute(pool)
                .await;
            let refs: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM media WHERE sha256 = ?1")
                    .bind(&sha)
                    .fetch_one(pool)
                    .await
                    .unwrap_or(0);
            if refs == 0 {
                let path = crate::media::blob_path_for(&sha, &compression);
                let _ = std::fs::remove_file(path);
            }
        }
        let _ = sqlx::query("UPDATE archive_cases SET status = 'deleted' WHERE id = ?1")
            .bind(case_id)
            .execute(pool)
            .await;
        let _ = sqlx::query(
            "INSERT OR IGNORE INTO notifications (user_id, kind, title, body, link, dedupe_key) \
             VALUES (?1, 'archive', '保管到期的数据已删除', ?2, '/subscriptions', ?3)",
        )
        .bind(user_id)
        .bind(format!("案例 #{case_id} 的移交数据已按约定删除。"))
        .bind(format!("archive-deleted:{case_id}"))
        .execute(pool)
        .await;
    }
    Ok(())
}

/// 展示辅助：案例状态中文。
pub fn archive_status_label(status: &str) -> &'static str {
    match status {
        "grace" => "宽限期",
        "notified" => "待确认",
        "held" => "平台保管中",
        "claimed" => "已付费保管",
        _ => "已处理",
    }
}

/// 展示辅助：占用费文案。
pub fn hold_fee_text() -> String {
    format!("{} / 月", format_cents(ARCHIVE_HOLD_FEE_CENTS))
}
