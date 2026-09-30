//! 后台：审核评论、管理用户、回复评价。
//!
//! **每个 server function 都自己校验管理员身份**。页面上的隐藏与跳转只是体验；
//! 接口是公开可达的，把按钮藏起来挡不住直接构造请求。
//!
//! 另一条约束是**防止把自己锁在门外**：管理员不能改自己的状态或角色。否则一个误操作
//! 就能让唯一的站长账号变成被封禁的普通用户，而后台再也进不去了。

use leptos::prelude::*;
use serde::{Deserialize, Serialize};

use crate::auth::ActionResult;

/// 评论状态的合法取值。
#[cfg(feature = "ssr")]
const COMMENT_STATUSES: [&str; 3] = ["pending", "approved", "rejected"];
/// 用户状态的合法取值。
#[cfg(feature = "ssr")]
const USER_STATUSES: [&str; 2] = ["active", "banned"];
/// 角色的合法取值。
#[cfg(feature = "ssr")]
const ROLES: [&str; 2] = ["admin", "user"];

/// 管理员身份校验。返回当前管理员的 id，或一句可直接展示的错误。
///
/// `pub(crate)`：Agent 管理（`crate::agents`）复用同一套校验。
#[cfg(feature = "ssr")]
pub(crate) async fn require_admin() -> Result<i64, String> {
    let Some(identity) = crate::auth::current_identity().await else {
        return Err("请先登录。".to_string());
    };
    if !identity.is_admin() {
        return Err("需要管理员权限。".to_string());
    }
    Ok(identity.id)
}

/// 当前用户是否管理员。
///
/// 只给后台页面决定**显示什么**用，**不承担授权**——每个 server function 自己校验。
/// 单独一个函数而不是复用 App 级的「当前用户」：那个资源在路由子树里读会时有时无
/// （连打 5 次能冒出 403），页面内部自己的资源才稳。
#[server]
pub async fn am_i_admin() -> Result<bool, ServerFnError> {
    Ok(require_admin().await.is_ok())
}

/// 后台首页的统计。
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct AdminOverview {
    pub pending_comments: i64,
    pub pending_reviews: i64,
    pub users: i64,
    pub banned: i64,
    /// 社区投稿总数与已下架数。
    pub community: i64,
    pub hidden_community: i64,
}

/// 后台看到的评论。
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct AdminComment {
    pub id: i64,
    pub target_kind: String,
    pub target_slug: String,
    pub author: String,
    pub body_md: String,
    pub status: String,
    pub parent_id: Option<i64>,
    pub created_at: String,
    /// 该评论下已存在的回复数——删除时会连带删掉它们，得先让操作者知道。
    pub reply_count: i64,
}

/// 后台看到的评价。
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct AdminReview {
    pub id: i64,
    pub author: String,
    pub rating: i64,
    pub body: String,
    pub reply: Option<String>,
    pub status: String,
    pub created_at: String,
    /// 是否已设为代跑页精选。
    #[serde(default)]
    pub featured: bool,
}

/// 后台看到的用户。
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct AdminUser {
    pub id: i64,
    pub username: String,
    pub display_name: String,
    pub role: String,
    pub status: String,
    pub created_at: String,
    pub last_login_at: Option<String>,
    pub comment_count: i64,
    /// 细分管理角色（user_roles）：agentmaster / communitymaster / skymaster
    pub scopes: Vec<String>,
    /// 资源分组：normal / developer / admin（决定并发与容量权益）
    pub res_group: String,
}

/// 后台统计。
#[server]
pub async fn admin_load_overview() -> Result<AdminOverview, ServerFnError> {
    use crate::state::AppState;

    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");
    crate::roles::require_staff()
        .await
        .map_err(|message| ServerFnError::new(message))?;

    let count = |sql: &'static str| {
        let pool = app.pool.clone();
        async move { sqlx::query_scalar::<_, i64>(sql).fetch_one(&pool).await }
    };

    Ok(AdminOverview {
        pending_comments: count("SELECT COUNT(*) FROM comments WHERE status = 'pending'")
            .await
            .map_err(|e| ServerFnError::new(format!("统计待审评论失败: {e}")))?,
        pending_reviews: count("SELECT COUNT(*) FROM sky_reviews WHERE status = 'pending'")
            .await
            .map_err(|e| ServerFnError::new(format!("统计待审评价失败: {e}")))?,
        users: count("SELECT COUNT(*) FROM users")
            .await
            .map_err(|e| ServerFnError::new(format!("统计用户失败: {e}")))?,
        banned: count("SELECT COUNT(*) FROM users WHERE status = 'banned'")
            .await
            .map_err(|e| ServerFnError::new(format!("统计封禁用户失败: {e}")))?,
        community: count("SELECT COUNT(*) FROM community_posts")
            .await
            .map_err(|e| ServerFnError::new(format!("统计社区投稿失败: {e}")))?,
        hidden_community: count("SELECT COUNT(*) FROM community_posts WHERE status = 'hidden'")
            .await
            .map_err(|e| ServerFnError::new(format!("统计已下架内容失败: {e}")))?,
    })
}

