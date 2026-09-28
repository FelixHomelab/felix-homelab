//! 管理权限细分。
//!
//! `users.role = 'admin'` 是**超级管理员**（Superadmin/Supermaster），拥有全部权限；
//! 细分角色存在 `user_roles` 表里，可叠加：
//!
//! | 角色            | 权限 scope | 可见后台                 |
//! | --------------- | ---------- | ------------------------ |
//! | agentmaster     | agent      | Agent 管理               |
//! | communitymaster | community  | 评论、社区               |
//! | skymaster       | sky        | 光遇评价                 |
//!
//! 「概览」对任何有管理权限的人可见；用户 / Pod / 备份仅超级管理员可见。

use leptos::prelude::*;

/// 可授予的细分角色（存 `user_roles.role`）。
pub const SCOPED_ROLES: [&str; 3] = ["agentmaster", "communitymaster", "skymaster"];

/// 角色的中文名（页面展示）。
pub fn role_label(role: &str) -> &'static str {
    match role {
        "admin" => "超级管理员",
        "agentmaster" => "Agent 管理员",
        "communitymaster" => "社区管理员",
        "skymaster" => "光遇管理员",
        _ => "普通用户",
    }
}

/// 细分角色 → 权限 scope。
#[cfg(feature = "ssr")]
pub fn scope_of_role(role: &str) -> Option<&'static str> {
    match role {
        "agentmaster" => Some("agent"),
        "communitymaster" => Some("community"),
        "skymaster" => Some("sky"),
        _ => None,
    }
}

/// 该用户持有的细分角色名（原始值，用于用户管理页展示/切换）。
#[cfg(feature = "ssr")]
pub async fn roles_of(pool: &sqlx::SqlitePool, user_id: i64) -> Vec<String> {
    sqlx::query_scalar::<_, String>("SELECT role FROM user_roles WHERE user_id = ?1 ORDER BY role")
        .bind(user_id)
        .fetch_all(pool)
        .await
        .unwrap_or_default()
}

/// 该用户实际拥有的权限 scope 集合。
#[cfg(feature = "ssr")]
pub async fn scopes_of(pool: &sqlx::SqlitePool, user_id: i64) -> Vec<String> {
    roles_of(pool, user_id)
        .await
        .into_iter()
        .filter_map(|role| scope_of_role(&role).map(str::to_string))
        .collect()
}

/// 任意管理权限（包括纯细分角色）都通过；用于「概览」这类聚合页。
#[cfg(feature = "ssr")]
pub async fn require_staff() -> Result<i64, String> {
    use crate::state::AppState;

    let Some(identity) = crate::auth::current_identity().await else {
        return Err("请先登录。".to_string());
    };
    if identity.is_admin() {
        return Ok(identity.id);
    }
    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");
    if scopes_of(&app.pool, identity.id).await.is_empty() {
        return Err("需要管理权限。".to_string());
    }
    Ok(identity.id)
}

/// 超级管理员或持有指定 scope 的细分管理员。
#[cfg(feature = "ssr")]
pub async fn require_permission(perm: &str) -> Result<i64, String> {
    use crate::state::AppState;

    let Some(identity) = crate::auth::current_identity().await else {
        return Err("请先登录。".to_string());
    };
    if identity.is_admin() {
        return Ok(identity.id);
    }
    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");
    if scopes_of(&app.pool, identity.id).await.iter().any(|s| s == perm) {
        return Ok(identity.id);
    }
    Err("需要对应的管理权限。".to_string())
}

/// 当前用户的后台权限：`["super"]` = 超级管理员；否则是 scope 列表（可能为空）。
#[server]
pub async fn admin_permissions() -> Result<Vec<String>, ServerFnError> {
    use crate::state::AppState;

    let Some(identity) = crate::auth::current_identity().await else {
        return Ok(Vec::new());
    };
    if identity.is_admin() {
        return Ok(vec!["super".to_string()]);
    }
    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");
    Ok(scopes_of(&app.pool, identity.id).await)
}
