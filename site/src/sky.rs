//! 光遇板块的服务端逻辑：官方内容（站内可编辑）、代跑页展示设置、光遇投稿管理。
//!
//! 光遇是独立于社区的游戏板块：投稿（`community_posts.kind = 'sky'`）在光遇板块
//! 展示与发布，代跑评价的精选与公告也在这里维护（`sky_boosting`）。

use leptos::prelude::*;
use serde::{Deserialize, Serialize};

use crate::community::CommunitySummary;

/// 官方内容的分类白名单（攻略 / 画廊）。
pub const SKY_CATEGORIES: [&str; 2] = ["gameplay", "gallery"];

#[cfg(feature = "ssr")]
const TITLE_MAX: usize = 100;
#[cfg(feature = "ssr")]
const SUMMARY_MAX: usize = 200;
#[cfg(feature = "ssr")]
const BODY_MAX: usize = 20000;
#[cfg(feature = "ssr")]
const SLUG_MAX: usize = 80;
#[cfg(feature = "ssr")]
const LINK_MAX: usize = 300;

/// 一条官方内容（列表用）。
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct SkyOfficial {
    pub id: i64,
    pub category: String,
    pub slug: String,
    pub title: String,
    pub summary: String,
    pub cover: String,
    pub sort: i64,
    pub status: String,
    pub updated_at: String,
    /// 原始 Markdown：仅后台列表返回（前台列表为空，详情用 `SkyOfficialDetail`）。
    #[serde(default)]
    pub body_md: String,
}

/// 官方内容详情。
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct SkyOfficialDetail {
    pub item: SkyOfficial,
    pub body_md: String,
    pub body_html: String,
}

/// 代跑页展示设置。
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct SkyBoostingInfo {
    pub announcement: String,
}

#[cfg(feature = "ssr")]
fn slugify(title: &str) -> String {
    let mut out = String::new();
    let mut pending = false;
    for ch in title.chars() {
        if ch.is_ascii_alphanumeric() {
            if pending && !out.is_empty() {
                out.push('-');
            }
            pending = false;
            out.push(ch.to_ascii_lowercase());
        } else {
            pending = true;
        }
    }
    out
}

/// 校验官方内容输入。
#[cfg(feature = "ssr")]
#[allow(clippy::too_many_arguments)]
fn validate_official(
    category: &str,
    slug: &str,
    title: &str,
    summary: &str,
    body: &str,
    cover: &str,
    sort: i64,
    status: &str,
) -> Result<(String, String, String, String, String, String, i64, String), String> {
    let category = category.trim();
    if !SKY_CATEGORIES.contains(&category) {
        return Err("分类不合法。".to_string());
    }
    if !matches!(status, "published" | "hidden") {
        return Err("状态不合法。".to_string());
    }

    let title = title.trim();
    if title.is_empty() {
        return Err("标题不能为空。".to_string());
    }
    if title.chars().count() > TITLE_MAX {
        return Err(format!("标题不能超过 {TITLE_MAX} 个字符。"));
    }
    if summary.trim().chars().count() > SUMMARY_MAX {
        return Err(format!("摘要不能超过 {SUMMARY_MAX} 个字符。"));
    }
    let body = body.trim();
    if body.is_empty() {
        return Err("正文不能为空。".to_string());
    }
    if body.chars().count() > BODY_MAX {
        return Err(format!("正文不能超过 {BODY_MAX} 个字符。"));
    }

    let cover = cover.trim();
    if !cover.is_empty()
        && !(cover.starts_with("https://")
            || cover.starts_with("http://")
            || cover.starts_with("/uploads/"))
    {
        return Err("封面地址必须是 http(s) 或站内上传路径。".to_string());
    }
    if cover.chars().count() > LINK_MAX {
        return Err("封面地址过长。".to_string());
    }

    let mut slug = slug.trim().to_ascii_lowercase();
    if slug.is_empty() {
        slug = slugify(title);
    }
    if slug.is_empty() {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        slug = format!("s{ts}");
    }
    if slug.chars().count() > SLUG_MAX
        || slug.starts_with('-')
        || !slug
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err("链接名只能用字母、数字、连字符和下划线（不超过 80 字符）。".to_string());
    }

    Ok((
        category.to_string(),
        slug,
        title.to_string(),
        summary.trim().to_string(),
        body.to_string(),
        cover.to_string(),
        sort.clamp(-9999, 9999),
        status.to_string(),
    ))
    .map(|(c, s, t, sum, b, cv, so, st)| (c, s, t, sum, b, cv, so, st))
    .map(|(c, s, t, sum, b, cv, so, st)| (c, s, t, sum, b, cv, so, st))
    .map(|(c, s, t, sum, b, cv, so, st)| (c, s, t.clone(), sum, b, cv, so, st))
}