/// 列出评论。`status` 传 `pending` 只看待审，传 `all` 看全部。
#[server]
pub async fn admin_list_comments(status: String) -> Result<Vec<AdminComment>, ServerFnError> {
    use crate::state::AppState;
    use sqlx::Row;

    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");
    crate::roles::require_permission("community")
        .await
        .map_err(|message| ServerFnError::new(message))?;

    // 过滤条件来自客户端，必须白名单化后再决定用哪条 SQL，绝不拼字符串
    let pending_only = status == "pending";
    let sql = if pending_only {
        "SELECT c.id, c.target_kind, c.target_slug, c.body_md, c.status, c.parent_id, \
                c.created_at, u.display_name, \
                (SELECT COUNT(*) FROM comments r WHERE r.parent_id = c.id) AS reply_count \
         FROM comments c JOIN users u ON u.id = c.user_id \
         WHERE c.status = 'pending' \
         ORDER BY c.created_at DESC, c.id DESC LIMIT 200"
    } else {
        "SELECT c.id, c.target_kind, c.target_slug, c.body_md, c.status, c.parent_id, \
                c.created_at, u.display_name, \
                (SELECT COUNT(*) FROM comments r WHERE r.parent_id = c.id) AS reply_count \
         FROM comments c JOIN users u ON u.id = c.user_id \
         ORDER BY c.created_at DESC, c.id DESC LIMIT 200"
    };

    let rows = sqlx::query(sql)
        .fetch_all(&app.pool)
        .await
        .map_err(|e| ServerFnError::new(format!("查询评论失败: {e}")))?;

    Ok(rows
        .into_iter()
        .map(|row| AdminComment {
            id: row.get("id"),
            target_kind: row.get("target_kind"),
            target_slug: row.get("target_slug"),
            author: row.get("display_name"),
            body_md: row.get("body_md"),
            status: row.get("status"),
            parent_id: row.get("parent_id"),
            created_at: row.get("created_at"),
            reply_count: row.get("reply_count"),
        })
        .collect())
}

/// 改一条评论的审核状态。
#[server]
pub async fn admin_set_comment_status(id: i64, status: String) -> ActionResult {
    use crate::state::AppState;

    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");
    let admin_id = match crate::roles::require_permission("community").await {
        Ok(id) => id,
        Err(message) => return Ok(Err(message)),
    };
    let _ = admin_id;

    if !COMMENT_STATUSES.contains(&status.as_str()) {
        return Ok(Err("状态取值不合法。".to_string()));
    }

    let affected = sqlx::query("UPDATE comments SET status = ?1 WHERE id = ?2")
        .bind(&status)
        .bind(id)
        .execute(&app.pool)
        .await
        .map_err(|e| ServerFnError::new(format!("更新评论状态失败: {e}")))?
        .rows_affected();

    if affected == 0 {
        return Ok(Err("这条评论已经不存在了。".to_string()));
    }
    Ok(Ok(()))
}

/// 删除一条评论。
///
/// 数据库的外键是 `ON DELETE CASCADE`，所以**它的回复会一并被删掉**——
/// 界面上必须先把这个后果告诉操作者。
#[server]
pub async fn admin_delete_comment(id: i64) -> ActionResult {
    use crate::state::AppState;

    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");
    if let Err(message) = crate::roles::require_permission("community").await {
        return Ok(Err(message));
    }

    let affected = sqlx::query("DELETE FROM comments WHERE id = ?1")
        .bind(id)
        .execute(&app.pool)
        .await
        .map_err(|e| ServerFnError::new(format!("删除评论失败: {e}")))?
        .rows_affected();

    if affected == 0 {
        return Ok(Err("这条评论已经不存在了。".to_string()));
    }
    Ok(Ok(()))
}

/// 列出评价。`status` 传 `pending` 只看待审，传 `all` 看全部。
#[server]
pub async fn admin_list_reviews(status: String) -> Result<Vec<AdminReview>, ServerFnError> {
    use crate::state::AppState;
    use sqlx::Row;

    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");
    crate::roles::require_permission("sky")
        .await
        .map_err(|message| ServerFnError::new(message))?;

    let pending_only = status == "pending";
    let sql = if pending_only {
        "SELECT r.id, r.rating, r.body, r.reply, r.status, r.created_at, u.display_name \
         FROM sky_reviews r JOIN users u ON u.id = r.user_id \
         WHERE r.status = 'pending' \
         ORDER BY r.created_at DESC, r.id DESC LIMIT 200"
    } else {
        "SELECT r.id, r.rating, r.body, r.reply, r.status, r.created_at, u.display_name \
         FROM sky_reviews r JOIN users u ON u.id = r.user_id \
         ORDER BY r.created_at DESC, r.id DESC LIMIT 200"
    };

    let rows = sqlx::query(sql)
        .fetch_all(&app.pool)
        .await
        .map_err(|e| ServerFnError::new(format!("查询评价失败: {e}")))?;

    let raw: String =
        sqlx::query_scalar("SELECT value FROM sky_boosting WHERE key = 'featured_reviews'")
            .fetch_optional(&app.pool)
            .await
            .map_err(|e| ServerFnError::new(format!("查询精选评价失败: {e}")))?
            .unwrap_or_else(|| "[]".to_string());
    let featured_ids: Vec<i64> = serde_json::from_str(&raw).unwrap_or_default();

    Ok(rows
        .into_iter()
        .map(|row| {
            let id: i64 = row.get("id");
            AdminReview {
                id,
                author: row.get("display_name"),
                rating: row.get("rating"),
                body: row.get("body"),
                reply: row.get("reply"),
                status: row.get("status"),
                created_at: row.get("created_at"),
                featured: featured_ids.contains(&id),
            }
        })
        .collect())
}

