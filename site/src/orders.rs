//! 订单与权益：AI Agent 时间池充值、容量池订阅、个人外置云存储订阅。
//!
//! 收款渠道：
//! - `manual`：管理员在后台「订单」页确认收款后发放权益（过渡通道）；
//! - `creem`：Creem 托管收单。官方依据（docs.creem.io，2026-09 拉取）：
//!   - 创建结账：`POST https://api.creem.io/v1/checkouts`，头 `x-api-key`，
//!     体 `{product_id, success_url, metadata}`，返回 `checkout_url`（跳转收款页）；
//!   - 回调：请求头 `creem-signature` = HMAC-SHA256(webhook_secret, 原始请求体)，
//!     事件 `checkout.completed` / `subscription.paid`（推荐用于开通）；
//!   - 金额为整数分，币种为大写 ISO（如 CNY）。
//!   未配置 `CREEM_API_KEY` 时自动走 manual 通道。

use leptos::prelude::*;
use serde::{Deserialize, Serialize};

/// 1GB（字节）。
pub const GIB: i64 = 1 << 30;

/// 时间池费率：¥19 = 30 天运行时间（试运行，保留调整权）。
pub const AGENT_TIME_RATE_CENTS_PER_MONTH: i64 = 1900;

/// 支付渠道。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrderView {
    pub id: i64,
    pub product: String,
    pub option: String,
    pub amount_cents: i64,
    pub status: String,
    pub provider: String,
    pub note: String,
    pub created_at: String,
    pub paid_at: Option<String>,
}

/// 创建订单的结果：`checkout_url` 非空时跳转收款页。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateOrderResult {
    pub order_id: i64,
    pub checkout_url: Option<String>,
    pub message: String,
}

/// 容量订阅价格（分）：5GB 起步，每 +5GB 加价 9 折；月付 ¥5 起、年付 ¥49 起。
pub fn capacity_price_cents(gb: i64, period: &str) -> Option<i64> {
    if gb < 5 || gb > 30 || gb % 5 != 0 {
        return None;
    }
    let blocks = (gb - 5) / 5; // 0..=5
    match period {
        "month" => Some(500 + blocks * 450),
        "year" => Some(4900 + blocks * 4410),
        _ => None,
    }
}

/// Agent 时间池充值：金额（分）→ 时长（秒）。
pub fn agent_seconds_for_cents(cents: i64) -> i64 {
    // ¥19 = 30 天 = 720h
    let seconds = cents * (720 * 3600) / AGENT_TIME_RATE_CENTS_PER_MONTH;
    seconds.max(0)
}

/// 外置云存储订阅（暂定价：¥9/月、¥29/季、¥119/年）。
pub fn external_storage_amount_cents(period: &str) -> Option<i64> {
    match period {
        "month" => Some(900),
        "quarter" => Some(2900),
        "year" => Some(11900),
        _ => None,
    }
}

/// 外置存储订阅时长（天）。
fn external_storage_days(period: &str) -> Option<i64> {
    match period {
        "month" => Some(30),
        "quarter" => Some(90),
        "year" => Some(365),
        _ => None,
    }
}

/// 业务错误（内层 String）→ 统一 ActionResult。
pub type ActionResult = Result<Result<(), String>, ServerFnError>;

/// 商品 + 选项 → 中文描述（订单列表展示用）。
pub fn product_label(product: &str, option: &str) -> String {
    match product {
        "agent_time" => {
            let cents: i64 = option
                .strip_prefix("cents:")
                .and_then(|value| value.parse().ok())
                .unwrap_or(0);
            format!("AI Agent 时间池充值 {}", format_cents(cents))
        }
        "capacity" => {
            let parts: Vec<&str> = option.split(':').collect();
            if let ["gb", gb, period] = parts.as_slice() {
                let period = if *period == "year" { "年" } else { "月" };
                format!("容量池 {gb}GB / {period}")
            } else {
                "容量池订阅".to_string()
            }
        }
        "external_storage" => {
            let period = match option {
                "month" => "月",
                "quarter" => "季",
                "year" => "年",
                _ => "期",
            };
            format!("个人外置云存储 / {period}")
        }
        _ => product.to_string(),
    }
}

/// 分 → ¥ 展示。
pub fn format_cents(cents: i64) -> String {
    if cents % 100 == 0 {
        format!("¥{}", cents / 100)
    } else {
        format!("¥{:.2}", cents as f64 / 100.0)
    }
}

