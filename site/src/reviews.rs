//! 光遇代跑评价。
//!
//! 两条与评论不同的取舍：
//!
//! 1. **评价正文是纯文本**，不走 Markdown。内容短、结构化，多一层渲染没有收益，
//!    反而多一个注入面。它以文本节点输出，由 Leptos 自动转义。
//! 2. **必须带 1–5 的评分**，这是评价区别于普通留言的地方。
//!
//! 与评论一样**先审后显示**。

use leptos::prelude::*;
use serde::{Deserialize, Serialize};

use crate::auth::ActionResult;

/// 评价正文长度上限（按字符数）。
#[cfg(feature = "ssr")]
const BODY_MAX: usize = 1000;

/// 评分的合法区间。
pub const RATING_MIN: i64 = 1;
pub const RATING_MAX: i64 = 5;

/// 一条已批准的评价。
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct ReviewView {
    pub id: i64,
    pub author: String,
    pub rating: i64,
    pub body: String,
    /// 站长的回复，可能没有。
    pub reply: Option<String>,
    pub created_at: String,
    /// 是否被光遇管理员设为精选（代跑页会置顶）。
    #[serde(default)]
    pub featured: bool,
}

/// 评价区的数据：列表 + 当前用户是否有待审评价。
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct ReviewBoard {
    /// 已批准的评价，新的在前。
    pub reviews: Vec<ReviewView>,
    /// 当前用户是否有待审评价。有提示，提交者才知道自己的评价在排队。
    pub viewer_has_pending: bool,
}

/// 载入评价区。
#[server]
pub async fn load_review_board() -> Result<ReviewBoard, ServerFnError> {
    use crate::state::AppState;
    use sqlx::Row;

    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");
    let viewer = crate::auth::current_user_id().await;

    let rows = sqlx::query(
        "SELECT r.id, r.rating, r.body, r.reply, r.created_at, u.display_name \
         FROM sky_reviews r JOIN users u ON u.id = r.user_id \
         WHERE r.status = 'approved' \
         ORDER BY r.created_at DESC, r.id DESC",
    )
    .fetch_all(&app.pool)
    .await
    .map_err(|e| ServerFnError::new(format!("查询评价失败: {e}")))?;

    // 精选：读光遇展示设置里的 id 列表，置顶并打标
    let raw: String =
        sqlx::query_scalar("SELECT value FROM sky_boosting WHERE key = 'featured_reviews'")
            .fetch_optional(&app.pool)
            .await
            .map_err(|e| ServerFnError::new(format!("查询精选评价失败: {e}")))?
            .unwrap_or_else(|| "[]".to_string());
    let featured_ids: Vec<i64> = serde_json::from_str(&raw).unwrap_or_default();

    let mut reviews: Vec<ReviewView> = rows
        .into_iter()
        .map(|row| {
            let id: i64 = row.get("id");
            ReviewView {
                id,
                author: row.get("display_name"),
                rating: row.get("rating"),
                body: row.get("body"),
                reply: row.get("reply"),
                created_at: row.get("created_at"),
                featured: featured_ids.contains(&id),
            }
        })
        .collect();
    reviews.sort_by_key(|review| !review.featured);

    let viewer_has_pending = match viewer {
        Some(user_id) => {
            let count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM sky_reviews WHERE user_id = ?1 AND status = 'pending'",
            )
            .bind(user_id)
            .fetch_one(&app.pool)
            .await
            .map_err(|e| ServerFnError::new(format!("查询待审评价失败: {e}")))?;
            count > 0
        }
        None => false,
    };

    Ok(ReviewBoard {
        reviews,
        viewer_has_pending,
    })
}

/// 提交一条代跑评价。落库为 `pending`。
#[server]
pub async fn submit_sky_review(rating: i64, body: String) -> ActionResult {
    use crate::state::AppState;

    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");

    let Some(user_id) = crate::auth::current_user_id().await else {
        return Ok(Err("请先登录再评价。".to_string()));
    };

    // 评分是数字输入，但仍然要校验区间——库里的 CHECK 只是最后一道网
    if !(RATING_MIN..=RATING_MAX).contains(&rating) {
        return Ok(Err(format!("评分需在 {RATING_MIN}–{RATING_MAX} 之间。")));
    }

    let body = body.trim();
    if body.is_empty() {
        return Ok(Err("评价不能为空。".to_string()));
    }
    if body.chars().count() > BODY_MAX {
        return Ok(Err(format!("评价不能超过 {BODY_MAX} 个字符。")));
    }

    sqlx::query("INSERT INTO sky_reviews (user_id, rating, body) VALUES (?1, ?2, ?3)")
        .bind(user_id)
        .bind(rating)
        .bind(body)
        .execute(&app.pool)
        .await
        .map_err(|e| ServerFnError::new(format!("保存评价失败: {e}")))?;

    Ok(Ok(()))
}