/// 改一条评价的审核状态。
#[server]
pub async fn admin_set_review_status(id: i64, status: String) -> ActionResult {
    use crate::state::AppState;

    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");
    if let Err(message) = crate::roles::require_permission("sky").await {
        return Ok(Err(message));
    }
    if !COMMENT_STATUSES.contains(&status.as_str()) {
        return Ok(Err("状态取值不合法。".to_string()));
    }

    let affected = sqlx::query("UPDATE sky_reviews SET status = ?1 WHERE id = ?2")
        .bind(&status)
        .bind(id)
        .execute(&app.pool)
        .await
        .map_err(|e| ServerFnError::new(format!("更新评价状态失败: {e}")))?
        .rows_affected();

    if affected == 0 {
        return Ok(Err("这条评价已经不存在了。".to_string()));
    }
    Ok(Ok(()))
}

/// 回复一条评价。回复是纯文本，会直接显示在评价下方。
#[server]
pub async fn admin_reply_review(id: i64, reply: String) -> ActionResult {
    use crate::state::AppState;

    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");
    if let Err(message) = crate::roles::require_permission("sky").await {
        return Ok(Err(message));
    }

    let reply = reply.trim();
    if reply.chars().count() > 1000 {
        return Ok(Err("回复不能超过 1000 个字符。".to_string()));
    }
    // 空回复等于撤销回复
    let value = (!reply.is_empty()).then_some(reply);

    let affected = sqlx::query("UPDATE sky_reviews SET reply = ?1 WHERE id = ?2")
        .bind(value)
        .bind(id)
        .execute(&app.pool)
        .await
        .map_err(|e| ServerFnError::new(format!("保存回复失败: {e}")))?
        .rows_affected();

    if affected == 0 {
        return Ok(Err("这条评价已经不存在了。".to_string()));
    }
    Ok(Ok(()))
}

/// 删除一条评价。
#[server]
pub async fn admin_delete_review(id: i64) -> ActionResult {
    use crate::state::AppState;

    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");
    if let Err(message) = crate::roles::require_permission("sky").await {
        return Ok(Err(message));
    }

    let affected = sqlx::query("DELETE FROM sky_reviews WHERE id = ?1")
        .bind(id)
        .execute(&app.pool)
        .await
        .map_err(|e| ServerFnError::new(format!("删除评价失败: {e}")))?
        .rows_affected();

    if affected == 0 {
        return Ok(Err("这条评价已经不存在了。".to_string()));
    }
    Ok(Ok(()))
}

/// 列出用户，附带各自的评论数。
#[server]
pub async fn admin_list_users() -> Result<Vec<AdminUser>, ServerFnError> {
    use crate::state::AppState;
    use sqlx::Row;

    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");
    require_admin()
        .await
        .map_err(|message| ServerFnError::new(message))?;

    let rows = sqlx::query(
        "SELECT u.id, u.username, u.display_name, u.role, u.status, u.created_at, \
                u.last_login_at, u.res_group, \
                (SELECT COUNT(*) FROM comments c WHERE c.user_id = u.id) AS comment_count, \
                COALESCE((SELECT group_concat(r.role, ',') FROM user_roles r WHERE r.user_id = u.id), '') \
                    AS scopes \
         FROM users u ORDER BY u.id ASC LIMIT 500",
    )
    .fetch_all(&app.pool)
    .await
    .map_err(|e| ServerFnError::new(format!("查询用户失败: {e}")))?;

    Ok(rows
        .into_iter()
        .map(|row| AdminUser {
            id: row.get("id"),
            username: row.get("username"),
            display_name: row.get("display_name"),
            role: row.get("role"),
            status: row.get("status"),
            created_at: row.get("created_at"),
            last_login_at: row.get("last_login_at"),
            comment_count: row.get("comment_count"),
            scopes: row
                .get::<String, _>("scopes")
                .split(',')
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect(),
            res_group: row.get("res_group"),
        })
        .collect())
}

/// 封禁或解封用户。
///
/// 不允许改自己：否则一次误操作就可能把唯一的站长账号封掉，后台再也进不去。
#[server]
pub async fn admin_set_user_status(id: i64, status: String) -> ActionResult {
    use crate::state::AppState;

    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");
    let admin_id = match require_admin().await {
        Ok(id) => id,
        Err(message) => return Ok(Err(message)),
    };

    if !USER_STATUSES.contains(&status.as_str()) {
        return Ok(Err("状态取值不合法。".to_string()));
    }
    if id == admin_id {
        return Ok(Err("不能修改自己的状态。".to_string()));
    }

    let affected = sqlx::query("UPDATE users SET status = ?1 WHERE id = ?2")
        .bind(&status)
        .bind(id)
        .execute(&app.pool)
        .await
        .map_err(|e| ServerFnError::new(format!("更新用户状态失败: {e}")))?
        .rows_affected();

    if affected == 0 {
        return Ok(Err("这个用户已经不存在了。".to_string()));
    }
    Ok(Ok(()))
}