/// 已发布的官方内容列表（按 sort 倒序，其次新在前）。
#[server]
pub async fn list_sky_official(category: String) -> Result<Vec<SkyOfficial>, ServerFnError> {
    use crate::state::AppState;
    use sqlx::Row;

    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");
    let category = category.trim();
    if !SKY_CATEGORIES.contains(&category) {
        return Ok(Vec::new());
    }

    let rows = sqlx::query(
        "SELECT id, category, slug, title, summary, cover, sort, status, updated_at \
         FROM sky_official WHERE status = 'published' AND category = ?1 \
         ORDER BY sort DESC, id DESC",
    )
    .bind(category)
    .fetch_all(&app.pool)
    .await
    .map_err(|e| ServerFnError::new(format!("查询光遇官方内容失败: {e}")))?;

    Ok(rows
        .iter()
        .map(|row| SkyOfficial {
            id: row.get("id"),
            category: row.get("category"),
            slug: row.get("slug"),
            title: row.get("title"),
            summary: row.get("summary"),
            cover: row.get("cover"),
            sort: row.get("sort"),
            status: row.get("status"),
            updated_at: row.get("updated_at"),
            body_md: String::new(),
        })
        .collect())
}

/// 官方内容详情（隐藏的仅管理员可见）。
#[server]
pub async fn get_sky_official(
    category: String,
    slug: String,
) -> Result<Option<SkyOfficialDetail>, ServerFnError> {
    use crate::state::AppState;
    use sqlx::Row;

    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");
    let row = sqlx::query(
        "SELECT id, category, slug, title, summary, body_md, body_html, cover, sort, status, \
                updated_at \
         FROM sky_official WHERE category = ?1 AND slug = ?2",
    )
    .bind(category.trim())
    .bind(slug.trim())
    .fetch_optional(&app.pool)
    .await
    .map_err(|e| ServerFnError::new(format!("查询光遇官方内容失败: {e}")))?;

    let Some(row) = row else {
        return Ok(None);
    };
    let status: String = row.get("status");
    let is_sky_admin = crate::roles::require_permission("sky").await.is_ok();
    if status != "published" && !is_sky_admin {
        return Ok(None);
    }

    let item = SkyOfficial {
        id: row.get("id"),
        category: row.get("category"),
        slug: row.get("slug"),
        title: row.get("title"),
        summary: row.get("summary"),
        cover: row.get("cover"),
        sort: row.get("sort"),
        status,
        updated_at: row.get("updated_at"),
        body_md: String::new(),
    };
    Ok(Some(SkyOfficialDetail {
        item,
        body_md: row.get("body_md"),
        body_html: row.get("body_html"),
    }))
}

/// 代跑页展示设置（公告）。
#[server]
pub async fn sky_boosting() -> Result<SkyBoostingInfo, ServerFnError> {
    use crate::state::AppState;

    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");
    let announcement: String =
        sqlx::query_scalar("SELECT value FROM sky_boosting WHERE key = 'announcement'")
            .fetch_optional(&app.pool)
            .await
            .map_err(|e| ServerFnError::new(format!("读取光遇展示设置失败: {e}")))?
            .unwrap_or_default();
    Ok(SkyBoostingInfo { announcement })
}

/// 光遇社区投稿列表：加精优先，其次新在前。
#[server]
pub async fn list_sky_community() -> Result<Vec<CommunitySummary>, ServerFnError> {
    use crate::state::AppState;

    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");
    let rows = sqlx::query(
        "SELECT c.id, c.kind, c.slug, c.title, c.summary, c.tags, c.created_at, \
                COALESCE(json_extract(c.meta, '$.featured'), 0) AS featured, \
                u.display_name, u.username \
         FROM community_posts c JOIN users u ON u.id = c.author_id \
         WHERE c.status = 'published' AND c.kind = 'sky' \
         ORDER BY featured DESC, c.created_at DESC, c.id DESC LIMIT 100",
    )
    .fetch_all(&app.pool)
    .await
    .map_err(|e| ServerFnError::new(format!("查询光遇投稿失败: {e}")))?;

    Ok(rows.iter().map(crate::community::summary_from_row).collect())
}