/// 订单状态中文。
pub fn order_status_label(status: &str) -> &'static str {
    match status {
        "paid" => "已支付",
        "cancelled" => "已取消",
        _ => "待确认",
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SubscriptionState {
    /// 外置云存储到期时间（UTC 字符串；None 表示未开通）
    pub external_storage_until: Option<String>,
    /// 容量池通过订阅获得的额度（字节，仍在有效期的合计）
    pub capacity_bytes: i64,
}

/// 我的订阅状态（外置存储 / 容量订阅）。
#[server]
pub async fn my_subscription_state() -> Result<SubscriptionState, ServerFnError> {
    use crate::state::AppState;

    let Some(identity) = crate::auth::current_identity().await else {
        return Ok(SubscriptionState {
            external_storage_until: None,
            capacity_bytes: 0,
        });
    };
    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");

    let external_storage_until: Option<String> = sqlx::query_scalar(
        "SELECT MAX(expires_at) FROM entitlements \
         WHERE user_id = ?1 AND kind = 'external_storage' \
           AND (expires_at IS NULL OR expires_at > datetime('now'))",
    )
    .bind(identity.id)
    .fetch_one(&app.pool)
    .await
    .map_err(|e| ServerFnError::new(format!("查询订阅状态失败: {e}")))?;

    let capacity_bytes: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(amount),0) FROM entitlements \
         WHERE user_id = ?1 AND kind = 'capacity' \
           AND (expires_at IS NULL OR expires_at > datetime('now'))",
    )
    .bind(identity.id)
    .fetch_one(&app.pool)
    .await
    .map_err(|e| ServerFnError::new(format!("查询容量权益失败: {e}")))?;

    Ok(SubscriptionState {
        external_storage_until,
        capacity_bytes,
    })
}

/// 我的订单（最近 20 条）。
#[server]
pub async fn my_orders() -> Result<Vec<OrderView>, ServerFnError> {
    use crate::state::AppState;

    let Some(identity) = crate::auth::current_identity().await else {
        return Ok(Vec::new());
    };
    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");

    fetch_orders(&app.pool, Some(identity.id), 20).await
}

/// 后台：全部订单（最近 100 条）。
#[server]
pub async fn admin_orders() -> Result<Vec<OrderView>, ServerFnError> {
    use crate::state::AppState;

    let identity = crate::auth::current_identity()
        .await
        .ok_or_else(|| ServerFnError::new("请先登录"))?;
    if !identity.is_admin() {
        return Err(ServerFnError::new("需要超级管理员权限"));
    }
    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");
    fetch_orders(&app.pool, None, 100).await
}

#[cfg(feature = "ssr")]
async fn fetch_orders(
    pool: &sqlx::SqlitePool,
    user_id: Option<i64>,
    limit: i64,
) -> Result<Vec<OrderView>, ServerFnError> {
    use sqlx::Row;
    let rows = match user_id {
        Some(user_id) => {
            sqlx::query(
                "SELECT id, product, option, amount_cents, status, provider, note, created_at, paid_at \
                 FROM orders WHERE user_id = ?1 ORDER BY id DESC LIMIT ?2",
            )
            .bind(user_id)
            .bind(limit)
            .fetch_all(pool)
            .await
        }
        None => {
            sqlx::query(
                "SELECT id, product, option, amount_cents, status, provider, note, created_at, paid_at \
                 FROM orders ORDER BY id DESC LIMIT ?1",
            )
            .bind(limit)
            .fetch_all(pool)
            .await
        }
    }
    .map_err(|e| ServerFnError::new(format!("查询订单失败: {e}")))?;

    Ok(rows
        .iter()
        .map(|row| OrderView {
            id: row.get("id"),
            product: row.get("product"),
            option: row.get("option"),
            amount_cents: row.get("amount_cents"),
            status: row.get("status"),
            provider: row.get("provider"),
            note: row.get("note"),
            created_at: row.get("created_at"),
            paid_at: row.get("paid_at"),
        })
        .collect())
}