/// 改用户角色。同样不允许改自己，理由同上。
#[server]
pub async fn admin_set_user_role(id: i64, role: String) -> ActionResult {
    use crate::state::AppState;

    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");
    let admin_id = match require_admin().await {
        Ok(id) => id,
        Err(message) => return Ok(Err(message)),
    };

    if !ROLES.contains(&role.as_str()) {
        return Ok(Err("角色取值不合法。".to_string()));
    }
    if id == admin_id {
        return Ok(Err("不能修改自己的角色。".to_string()));
    }

    let affected = sqlx::query("UPDATE users SET role = ?1 WHERE id = ?2")
        .bind(&role)
        .bind(id)
        .execute(&app.pool)
        .await
        .map_err(|e| ServerFnError::new(format!("更新用户角色失败: {e}")))?
        .rows_affected();

    if affected == 0 {
        return Ok(Err("这个用户已经不存在了。".to_string()));
    }
    Ok(Ok(()))
}

/// 授予 / 撤销细分管理角色（仅超级管理员）。
#[server]
pub async fn admin_set_user_scope(
    id: i64,
    role: String,
    grant: bool,
) -> ActionResult {
    use crate::state::AppState;

    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");
    let admin_id = match require_admin().await {
        Ok(id) => id,
        Err(message) => return Ok(Err(message)),
    };
    if id == admin_id {
        return Ok(Err("不能修改自己的角色。".to_string()));
    }
    if !crate::roles::SCOPED_ROLES.contains(&role.as_str()) {
        return Ok(Err("角色取值不合法。".to_string()));
    }

    let result = if grant {
        sqlx::query("INSERT OR IGNORE INTO user_roles (user_id, role) VALUES (?1, ?2)")
            .bind(id)
            .bind(&role)
            .execute(&app.pool)
            .await
    } else {
        sqlx::query("DELETE FROM user_roles WHERE user_id = ?1 AND role = ?2")
            .bind(id)
            .bind(&role)
            .execute(&app.pool)
            .await
    };
    result.map_err(|e| ServerFnError::new(format!("更新管理角色失败: {e}")))?;
    Ok(Ok(()))
}

/// 设置用户资源分组（normal / developer / admin）。
#[server]
pub async fn admin_set_user_group(id: i64, group: String) -> ActionResult {
    use crate::state::AppState;

    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");
    if let Err(message) = require_admin().await {
        return Ok(Err(message));
    }
    let group = group.trim().to_ascii_lowercase();
    if !matches!(group.as_str(), "normal" | "developer" | "admin") {
        return Ok(Err("分组取值不合法。".to_string()));
    }
    sqlx::query("UPDATE users SET res_group = ?1 WHERE id = ?2")
        .bind(&group)
        .bind(id)
        .execute(&app.pool)
        .await
        .map_err(|e| ServerFnError::new(format!("更新资源分组失败: {e}")))?;
    Ok(Ok(()))
}

/// 后台读取「资源与计费」设置。
#[server]
pub async fn admin_resource_settings() -> Result<crate::policy::ResourceSettings, ServerFnError> {
    use crate::state::AppState;

    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");
    require_admin()
        .await
        .map_err(|message| ServerFnError::new(message))?;

    let status = crate::policy::billing_status(&app.pool).await;
    Ok(crate::policy::ResourceSettings {
        billing_paused: status.paused,
        pause_started_at: status.started_at,
        pause_ends_at: status.ends_at,
        global_running_cap: crate::policy::global_cap(&app.pool).await,
        group_cap_normal: crate::policy::group_cap(&app.pool, "normal").await,
        group_cap_admin: crate::policy::group_cap(&app.pool, "admin").await,
        group_cap_developer: crate::policy::group_cap(&app.pool, "developer").await,
    })
}

/// 保存「资源与计费」设置（超级管理员）。
#[server]
#[allow(clippy::too_many_arguments)]
pub async fn admin_save_resource_settings(
    billing_paused: bool,
    pause_started_at: String,
    pause_ends_at: String,
    global_running_cap: i64,
    group_cap_normal: i64,
    group_cap_admin: i64,
    group_cap_developer: i64,
) -> ActionResult {
    use crate::state::AppState;

    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");
    if let Err(message) = require_admin().await {
        return Ok(Err(message));
    }
    let clamp = |v: i64| v.clamp(0, 64);
    let started = pause_started_at.trim();
    let ends = pause_ends_at.trim();
    // 只接受空值或 SQLite 可解析的日期（宽松校验：长度 + 数字/横杠/冒号）
    let valid_date = |v: &str| {
        v.is_empty()
            || (v.len() >= 10
                && v.len() <= 19
                && v.chars().all(|c| c.is_ascii_digit() || c == '-' || c == ' ' || c == ':'))
    };
    if !valid_date(started) || !valid_date(ends) {
        return Ok(Err("时间格式应为 YYYY-MM-DD 或 YYYY-MM-DD HH:MM:SS。".to_string()));
    }

    crate::policy::set_setting(&app.pool, "billing_paused", if billing_paused { "1" } else { "0" })
        .await;
    let started_val = if started.is_empty() {
        crate::policy::get_setting(&app.pool, "billing_pause_started_at")
            .await
            .unwrap_or_default()
    } else {
        started.to_string()
    };
    crate::policy::set_setting(&app.pool, "billing_pause_started_at", &started_val).await;
    crate::policy::set_setting(&app.pool, "billing_pause_ends_at", ends).await;
    crate::policy::set_setting(&app.pool, "global_running_cap", &clamp(global_running_cap).to_string())
        .await;
    crate::policy::set_setting(&app.pool, "group_cap_normal", &clamp(group_cap_normal).to_string())
        .await;
    crate::policy::set_setting(&app.pool, "group_cap_admin", &clamp(group_cap_admin).to_string())
        .await;
    crate::policy::set_setting(
        &app.pool,
        "group_cap_developer",
        &clamp(group_cap_developer).to_string(),
    )
    .await;
    Ok(Ok(()))
}

