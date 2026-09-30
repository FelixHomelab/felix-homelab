//! 容量池（字节）：订阅得到的存储额度与当前占用。
//!
//! 口径：**配额按原始大小计**（`media.original_size`），压缩节省归平台。
//! 已用暂时只含站内媒体；OpenCloud/Forgejo 的用量后续并入。

use leptos::prelude::*;
use serde::{Deserialize, Serialize};

/// 容量池视图。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CapacityPoolView {
    /// 统计窗口：all / month / week / day（与时长池一致）
    pub window: String,
    /// 窗口内入账（累计订阅/结转）
    pub total_bytes: i64,
    /// 当前已用（全部时间的媒体原始大小合计）
    pub used_bytes: i64,
}

/// 查询账号容量池（窗口内入账 + 当前已用）。
#[server]
pub async fn capacity_pool(window: String) -> Result<CapacityPoolView, ServerFnError> {
    use crate::state::AppState;

    let window = match window.as_str() {
        "month" | "week" | "day" => window,
        _ => "all".to_string(),
    };

    let Some(identity) = crate::auth::current_identity().await else {
        return Ok(CapacityPoolView {
            window,
            total_bytes: 0,
            used_bytes: 0,
        });
    };
    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");

    let total_sql = match window.as_str() {
        "month" => {
            "SELECT COALESCE(SUM(bytes),0) FROM capacity_pool_entries \
             WHERE user_id = ?1 AND bytes > 0 AND created_at >= datetime('now','start of month')"
        }
        "week" => {
            "SELECT COALESCE(SUM(bytes),0) FROM capacity_pool_entries \
             WHERE user_id = ?1 AND bytes > 0 \
               AND created_at >= datetime(date('now','weekday 0','-6 days'))"
        }
        "day" => {
            "SELECT COALESCE(SUM(bytes),0) FROM capacity_pool_entries \
             WHERE user_id = ?1 AND bytes > 0 AND created_at >= datetime('now','start of day')"
        }
        _ => {
            "SELECT COALESCE(SUM(bytes),0) FROM capacity_pool_entries \
             WHERE user_id = ?1 AND bytes > 0"
        }
    };

    let total_bytes: i64 = sqlx::query_scalar(total_sql)
        .bind(identity.id)
        .fetch_one(&app.pool)
        .await
        .map_err(|e| ServerFnError::new(format!("查询容量池失败: {e}")))?;
    let used_bytes = user_capacity_used(&app.pool, identity.id).await;

    Ok(CapacityPoolView {
        window,
        total_bytes,
        used_bytes,
    })
}

/// 已用容量（字节）：用户全部媒体的原始大小合计。
#[cfg(feature = "ssr")]
pub async fn user_capacity_used(pool: &sqlx::SqlitePool, user_id: i64) -> i64 {
    sqlx::query_scalar("SELECT COALESCE(SUM(original_size),0) FROM media WHERE owner_id = ?1")
        .bind(user_id)
        .fetch_one(pool)
        .await
        .unwrap_or(0)
}

/// 容量池总额（字节，全部入账）。0 表示尚未开通——过渡期不限制上传。
#[cfg(feature = "ssr")]
pub async fn user_capacity_total(pool: &sqlx::SqlitePool, user_id: i64) -> i64 {
    sqlx::query_scalar(
        "SELECT COALESCE(SUM(bytes),0) FROM capacity_pool_entries WHERE user_id = ?1",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
    .unwrap_or(0)
}