// ---------------------------------------------------------------------------
// 后台（光遇管理员 scope = sky）
// ---------------------------------------------------------------------------

/// 后台：全部官方内容（含隐藏）。
#[server]
pub async fn admin_list_sky_official() -> Result<Vec<SkyOfficial>, ServerFnError> {
    use crate::state::AppState;
    use sqlx::Row;

    if let Err(message) = crate::roles::require_permission("sky").await {
        return Err(ServerFnError::new(message));
    }
    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");
    let rows = sqlx::query(
        "SELECT id, category, slug, title, summary, body_md, cover, sort, status, updated_at \
         FROM sky_official ORDER BY category, sort DESC, id DESC",
    )
    .fetch_all(&app.pool)
    .await
    .map_err(|e| ServerFnError::new(format!("查询光遇官方内容失败: {e}")))?;

    Ok(rows
        .iter()
        .map(|row| SkyOfficial {
            id: row.get("id"),
            category: row.get("category"),
            slug: row.get("slug"),
            title: row.get("title"),
            summary: row.get("summary"),
            cover: row.get("cover"),
            sort: row.get("sort"),
            status: row.get("status"),
            updated_at: row.get("updated_at"),
            body_md: row.get("body_md"),
        })
        .collect())
}

/// 后台：新建或更新一条官方内容。返回内容 id。
#[server]
#[allow(clippy::too_many_arguments)]
pub async fn admin_save_sky_official(
    id: Option<i64>,
    category: String,
    slug: String,
    title: String,
    summary: String,
    body: String,
    cover: String,
    sort: i64,
    status: String,
) -> Result<Result<i64, String>, ServerFnError> {
    use crate::state::AppState;

    if let Err(message) = crate::roles::require_permission("sky").await {
        return Ok(Err(message));
    }
    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");

    let (category, slug, title, summary, body, cover, sort, status) = match validate_official(
        &category, &slug, &title, &summary, &body, &cover, sort, &status,
    ) {
        Ok(values) => values,
        Err(message) => return Ok(Err(message)),
    };

    let body_html = crate::content::render_markdown(&body, false);
    let author_id = crate::auth::current_identity().await.map(|identity| identity.id);

    let result = match id {
        Some(id) => sqlx::query(
            "UPDATE sky_official SET category = ?1, slug = ?2, title = ?3, summary = ?4, \
                    body_md = ?5, body_html = ?6, cover = ?7, sort = ?8, status = ?9, \
                    updated_at = datetime('now') WHERE id = ?10",
        )
        .bind(&category)
        .bind(&slug)
        .bind(&title)
        .bind(&summary)
        .bind(&body)
        .bind(&body_html)
        .bind(&cover)
        .bind(sort)
        .bind(&status)
        .bind(id)
        .execute(&app.pool)
        .await
        .map(|_| id),
        None => sqlx::query(
            "INSERT INTO sky_official \
                 (category, slug, title, summary, body_md, body_html, cover, sort, status, author_id) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        )
        .bind(&category)
        .bind(&slug)
        .bind(&title)
        .bind(&summary)
        .bind(&body)
        .bind(&body_html)
        .bind(&cover)
        .bind(sort)
        .bind(&status)
        .bind(author_id)
        .execute(&app.pool)
        .await
        .map(|result| result.last_insert_rowid()),
    };

    match result {
        Ok(id) => Ok(Ok(id)),
        Err(sqlx::Error::Database(db))
            if db.is_unique_violation() =>
        {
            Ok(Err("同一分类下链接名重复，换一个链接名。".to_string()))
        }
        Err(e) => Ok(Err(format!("保存失败: {e}"))),
    }
}

/// 后台：删除一条官方内容。
#[server]
pub async fn admin_delete_sky_official(id: i64) -> Result<Result<(), String>, ServerFnError> {
    use crate::state::AppState;

    if let Err(message) = crate::roles::require_permission("sky").await {
        return Ok(Err(message));
    }
    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");
    sqlx::query("DELETE FROM sky_official WHERE id = ?1")
        .bind(id)
        .execute(&app.pool)
        .await
        .map_err(|e| ServerFnError::new(format!("删除失败: {e}")))?;
    Ok(Ok(()))
}

/// 后台：保存代跑页公告（空字符串 = 不显示）。
#[server]
pub async fn admin_save_sky_boosting(
    announcement: String,
) -> Result<Result<(), String>, ServerFnError> {
    use crate::state::AppState;

    if let Err(message) = crate::roles::require_permission("sky").await {
        return Ok(Err(message));
    }
    if announcement.chars().count() > 4000 {
        return Ok(Err("公告不能超过 4000 个字符。".to_string()));
    }
    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");
    sqlx::query(
        "INSERT INTO sky_boosting (key, value) VALUES ('announcement', ?1) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(announcement.trim())
    .execute(&app.pool)
    .await
    .map_err(|e| ServerFnError::new(format!("保存失败: {e}")))?;
    Ok(Ok(()))
}

/// 后台：光遇投稿列表（`published` / `hidden` / `all`）。
#[server]
pub async fn admin_list_sky_posts(status: String) -> Result<Vec<CommunitySummary>, ServerFnError> {
    use crate::state::AppState;

    if let Err(message) = crate::roles::require_permission("sky").await {
        return Err(ServerFnError::new(message));
    }
    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");
    let status = status.trim();
    let filter = match status {
        "published" | "hidden" => " AND c.status = ?1",
        _ => "",
    };
    let sql = format!(
        "SELECT c.id, c.kind, c.slug, c.title, c.summary, c.tags, c.created_at, \
                COALESCE(json_extract(c.meta, '$.featured'), 0) AS featured, \
                u.display_name, u.username \
         FROM community_posts c JOIN users u ON u.id = c.author_id \
         WHERE c.kind = 'sky'{filter} \
         ORDER BY featured DESC, c.created_at DESC, c.id DESC"
    );
    let query = sqlx::query(&sql);
    let query = match status {
        "published" | "hidden" => query.bind(status),
        _ => query,
    };
    let rows = query
        .fetch_all(&app.pool)
        .await
        .map_err(|e| ServerFnError::new(format!("查询光遇投稿失败: {e}")))?;

    Ok(rows.iter().map(crate::community::summary_from_row).collect())
}

/// 后台：给光遇投稿加精 / 取消加精。
#[server]
pub async fn admin_feature_sky_post(id: i64, featured: bool) -> Result<Result<(), String>, ServerFnError> {
    use crate::state::AppState;
    use sqlx::Row;

    if let Err(message) = crate::roles::require_permission("sky").await {
        return Ok(Err(message));
    }
    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");

    let row = sqlx::query("SELECT kind, meta FROM community_posts WHERE id = ?1")
        .bind(id)
        .fetch_optional(&app.pool)
        .await
        .map_err(|e| ServerFnError::new(format!("读取投稿失败: {e}")))?;
    let Some(row) = row else {
        return Ok(Err("投稿不存在。".to_string()));
    };
    if row.get::<String, _>("kind") != "sky" {
        return Ok(Err("只能给光遇投稿加精。".to_string()));
    }

    let mut meta: crate::community::CommunityMeta =
        serde_json::from_str(&row.get::<String, _>("meta")).unwrap_or_default();
    meta.featured = featured;
    sqlx::query("UPDATE community_posts SET meta = ?1 WHERE id = ?2")
        .bind(serde_json::to_string(&meta).unwrap_or_else(|_| "{}".to_string()))
        .bind(id)
        .execute(&app.pool)
        .await
        .map_err(|e| ServerFnError::new(format!("更新失败: {e}")))?;
    Ok(Ok(()))
}

/// 后台：把一条代跑评价设为精选 / 取消精选。
#[server]
pub async fn admin_pin_review(id: i64, pinned: bool) -> Result<Result<(), String>, ServerFnError> {
    use crate::state::AppState;

    if let Err(message) = crate::roles::require_permission("sky").await {
        return Ok(Err(message));
    }
    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");

    let raw: String =
        sqlx::query_scalar("SELECT value FROM sky_boosting WHERE key = 'featured_reviews'")
            .fetch_optional(&app.pool)
            .await
            .map_err(|e| ServerFnError::new(format!("读取精选失败: {e}")))?
            .unwrap_or_else(|| "[]".to_string());
    let mut ids: Vec<i64> = serde_json::from_str(&raw).unwrap_or_default();
    if pinned {
        if !ids.contains(&id) {
            ids.push(id);
        }
    } else {
        ids.retain(|x| *x != id);
    }
    sqlx::query(
        "INSERT INTO sky_boosting (key, value) VALUES ('featured_reviews', ?1) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(serde_json::to_string(&ids).unwrap_or_else(|_| "[]".to_string()))
    .execute(&app.pool)
    .await
    .map_err(|e| ServerFnError::new(format!("保存失败: {e}")))?;
    Ok(Ok(()))
}