/// 后台：当前排队列表（等待中）。
#[server]
pub async fn admin_list_queue() -> Result<Vec<crate::policy::AdminQueueEntry>, ServerFnError> {
    use crate::state::AppState;
    use sqlx::Row;

    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");
    require_admin()
        .await
        .map_err(|message| ServerFnError::new(message))?;

    let rows = sqlx::query(
        "SELECT q.id, u.username, q.slot, q.subdomain, q.kind, q.created_at, \
                (SELECT COUNT(*) FROM agent_queue x WHERE x.status = 'waiting' AND x.id < q.id) + 1 \
                    AS position \
         FROM agent_queue q JOIN users u ON u.id = q.user_id \
         WHERE q.status = 'waiting' ORDER BY q.id ASC LIMIT 200",
    )
    .fetch_all(&app.pool)
    .await
    .map_err(|e| ServerFnError::new(format!("查询排队失败: {e}")))?;

    Ok(rows
        .into_iter()
        .map(|row| crate::policy::AdminQueueEntry {
            id: row.get("id"),
            username: row.get("username"),
            slot: row.get("slot"),
            subdomain: row.get("subdomain"),
            kind: row.get("kind"),
            created_at: row.get("created_at"),
            position: row.get("position"),
        })
        .collect())
}

/// 后台：取消某条排队记录（通知所有者）。
#[server]
pub async fn admin_cancel_queue(id: i64) -> ActionResult {
    use crate::state::AppState;
    use sqlx::Row;

    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");
    if let Err(message) = require_admin().await {
        return Ok(Err(message));
    }

    let owner = sqlx::query(
        "SELECT user_id, slot FROM agent_queue WHERE id = ?1 AND status = 'waiting'",
    )
    .bind(id)
    .fetch_optional(&app.pool)
    .await
    .map_err(|e| ServerFnError::new(format!("查询排队记录失败: {e}")))?;
    let Some(owner) = owner else {
        return Ok(Err("这条排队记录不存在或已处理。".to_string()));
    };
    let user_id: i64 = owner.get("user_id");
    let slot: i64 = owner.get("slot");
    sqlx::query("UPDATE agent_queue SET status = 'cancelled' WHERE id = ?1 AND status = 'waiting'")
        .bind(id)
        .execute(&app.pool)
        .await
        .map_err(|e| ServerFnError::new(format!("取消排队失败: {e}")))?;
    crate::policy::notify(
        &app.pool,
        user_id,
        "queue",
        "排队已被取消",
        &format!("管理员取消了实例 #{slot} 的启动排队。"),
        "/subscriptions",
        &format!("queue-cancelled-{id}"),
    )
    .await;
    Ok(Ok(()))
}

// ---------------------------------------------------------------------------
// 社区投稿：直接发布 + 事后管理
//
// 与评论的「先审后发」不同：社区内容发布即公开，这里负责事后下架 / 恢复 / 删除。
// ---------------------------------------------------------------------------

/// 社区内容状态（后台可切换）。
#[cfg(feature = "ssr")]
const COMMUNITY_STATUSES: [&str; 2] = ["published", "hidden"];

/// 后台看到的社区内容。
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct AdminCommunityPost {
    pub id: i64,
    pub kind: String,
    pub slug: String,
    pub title: String,
    pub author: String,
    pub author_username: String,
    pub status: String,
    pub created_at: String,
}

/// 列出社区内容。`status` 传 `hidden` 只看已下架，传 `all` 看全部。
#[server]
pub async fn admin_list_community(status: String) -> Result<Vec<AdminCommunityPost>, ServerFnError> {
    use crate::state::AppState;
    use sqlx::Row;

    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");
    crate::roles::require_permission("community")
        .await
        .map_err(|message| ServerFnError::new(message))?;

    let base = "SELECT c.id, c.kind, c.slug, c.title, c.status, c.created_at, \
                       u.display_name, u.username \
                FROM community_posts c JOIN users u ON u.id = c.author_id";
    let rows = if status == "all" {
        sqlx::query(&format!("{base} ORDER BY c.created_at DESC, c.id DESC LIMIT 500"))
            .fetch_all(&app.pool)
            .await
    } else {
        let wanted = if COMMUNITY_STATUSES.contains(&status.as_str()) {
            status
        } else {
            "hidden".to_string()
        };
        sqlx::query(&format!(
            "{base} WHERE c.status = ?1 ORDER BY c.created_at DESC, c.id DESC LIMIT 500"
        ))
        .bind(wanted)
        .fetch_all(&app.pool)
        .await
    }
    .map_err(|e| ServerFnError::new(format!("查询社区内容失败: {e}")))?;

    Ok(rows
        .into_iter()
        .map(|row| AdminCommunityPost {
            id: row.get("id"),
            kind: row.get("kind"),
            slug: row.get("slug"),
            title: row.get("title"),
            author: row.get("display_name"),
            author_username: row.get("username"),
            status: row.get("status"),
            created_at: row.get("created_at"),
        })
        .collect())
}

