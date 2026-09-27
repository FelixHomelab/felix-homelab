//! 评论。
//!
//! 用 `target_kind` + `target_slug` 做多态目标（`post` / `sky` / `community`），
//! 官方博客、光遇与社区内容共用一张表、一套审核逻辑，不必为每个板块复制一份。
//!
//! **新评论一律先落 `pending`**：开放注册的站点必然会被灌水，先审后显示比事后清理省力。
//!
//! 正文渲染必须传 `allow_html = false`——评论来自访客，放行裸 HTML 等于把 XSS
//! 直接送给每个读者。

use leptos::prelude::*;
use serde::{Deserialize, Serialize};

use crate::auth::ActionResult;

/// 评论正文长度上限（按字符数）。
#[cfg(feature = "ssr")]
const BODY_MAX: usize = 2000;

/// 合法的目标类型。
///
/// 这是**用户输入，必须白名单校验**：库里的 `CHECK` 约束虽然拦得住，但那要先打一次
/// 数据库、并让一个本来无效的请求留下错误日志；在这里挡掉更干净。
#[cfg(feature = "ssr")]
const TARGET_KINDS: [&str; 3] = ["post", "sky", "community"];

/// 一条已批准的评论。
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct CommentView {
    pub id: i64,
    pub parent_id: Option<i64>,
    pub author: String,
    pub author_username: String,
    /// 已渲染、且已过滤掉裸 HTML 的正文。
    pub body_html: String,
    pub created_at: String,
}

/// 一个目标下的评论区数据。
///
/// 评论列表与「当前用户是否有待审评论」放在一次请求里返回：两者用的是同一个目标，
/// 分两个 server function 只会多一次往返。
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct CommentThread {
    /// 已批准的评论，按时间正序。
    pub comments: Vec<CommentView>,
    /// 当前用户在这个目标下是否有待审评论。
    ///
    /// 有这个提示，提交者刷新后才知道自己的评论还在排队，而不是以为提交失败了。
    pub viewer_has_pending: bool,
}

/// 载入某个目标的评论区。
#[server]
pub async fn load_comment_thread(
    target_kind: String,
    target_slug: String,
) -> Result<CommentThread, ServerFnError> {
    use crate::state::AppState;
    use sqlx::Row;

    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");
    if !TARGET_KINDS.contains(&target_kind.as_str()) {
        return Ok(CommentThread {
            comments: Vec::new(),
            viewer_has_pending: false,
        });
    }

    let viewer = crate::auth::current_user_id().await;

    let rows = sqlx::query(
        "SELECT c.id, c.parent_id, c.body_html, c.created_at, u.display_name, u.username \
         FROM comments c JOIN users u ON u.id = c.user_id \
         WHERE c.target_kind = ?1 AND c.target_slug = ?2 AND c.status = 'approved' \
         ORDER BY c.created_at ASC, c.id ASC",
    )
    .bind(&target_kind)
    .bind(&target_slug)
    .fetch_all(&app.pool)
    .await
    .map_err(|e| ServerFnError::new(format!("查询评论失败: {e}")))?;

    let comments: Vec<CommentView> = rows
        .into_iter()
        .map(|row| CommentView {
            id: row.get("id"),
            parent_id: row.get("parent_id"),
            author: row.get("display_name"),
            author_username: row.get("username"),
            body_html: row.get("body_html"),
            created_at: row.get("created_at"),
        })
        .collect();

    let viewer_has_pending = match viewer {
        Some(user_id) => {
            let count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM comments \
                 WHERE target_kind = ?1 AND target_slug = ?2 AND user_id = ?3 \
                   AND status = 'pending'",
            )
            .bind(&target_kind)
            .bind(&target_slug)
            .bind(user_id)
            .fetch_one(&app.pool)
            .await
            .map_err(|e| ServerFnError::new(format!("查询待审评论失败: {e}")))?;
            count > 0
        }
        None => false,
    };

    Ok(CommentThread {
        comments,
        viewer_has_pending,
    })
}

/// 提交一条评论。落库为 `pending`，审核通过后才会出现在页面上。
#[server]
pub async fn submit_comment(
    target_kind: String,
    target_slug: String,
    body: String,
    parent_id: Option<i64>,
) -> ActionResult {
    use crate::state::AppState;

    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");

    let Some(user_id) = crate::auth::current_user_id().await else {
        return Ok(Err("请先登录再评论。".to_string()));
    };

    if !TARGET_KINDS.contains(&target_kind.as_str()) {
        return Ok(Err("评论目标不合法。".to_string()));
    }
    let slug = target_slug.trim();
    if slug.is_empty() || slug.chars().count() > 200 {
        return Ok(Err("评论目标不合法。".to_string()));
    }

    let body = body.trim();
    if body.is_empty() {
        return Ok(Err("评论不能为空。".to_string()));
    }
    if body.chars().count() > BODY_MAX {
        return Ok(Err(format!("评论不能超过 {BODY_MAX} 个字符。")));
    }

    // 父评论必须属于同一个目标，否则可以把回复挂到别的文章下面去
    if let Some(parent) = parent_id {
        let exists: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM comments \
             WHERE id = ?1 AND target_kind = ?2 AND target_slug = ?3 AND status = 'approved'",
        )
        .bind(parent)
        .bind(&target_kind)
        .bind(slug)
        .fetch_optional(&app.pool)
        .await
        .map_err(|e| ServerFnError::new(format!("校验父评论失败: {e}")))?;

        if exists.is_none() {
            return Ok(Err("要回复的评论不存在。".to_string()));
        }
    }

    // allow_html = false：评论来自访客，裸 HTML 必须过滤掉
    let body_html = crate::content::render_markdown(body, false);

    sqlx::query(
        "INSERT INTO comments (target_kind, target_slug, user_id, parent_id, body_md, body_html) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )
    .bind(&target_kind)
    .bind(slug)
    .bind(user_id)
    .bind(parent_id)
    .bind(body)
    .bind(&body_html)
    .execute(&app.pool)
    .await
    .map_err(|e| ServerFnError::new(format!("保存评论失败: {e}")))?;

    Ok(Ok(()))
}