/// 创建订单（登录用户）。
///
/// - `product`：`agent_time`（`option` = 金额分，如 "600"）、
///   `capacity`（`option` = "gb:period"，如 "10:month"）、
///   `external_storage`（`option` = month/quarter/year）。
#[server]
pub async fn create_order(product: String, option: String) -> Result<CreateOrderResult, ServerFnError> {
    use crate::state::AppState;

    let Some(identity) = crate::auth::current_identity().await else {
        return Err(ServerFnError::new("请先登录"));
    };
    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");

    let (amount_cents, normalized_option) = match product.as_str() {
        "agent_time" => {
            let cents: i64 = option
                .parse()
                .map_err(|_| ServerFnError::new("充值金额格式不对"))?;
            if cents < 600 || cents > 100_000 {
                return Err(ServerFnError::new("充值金额需在 ¥6 ~ ¥1000 之间"));
            }
            (cents, format!("cents:{cents}"))
        }
        "capacity" => {
            let (gb, period) = option
                .split_once(':')
                .ok_or_else(|| ServerFnError::new("容量参数格式不对"))?;
            let gb: i64 = gb.parse().map_err(|_| ServerFnError::new("容量格式不对"))?;
            let cents = capacity_price_cents(gb, period)
                .ok_or_else(|| ServerFnError::new("容量档位或周期不支持"))?;
            (cents, format!("gb:{gb}:{period}"))
        }
        "external_storage" => {
            let cents = external_storage_amount_cents(&option)
                .ok_or_else(|| ServerFnError::new("周期不支持"))?;
            (cents, option.clone())
        }
        _ => return Err(ServerFnError::new("未知商品")),
    };

    let provider = if creem_enabled() { "creem" } else { "manual" };
    let insert = sqlx::query(
        "INSERT INTO orders (user_id, product, option, amount_cents, provider) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(identity.id)
    .bind(&product)
    .bind(&normalized_option)
    .bind(amount_cents)
    .bind(provider)
    .execute(&app.pool)
    .await
    .map_err(|e| ServerFnError::new(format!("创建订单失败: {e}")))?;
    let order_id = insert.last_insert_rowid();

    // Creem：创建结账并返回跳转地址；失败则回退人工通道提示
    if provider == "creem" {
        match creem_create_checkout(order_id, &product, amount_cents).await {
            Ok(url) => {
                let _ = sqlx::query("UPDATE orders SET provider_ref = ?1 WHERE id = ?2")
                    .bind(&url)
                    .bind(order_id)
                    .execute(&app.pool)
                    .await;
                return Ok(CreateOrderResult {
                    order_id,
                    checkout_url: Some(url),
                    message: String::new(),
                });
            }
            Err(error) => {
                tracing::warn!("Creem 结账创建失败，回退人工通道: {error}");
                let _ = sqlx::query("UPDATE orders SET provider = 'manual' WHERE id = ?1")
                    .bind(order_id)
                    .execute(&app.pool)
                    .await;
                return Ok(CreateOrderResult {
                    order_id,
                    checkout_url: None,
                    message: "在线支付暂时不可用，已转为人工确认订单，请联系站长付款。".to_string(),
                });
            }
        }
    }

    Ok(CreateOrderResult {
        order_id,
        checkout_url: None,
        message: format!(
            "订单已创建（#{order_id}）。当前为人工通道：请通过「关于」页联系站长付款，确认后自动发放。"
        ),
    })
}

/// 管理员确认收款并发放权益（幂等：仅 pending 可确认）。
#[server]
pub async fn admin_confirm_order(order_id: i64) -> ActionResult {
    use crate::state::AppState;

    let identity = crate::auth::current_identity()
        .await
        .ok_or_else(|| ServerFnError::new("请先登录"))?;
    if !identity.is_admin() {
        return Ok(Err("需要超级管理员权限。".to_string()));
    }
    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");

    match mark_paid_and_grant(&app.pool, order_id).await {
        Ok(()) => Ok(Ok(())),
        Err(message) => Ok(Err(message)),
    }
}

/// 管理员取消订单（仅 pending）。
#[server]
pub async fn admin_cancel_order(order_id: i64) -> ActionResult {
    use crate::state::AppState;

    let identity = crate::auth::current_identity()
        .await
        .ok_or_else(|| ServerFnError::new("请先登录"))?;
    if !identity.is_admin() {
        return Ok(Err("需要超级管理员权限。".to_string()));
    }
    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");

    let done = sqlx::query("UPDATE orders SET status = 'cancelled' WHERE id = ?1 AND status = 'pending'")
        .bind(order_id)
        .execute(&app.pool)
        .await;
    match done {
        Ok(result) if result.rows_affected() > 0 => Ok(Ok(())),
        Ok(_) => Ok(Err("订单不存在或已处理。".to_string())),
        Err(e) => {
            tracing::error!("取消订单失败: {e}");
            Ok(Err("操作失败，稍后再试。".to_string()))
        }
    }
}

/// 标记订单已付并发放权益（幂等）。供人工确认与 Creem 回调复用。
#[cfg(feature = "ssr")]
pub async fn mark_paid_and_grant(pool: &sqlx::SqlitePool, order_id: i64) -> Result<(), String> {
    use sqlx::Row;

    let row = sqlx::query(
        "SELECT user_id, product, option, amount_cents, status FROM orders WHERE id = ?1",
    )
    .bind(order_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        tracing::error!("查询订单失败: {e}");
        "服务器出了点问题。".to_string()
    })?;

    let Some(row) = row else {
        return Err("订单不存在。".to_string());
    };
    let status: String = row.get("status");
    if status == "paid" {
        return Ok(()); // 幂等
    }
    if status != "pending" {
        return Err("订单状态不允许确认。".to_string());
    }

    let user_id: i64 = row.get("user_id");
    let product: String = row.get("product");
    let option: String = row.get("option");
    let amount_cents: i64 = row.get("amount_cents");

    sqlx::query("UPDATE orders SET status = 'paid', paid_at = datetime('now') WHERE id = ?1")
        .bind(order_id)
        .execute(pool)
        .await
        .map_err(|e| {
            tracing::error!("更新订单状态失败: {e}");
            "服务器出了点问题。".to_string()
        })?;

    let grant = async {
        match product.as_str() {
            "agent_time" => {
                let seconds = agent_seconds_for_cents(amount_cents);
                sqlx::query(
                    "INSERT INTO entitlements (user_id, order_id, kind, amount, note) \
                     VALUES (?1, ?2, 'time_pool', ?3, '时间池充值')",
                )
                .bind(user_id)
                .bind(order_id)
                .bind(seconds)
                .execute(pool)
                .await?;
                sqlx::query(
                    "INSERT INTO time_pool_entries (user_id, seconds, kind, note) \
                     VALUES (?1, ?2, 'recharge', '时间池充值（订单）')",
                )
                .bind(user_id)
                .bind(seconds)
                .execute(pool)
                .await?;
            }
            "capacity" => {
                let parts: Vec<&str> = option.split(':').collect();
                let (gb, period) = match parts.as_slice() {
                    ["gb", gb, period] => (
                        gb.parse::<i64>().map_err(|_| anyhow::anyhow!("容量格式错误"))?,
                        (*period).to_string(),
                    ),
                    _ => anyhow::bail!("容量订单参数错误"),
                };
                let days = if period == "year" { 365 } else { 30 };
                sqlx::query(
                    "INSERT INTO entitlements (user_id, order_id, kind, amount, expires_at, note) \
                     VALUES (?1, ?2, 'capacity', ?3, datetime('now', ?4), '容量池订阅')",
                )
                .bind(user_id)
                .bind(order_id)
                .bind(gb * GIB)
                .bind(format!("+{days} days"))
                .execute(pool)
                .await?;
            }
            "external_storage" => {
                let days = external_storage_days(&option).unwrap_or(30);
                sqlx::query(
                    "INSERT INTO entitlements (user_id, order_id, kind, amount, expires_at, note) \
                     VALUES (?1, ?2, 'external_storage', 1, datetime('now', ?3), '个人外置云存储')",
                )
                .bind(user_id)
                .bind(order_id)
                .bind(format!("+{days} days"))
                .execute(pool)
                .await?;
            }
            _ => anyhow::bail!("未知商品"),
        }
        Ok::<(), anyhow::Error>(())
    }
    .await;

    if let Err(error) = grant {
        tracing::error!("发放权益失败（订单 #{order_id}）: {error}");
        return Err("权益发放失败，请联系管理员处理。".to_string());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Creem（托管收单）
// ---------------------------------------------------------------------------

/// 是否启用 Creem 通道（配置了 API Key 即启用）。
#[cfg(feature = "ssr")]
pub fn creem_enabled() -> bool {
    std::env::var("CREEM_API_KEY")
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
}

/// 商品在 Creem 的商品 ID（环境变量 `CREEM_PRODUCT_AGENT_TIME` 等）。
#[cfg(feature = "ssr")]
fn creem_product_id(product: &str) -> Option<String> {
    let key = format!("CREEM_PRODUCT_{}", product.to_uppercase());
    std::env::var(key)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

/// 创建 Creem 结账，返回 `checkout_url`。
#[cfg(feature = "ssr")]
async fn creem_create_checkout(
    order_id: i64,
    product: &str,
    _amount_cents: i64,
) -> anyhow::Result<String> {
    let api_key = std::env::var("CREEM_API_KEY")?;
    let base =
        std::env::var("CREEM_API_BASE").unwrap_or_else(|_| "https://api.creem.io".to_string());
    let product_id = creem_product_id(product)
        .ok_or_else(|| anyhow::anyhow!("缺少 {product} 的 Creem 商品 ID"))?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()?;
    let response = client
        .post(format!("{base}/v1/checkouts"))
        .header("x-api-key", api_key)
        .json(&serde_json::json!({
            "product_id": product_id,
            "success_url": format!("https://www.wraindrock.com/subscription/success?order={order_id}"),
            "metadata": { "order_id": order_id.to_string() },
        }))
        .send()
        .await?;

    let status = response.status();
    let body: serde_json::Value = response.json().await?;
    if !status.is_success() {
        anyhow::bail!("Creem 返回 {status}: {body}");
    }
    body.get("checkout_url")
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("Creem 响应缺少 checkout_url: {body}"))
}

/// 标准 HMAC-SHA256（避免 hmac crate 与 sha2 0.11 的 digest 版本冲突）。
#[cfg(feature = "ssr")]
fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    const BLOCK: usize = 64;
    let mut key_block = [0u8; BLOCK];
    if key.len() > BLOCK {
        let hashed = Sha256::digest(key);
        key_block[..32].copy_from_slice(&hashed);
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }
    let mut ipad = [0x36u8; BLOCK];
    let mut opad = [0x5cu8; BLOCK];
    for index in 0..BLOCK {
        ipad[index] ^= key_block[index];
        opad[index] ^= key_block[index];
    }
    let inner = Sha256::new().chain_update(ipad).chain_update(data).finalize();
    let outer = Sha256::new().chain_update(opad).chain_update(inner).finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&outer);
    out
}