/// 下架 / 恢复一条社区内容。
#[server]
pub async fn admin_set_community_status(id: i64, status: String) -> ActionResult {
    use crate::state::AppState;

    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");
    if let Err(message) = crate::roles::require_permission("community").await {
        return Ok(Err(message));
    }

    if !COMMUNITY_STATUSES.contains(&status.as_str()) {
        return Ok(Err("状态取值不合法。".to_string()));
    }

    let affected = sqlx::query("UPDATE community_posts SET status = ?1 WHERE id = ?2")
        .bind(&status)
        .bind(id)
        .execute(&app.pool)
        .await
        .map_err(|e| ServerFnError::new(format!("更新社区内容状态失败: {e}")))?
        .rows_affected();

    if affected == 0 {
        return Ok(Err("这条内容已经不存在了。".to_string()));
    }
    Ok(Ok(()))
}

/// 彻底删除一条社区内容（连同其评论）。
#[server]
pub async fn admin_delete_community(id: i64) -> ActionResult {
    use crate::state::AppState;
    use sqlx::Row;

    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");
    if let Err(message) = crate::roles::require_permission("community").await {
        return Ok(Err(message));
    }

    let row = sqlx::query(
        "SELECT (SELECT u.username || '/' || c.slug FROM users u WHERE u.id = c.author_id) AS target \
         FROM community_posts c WHERE c.id = ?1",
    )
    .bind(id)
    .fetch_optional(&app.pool)
    .await
    .map_err(|e| ServerFnError::new(format!("读取社区内容失败: {e}")))?;
    let Some(row) = row else {
        return Ok(Err("这条内容已经不存在了。".to_string()));
    };

    // 评论没有外键（多态关联），删除内容时手动清理
    let target: String = row.get("target");
    sqlx::query("DELETE FROM comments WHERE target_kind = 'community' AND target_slug = ?1")
        .bind(&target)
        .execute(&app.pool)
        .await
        .map_err(|e| ServerFnError::new(format!("删除关联评论失败: {e}")))?;

    sqlx::query("DELETE FROM community_posts WHERE id = ?1")
        .bind(id)
        .execute(&app.pool)
        .await
        .map_err(|e| ServerFnError::new(format!("删除社区内容失败: {e}")))?;

    Ok(Ok(()))
}

// ---------------------------------------------------------------------------
// Pod 管理（Felix-Homelab）
//
// 站点容器挂载了 rootless podman.sock，这里通过 Docker 兼容 API 读取容器状态、
// 执行重启。授权同样由 require_admin 自己完成。
// ---------------------------------------------------------------------------

/// Pod 里的一个容器（后台 Pod 管理页用）。
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct PodContainer {
    pub name: String,
    pub state: String,
    pub status: String,
    pub image: String,
    pub health: Option<String>,
}

/// rootless podman 的 API socket（在站点容器内挂载于此）。
#[cfg(feature = "ssr")]
const PODMAN_SOCK: &str = "/run/podman/podman.sock";
#[cfg(feature = "ssr")]
const PODMAN_API: &str = "http://localhost/v1.44";

/// 调用 Podman 的 Docker 兼容 API（经 curl 走 unix socket，避免引入 HTTP-over-UDS 依赖）。
#[cfg(feature = "ssr")]
fn podman_api(path: &str, method: &str) -> Result<String, String> {
    let out = std::process::Command::new("curl")
        .arg("-sSf")
        .arg("--max-time")
        .arg("10")
        .arg("--unix-socket")
        .arg(PODMAN_SOCK)
        .arg("-X")
        .arg(method)
        .arg(format!("{PODMAN_API}{path}"))
        .output()
        .map_err(|e| format!("执行 curl 失败：{e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("Podman API 调用失败：{}", err.trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// 该容器是否属于 Felix-Homelab（用于过滤与校验）。
#[cfg(feature = "ssr")]
fn is_workstation_container(name: &str) -> bool {
    name.starts_with("felix-homelab-") || name.starts_with("Felix-Homelab")
}

/// 后台 Pod 管理页：列出 Felix-Homelab 内的容器与状态。
#[server]
pub async fn admin_list_pod() -> Result<Vec<PodContainer>, ServerFnError> {
    require_admin().await.map_err(ServerFnError::new)?;

    let json = podman_api("/containers/json?all=true", "GET").map_err(ServerFnError::new)?;
    let value: serde_json::Value =
        serde_json::from_str(&json).map_err(|e| ServerFnError::new(e.to_string()))?;

    let mut list = Vec::new();
    if let Some(items) = value.as_array() {
        for item in items {
            let name = item
                .get("Names")
                .and_then(|v| v.as_array())
                .map(|names| {
                    names
                        .iter()
                        .filter_map(|n| n.as_str())
                        .map(|n| n.trim_start_matches('/').to_string())
                        .find(|n| is_workstation_container(n))
                })
                .flatten();
            let Some(name) = name else { continue };

            // 单独 inspect 拿健康状态
            let health = podman_api(&format!("/containers/{name}/json"), "GET")
                .ok()
                .and_then(|j| serde_json::from_str::<serde_json::Value>(&j).ok())
                .and_then(|v| {
                    v.get("State")
                        .and_then(|s| s.get("Health"))
                        .and_then(|h| h.get("Status"))
                        .and_then(|x| x.as_str())
                        .map(|s| s.to_string())
                });

            list.push(PodContainer {
                name,
                state: item.get("State").and_then(|v| v.as_str()).unwrap_or("unknown").to_string(),
                status: item.get("Status").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                image: item.get("Image").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                health,
            });
        }
    }
    list.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(list)
}

/// 重启指定容器（后台）。
#[server]
pub async fn admin_restart_container(name: String) -> Result<Result<(), String>, ServerFnError> {
    if require_admin().await.is_err() {
        return Ok(Err("需要管理员权限。".to_string()));
    }
    // 只允许本工作站的容器名，避免越权操作
    if !is_workstation_container(&name) || name.contains('/') {
        return Ok(Err("不允许的容器名。".to_string()));
    }
    match podman_api(&format!("/containers/{name}/restart?t=10"), "POST") {
        Ok(_) => Ok(Ok(())),
        Err(e) => Ok(Err(format!("重启失败：{e}"))),
    }
}

// ---------------------------------------------------------------------------
// 备份配置（分源 + 多渠道）
// ---------------------------------------------------------------------------

/// 一个备份渠道（异地目标）。
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct BackupChannel {
    pub name: String,
    /// "rclone" 或 "rsync"
    pub kind: String,
    pub target: String,
    pub enable: bool,
}

/// 备份总配置。
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct BackupConfig {
    pub forgejo: bool,
    pub site: bool,
    pub keep_days: u32,
    pub channels: Vec<BackupChannel>,
    /// 最近一次同步的结果（读 /status/sync.status）
    pub sync_status: String,
}

impl Default for BackupConfig {
    fn default() -> Self {
        Self {
            forgejo: true,
            site: true,
            keep_days: 7,
            channels: Vec::new(),
            sync_status: String::new(),
        }
    }
}

#[cfg(feature = "ssr")]
const BACKUP_CONF: &str = "/sync/backup.conf";
#[cfg(feature = "ssr")]
const SYNC_STATUS: &str = "/status/sync.status";
#[cfg(feature = "ssr")]
const SYNC_REQUEST: &str = "/sync/sync-request";
#[cfg(feature = "ssr")]
const BACKUP_REQUEST: &str = "/sync/backup-request";

#[cfg(feature = "ssr")]
fn read_backup_config() -> BackupConfig {
    let mut config = BackupConfig::default();
    let Ok(text) = std::fs::read_to_string(BACKUP_CONF) else {
        return config;
    };
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim().trim_matches('"').to_string();
        match key {
            "BACKUP_FORGEJO" => config.forgejo = value != "0",
            "BACKUP_SITE" => config.site = value != "0",
            "KEEP_DAYS" => config.keep_days = value.parse().unwrap_or(7),
            _ => {
                // 渠道字段按实际行读取，不用 CHANNEL_COUNT 索引
                if let Some(rest) = key.strip_prefix("CHANNEL_") {
                    if let Some((idx, field)) = rest.split_once('_') {
                        if let Ok(n) = idx.parse::<usize>() {
                            if n == 0 {
                                continue;
                            }
                            let pos = n - 1;
                            while config.channels.len() <= pos {
                                config.channels.push(BackupChannel {
                                    name: String::new(),
                                    kind: "rclone".to_string(),
                                    target: String::new(),
                                    enable: false,
                                });
                            }
                            match field {
                                "NAME" => config.channels[pos].name = value,
                                "KIND" => config.channels[pos].kind = value,
                                "TARGET" => config.channels[pos].target = value,
                                "ENABLE" => config.channels[pos].enable = value != "0",
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
    }
    config.channels.retain(|c| !c.name.is_empty() || !c.target.is_empty());
    config.sync_status = std::fs::read_to_string(SYNC_STATUS).unwrap_or_default();
    config
}

#[cfg(feature = "ssr")]
fn write_backup_config(config: &BackupConfig) -> Result<(), String> {
    let check = |label: &str, value: &str| -> Result<(), String> {
        if value.contains('"') || value.contains('\n') || value.contains('\r') {
            return Err(format!("{label} 不能包含引号或换行"));
        }
        Ok(())
    };
    let days = config.keep_days.clamp(1, 90);
    let mut text = String::from(
        "# Felix-Homelab 备份配置（由后台保存；也可手工编辑）\n\
         # 备份源：1 开启 / 0 关闭\n",
    );
    text.push_str(&format!(
        "BACKUP_FORGEJO={}\nBACKUP_SITE={}\nKEEP_DAYS={days}\n\n",
        u8::from(config.forgejo),
        u8::from(config.site),
    ));

    let mut channels = 0usize;
    for channel in &config.channels {
        if channel.name.trim().is_empty() && channel.target.trim().is_empty() {
            continue;
        }
        let kind = if channel.kind == "rsync" { "rsync" } else { "rclone" };
        check("渠道名称", &channel.name)?;
        check("渠道目标", &channel.target)?;
        channels += 1;
        text.push_str(&format!(
            "CHANNEL_{n}_NAME=\"{}\"\nCHANNEL_{n}_KIND=\"{kind}\"\nCHANNEL_{n}_TARGET=\"{}\"\nCHANNEL_{n}_ENABLE={}\n\n",
            channel.name.trim(),
            channel.target.trim(),
            u8::from(channel.enable),
            n = channels,
        ));
    }
    text.push_str(&format!("CHANNEL_COUNT={channels}\n"));

    std::fs::write(BACKUP_CONF, text).map_err(|e| format!("写入失败：{e}"))
}

/// 备份配置（分源开关 + 多渠道 + 最近同步状态）。
#[server]
pub async fn admin_backup_config() -> Result<BackupConfig, ServerFnError> {
    require_admin().await.map_err(ServerFnError::new)?;
    Ok(read_backup_config())
}

/// 保存备份配置。
///
/// `channels` 用 Option 包一层：表单编码遇到空数组时会直接省略该字段，
/// 非 Option 的 Vec 会反序列化失败（missing field `channels`）。
#[server]
pub async fn admin_backup_save_config(
    forgejo: bool,
    site: bool,
    keep_days: u32,
    channels: Option<Vec<BackupChannel>>,
) -> Result<Result<(), String>, ServerFnError> {
    if require_admin().await.is_err() {
        return Ok(Err("需要管理员权限。".to_string()));
    }
    let config = BackupConfig {
        forgejo,
        site,
        keep_days,
        channels: channels.unwrap_or_default(),
        sync_status: String::new(),
    };
    Ok(write_backup_config(&config).map_err(|e| e.to_string()))
}

/// 请求一次同步（写触发文件，由宿主机 systemd path 单元执行）。
#[server]
pub async fn admin_backup_trigger_sync() -> Result<Result<(), String>, ServerFnError> {
    if require_admin().await.is_err() {
        return Ok(Err("需要管理员权限。".to_string()));
    }
    let stamp = chrono::Local::now().format("%F %T").to_string();
    match std::fs::write(SYNC_REQUEST, format!("{stamp}\n")) {
        Ok(()) => Ok(Ok(())),
        Err(e) => Ok(Err(format!("写入触发文件失败：{e}"))),
    }
}

/// 请求一次备份（写触发文件，运行宿主机备份服务）。
#[server]
pub async fn admin_backup_now() -> Result<Result<String, String>, ServerFnError> {
    if require_admin().await.is_err() {
        return Ok(Err("需要管理员权限。".to_string()));
    }
    let before: Vec<String> = list_backup_files().into_iter().map(|f| f.name).collect();
    let stamp = chrono::Local::now().format("%F %T").to_string();
    if let Err(e) = std::fs::write(BACKUP_REQUEST, format!("{stamp}\n")) {
        return Ok(Err(format!("写入触发文件失败：{e}")));
    }
    for _ in 0..60 {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        if let Some(new) = list_backup_files()
            .into_iter()
            .map(|f| f.name)
            .find(|name| !before.contains(name))
        {
            return Ok(Ok(new));
        }
    }
    Ok(Err("已触发备份，但 60 秒内未看到新归档（可稍后刷新）".to_string()))
}

/// 一个备份归档（后台展示用）。
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct BackupFile {
    pub name: String,
    pub size: String,
    pub time: String,
}

#[cfg(feature = "ssr")]
const BACKUP_DIR: &str = "/backups";

#[cfg(feature = "ssr")]
fn human_size(bytes: u64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < units.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", units[unit])
    }
}

#[cfg(feature = "ssr")]
fn list_backup_files() -> Vec<BackupFile> {
    let mut out: Vec<(std::time::SystemTime, BackupFile)> = Vec::new();
    let Ok(entries) = std::fs::read_dir(BACKUP_DIR) else {
        return Vec::new();
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        // 兼容旧命名 felix-ws-* 的历史归档
        let is_backup = (name.starts_with("felix-homelab-") || name.starts_with("felix-ws-"))
            && name.ends_with(".tar.gz");
        if !is_backup {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        let modified = meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        let time = {
            let dt: chrono::DateTime<chrono::Local> = modified.into();
            dt.format("%Y-%m-%d %H:%M").to_string()
        };
        out.push((
            modified,
            BackupFile {
                name,
                size: human_size(meta.len()),
                time,
            },
        ));
    }
    out.sort_by(|a, b| b.0.cmp(&a.0));
    out.into_iter().map(|(_, file)| file).collect()
}

/// 备份归档列表。
#[server]
pub async fn admin_backup_list() -> Result<Vec<BackupFile>, ServerFnError> {
    require_admin().await.map_err(ServerFnError::new)?;
    Ok(list_backup_files())
}