/// 常数时间比较（防时序侧信道；仅用于定长十六进制串）。
#[cfg(feature = "ssr")]
fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in left.iter().zip(right.iter()) {
        diff |= a ^ b;
    }
    diff == 0
}

/// Creem 回调：验签（`creem-signature` = HMAC-SHA256(secret, 原始请求体)）→ 幂等发放。
#[cfg(feature = "ssr")]
pub async fn creem_webhook(
    axum::extract::State(state): axum::extract::State<crate::state::AppState>,
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> axum::response::Response {
    use axum::response::IntoResponse;

    let Ok(secret) = std::env::var("CREEM_WEBHOOK_SECRET") else {
        return (axum::http::StatusCode::SERVICE_UNAVAILABLE, "webhook 未配置").into_response();
    };
    let signature = headers
        .get("creem-signature")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();

    let expected = hex::encode(hmac_sha256(secret.as_bytes(), &body));
    if !constant_time_eq(signature.as_bytes(), expected.as_bytes()) {
        tracing::warn!("Creem webhook 验签失败");
        return (axum::http::StatusCode::UNAUTHORIZED, "签名校验失败").into_response();
    }

    let payload: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(_) => return (axum::http::StatusCode::BAD_REQUEST, "载荷格式错误").into_response(),
    };
    let event = payload
        .get("eventType")
        .and_then(|value| value.as_str())
        .unwrap_or_default();

    if matches!(event, "checkout.completed" | "subscription.paid") {
        // 订单定位：metadata.order_id 优先；否则用 checkout.id / subscription.id
        let order_id = payload
            .pointer("/object/metadata/order_id")
            .or_else(|| payload.pointer("/object/checkout/metadata/order_id"))
            .and_then(|value| value.as_str())
            .and_then(|value| value.parse::<i64>().ok());
        let order_id = match order_id {
            Some(id) => Some(id),
            None => {
                let reference = payload
                    .pointer("/object/checkout/id")
                    .or_else(|| payload.pointer("/object/subscription/id"))
                    .and_then(|value| value.as_str());
                match reference {
                    Some(reference) => sqlx::query_scalar::<_, i64>(
                        "SELECT id FROM orders WHERE provider_ref = ?1 ORDER BY id DESC LIMIT 1",
                    )
                    .bind(reference)
                    .fetch_optional(&state.pool)
                    .await
                    .ok()
                    .flatten(),
                    None => None,
                }
            }
        };

        if let Some(order_id) = order_id {
            if let Err(error) = mark_paid_and_grant(&state.pool, order_id).await {
                tracing::warn!("Creem 回调发放失败（订单 #{order_id}）: {error}");
                return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "发放失败").into_response();
            }
        } else {
            tracing::warn!("Creem 回调无法定位订单: {event}");
        }
    }

    (axum::http::StatusCode::OK, "ok").into_response()
}
