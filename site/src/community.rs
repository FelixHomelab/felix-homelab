//! 社区投稿（UGC）：注册用户直接发布文章 / 项目 / 光遇内容。
//!
//! 与仓库里的官方内容分开：官方内容随 Markdown 版本化，社区内容存 SQLite，
//! 由作者本人或管理员维护（**直接发布 + 事后下架/删除**，与评论的先审后发不同）。
//!
//! 正文用 `render_markdown(..., false)` 渲染：与评论同一条安全线，禁止裸 HTML，
//! 否则等于把 XSS 直接送给每个读者。

use leptos::prelude::*;
use serde::{Deserialize, Serialize};

use crate::auth::ActionResult;

/// 合法的内容类型。**用户输入，必须白名单校验**。
#[cfg(feature = "ssr")]
pub const KINDS: [&str; 3] = ["post", "project", "sky"];

#[cfg(feature = "ssr")]
const TITLE_MAX: usize = 100;
#[cfg(feature = "ssr")]
const SUMMARY_MAX: usize = 200;
#[cfg(feature = "ssr")]
const BODY_MAX: usize = 20000;
#[cfg(feature = "ssr")]
const SLUG_MAX: usize = 80;
#[cfg(feature = "ssr")]
const TAG_MAX: usize = 10;
#[cfg(feature = "ssr")]
const TAG_LEN_MAX: usize = 30;
#[cfg(feature = "ssr")]
const LINK_MAX: usize = 300;
/// 一条内容最多列出的条数（列表页）。
#[cfg(feature = "ssr")]
const LIST_LIMIT: i64 = 100;

/// 类型的中文名（前端与后台共用）。
pub fn kind_label(kind: &str) -> &'static str {
    match kind {
        "post" => "文章",
        "project" => "项目",
        "sky" => "光遇",
        _ => "内容",
    }
}

/// 列表里的一条社区内容。
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct CommunitySummary {
    pub id: i64,
    pub kind: String,
    pub slug: String,
    pub title: String,
    pub summary: String,
    pub tags: Vec<String>,
    /// 作者昵称。
    pub author: String,
    pub author_username: String,
    pub created_at: String,
}

/// 按类型存放的附加字段。
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct CommunityMeta {
    #[serde(default)]
    pub project_kind: String,
    #[serde(default)]
    pub repo: String,
    #[serde(default)]
    pub demo: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub cover: String,
}

/// 详情页数据。
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct CommunityDetail {
    pub summary: CommunitySummary,
    /// 原始 Markdown，供作者编辑时回填表单。
    pub body_md: String,
    pub body_html: String,
    pub meta: CommunityMeta,
    /// `published` 或 `hidden`。
    pub status: String,
    /// 当前访问者是否可以编辑/删除（作者本人或管理员）。
    pub can_edit: bool,
}

/// 附加字段的合法取值。
#[cfg(feature = "ssr")]
const PROJECT_KINDS: [&str; 3] = ["open", "private", "team"];
#[cfg(feature = "ssr")]
const SKY_CATEGORIES: [&str; 2] = ["gameplay", "gallery"];

/// 校验并整理一条投稿的公共输入，返回 `(slug, tags_json, meta_json)`。
#[cfg(feature = "ssr")]
fn validate_input(
    kind: &str,
    title: &str,
    summary: &str,
    body: &str,
    slug: &str,
    tags_raw: &str,
    project_kind: &str,
    repo: &str,
    demo: &str,
    category: &str,
    cover: &str,
) -> Result<(String, String, String), String> {
    if !KINDS.contains(&kind) {
        return Err("内容类型不合法。".to_string());
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

    // 链接只放行 http(s) 与站内上传路径，别的（javascript:、data: 等）一律拒绝
    let valid_link = |value: &str| {
        value.is_empty()
            || ((value.starts_with("https://")
                || value.starts_with("http://")
                || value.starts_with("/uploads/"))
                && value.chars().count() <= LINK_MAX)
    };
    if !valid_link(repo) {
        return Err("仓库链接必须是 http(s) 地址。".to_string());
    }
    if !valid_link(demo) {
        return Err("演示链接必须是 http(s) 地址。".to_string());
    }
    if !valid_link(cover) {
        return Err("封面地址必须是 http(s) 或站内上传路径。".to_string());
    }

    // slug：留空则由标题推导；推不出来（例如纯中文标题）就用时间戳兜底
    let mut slug = slug.trim().to_ascii_lowercase();
    if slug.is_empty() {
        slug = slugify(title);
    }
    if slug.is_empty() {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        slug = format!("c{ts}");
    }
    if slug.chars().count() > SLUG_MAX
        || slug.starts_with('-')
        || !slug
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err("链接名只能用字母、数字、连字符和下划线（不超过 80 字符）。".to_string());
    }

    // 标签：中英文逗号与顿号都当分隔符，去重后最多 10 个
    let mut tags: Vec<String> = Vec::new();
    for raw in tags_raw.split([',', '，', '、']) {
        let tag = raw.trim();
        if tag.is_empty() {
            continue;
        }
        if tag.chars().count() > TAG_LEN_MAX {
            return Err(format!("单个标签不能超过 {TAG_LEN_MAX} 个字符。"));
        }
        if !tags.iter().any(|existing| existing == tag) {
            tags.push(tag.to_string());
        }
    }
    if tags.len() > TAG_MAX {
        return Err(format!("标签最多 {TAG_MAX} 个。"));
    }

    let meta = match kind {
        "project" => {
            let project_kind = if PROJECT_KINDS.contains(&project_kind) {
                project_kind.to_string()
            } else {
                "open".to_string()
            };
            CommunityMeta {
                project_kind,
                repo: repo.trim().to_string(),
                demo: demo.trim().to_string(),
                ..Default::default()
            }
        }
        "sky" => {
            let category = if SKY_CATEGORIES.contains(&category) {
                category.to_string()
            } else {
                "gameplay".to_string()
            };
            CommunityMeta {
                category,
                cover: cover.trim().to_string(),
                ..Default::default()
            }
        }
        _ => CommunityMeta::default(),
    };

    let tags_json = serde_json::to_string(&tags).unwrap_or_else(|_| "[]".to_string());
    let meta_json = serde_json::to_string(&meta).unwrap_or_else(|_| "{}".to_string());
    Ok((slug, tags_json, meta_json))
}

/// 由标题推导 slug：只保留 ASCII 字母数字，其余折叠为连字符。
#[cfg(feature = "ssr")]
fn slugify(title: &str) -> String {
    let mut out = String::new();
    let mut pending_dash = false;
    for ch in title.chars() {
        if ch.is_ascii_alphanumeric() {
            if pending_dash && !out.is_empty() {
                out.push('-');
            }
            pending_dash = false;
            out.push(ch.to_ascii_lowercase());
        } else {
            pending_dash = true;
        }
    }
    out
}

/// 同作者内 slug 去重：`foo` → `foo-2` → `foo-3` …
#[cfg(feature = "ssr")]
async fn unique_slug(
    pool: &sqlx::SqlitePool,
    author_id: i64,
    mut slug: String,
    exclude_id: Option<i64>,
) -> Result<String, String> {
    for suffix in 0..20 {
        let candidate = if suffix == 0 {
            slug.clone()
        } else {
            format!("{slug}-{}", suffix + 1)
        };
        let exists: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM community_posts WHERE author_id = ?1 AND slug = ?2 \
               AND (?3 IS NULL OR id != ?3)",
        )
        .bind(author_id)
        .bind(&candidate)
        .bind(exclude_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| format!("校验链接名失败: {e}"))?;
        if exists.is_none() {
            return Ok(candidate);
        }
        if suffix == 19 {
            slug = format!("{slug}-{}", chrono::Utc::now().timestamp());
        }
    }
    Ok(slug)
}

/// 社区内容列表（已发布）。`kind` 为 `None` 时返回全部类型。
#[server]
pub async fn list_community(kind: Option<String>) -> Result<Vec<CommunitySummary>, ServerFnError> {
    use crate::state::AppState;
    use sqlx::Row;

    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");

    let kind = kind.filter(|value| !value.is_empty());
    if let Some(value) = kind.as_deref() {
        if !KINDS.contains(&value) {
            return Ok(Vec::new());
        }
    }

    let base = "SELECT c.id, c.kind, c.slug, c.title, c.summary, c.tags, c.created_at, \
                       u.display_name, u.username \
                FROM community_posts c JOIN users u ON u.id = c.author_id \
                WHERE c.status = 'published'";

    let rows = match kind.as_deref() {
        Some(value) => {
            sqlx::query(&format!(
                "{base} AND c.kind = ?1 ORDER BY c.created_at DESC, c.id DESC LIMIT ?2"
            ))
            .bind(value)
            .bind(LIST_LIMIT)
            .fetch_all(&app.pool)
            .await
        }
        None => {
            sqlx::query(&format!(
                "{base} ORDER BY c.created_at DESC, c.id DESC LIMIT ?1"
            ))
            .bind(LIST_LIMIT)
            .fetch_all(&app.pool)
            .await
        }
    }
    .map_err(|e| ServerFnError::new(format!("查询社区内容失败: {e}")))?;

    Ok(rows
        .into_iter()
        .map(|row| CommunitySummary {
            id: row.get("id"),
            kind: row.get("kind"),
            slug: row.get("slug"),
            title: row.get("title"),
            summary: row.get("summary"),
            tags: serde_json::from_str(&row.get::<String, _>("tags")).unwrap_or_default(),
            author: row.get("display_name"),
            author_username: row.get("username"),
            created_at: row.get("created_at"),
        })
        .collect())
}

/// 一篇文章的详情。隐藏内容仅作者本人与管理员可见。
#[server]
pub async fn get_community(
    username: String,
    slug: String,
) -> Result<Option<CommunityDetail>, ServerFnError> {
    use crate::state::AppState;
    use sqlx::Row;

    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");
    let username = username.trim();
    let slug = slug.trim();
    if username.is_empty() || slug.is_empty() {
        return Ok(None);
    }

    let row = sqlx::query(
        "SELECT c.id, c.kind, c.slug, c.title, c.summary, c.tags, c.body_md, c.body_html, c.meta, \
                c.status, c.created_at, c.author_id, u.display_name, u.username \
         FROM community_posts c JOIN users u ON u.id = c.author_id \
         WHERE u.username = ?1 COLLATE NOCASE AND c.slug = ?2",
    )
    .bind(username)
    .bind(slug)
    .fetch_optional(&app.pool)
    .await
    .map_err(|e| ServerFnError::new(format!("查询社区内容失败: {e}")))?;

    let Some(row) = row else {
        return Ok(None);
    };

    let author_id: i64 = row.get("author_id");
    let status: String = row.get("status");
    let viewer = crate::auth::current_identity().await;
    let is_author = viewer.as_ref().is_some_and(|identity| identity.id == author_id);
    let is_admin = viewer.as_ref().is_some_and(|identity| identity.is_admin());

    // 已下架的内容只有作者与管理员能打开，其他人一律当作不存在
    if status != "published" && !is_author && !is_admin {
        return Ok(None);
    }

    let meta: CommunityMeta =
        serde_json::from_str(&row.get::<String, _>("meta")).unwrap_or_default();

    Ok(Some(CommunityDetail {
        summary: CommunitySummary {
            id: row.get("id"),
            kind: row.get("kind"),
            slug: row.get("slug"),
            title: row.get("title"),
            summary: row.get("summary"),
            tags: serde_json::from_str(&row.get::<String, _>("tags")).unwrap_or_default(),
            author: row.get("display_name"),
            author_username: row.get("username"),
            created_at: row.get("created_at"),
        },
        body_md: row.get("body_md"),
        body_html: row.get("body_html"),
        meta,
        status,
        can_edit: is_author || is_admin,
    }))
}

/// 发布一条社区内容。成功时返回详情页地址。
#[server]
#[allow(clippy::too_many_arguments)]
pub async fn submit_community(
    kind: String,
    title: String,
    summary: String,
    body: String,
    slug: String,
    tags: String,
    project_kind: String,
    repo: String,
    demo: String,
    category: String,
    cover: String,
) -> Result<Result<String, String>, ServerFnError> {
    use crate::state::AppState;

    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");

    let Some(identity) = crate::auth::current_identity().await else {
        return Ok(Err("请先登录再发布。".to_string()));
    };

    let (slug, tags_json, meta_json) = match validate_input(
        &kind,
        &title,
        &summary,
        &body,
        &slug,
        &tags,
        &project_kind,
        &repo,
        &demo,
        &category,
        &cover,
    ) {
        Ok(values) => values,
        Err(message) => return Ok(Err(message)),
    };

    let slug = match unique_slug(&app.pool, identity.id, slug, None).await {
        Ok(slug) => slug,
        Err(message) => return Ok(Err(message)),
    };

    // 社区内容直接发布；作者本人的正文同样过滤裸 HTML
    let body_html = crate::content::render_markdown(body.trim(), false);

    sqlx::query(
        "INSERT INTO community_posts \
             (kind, slug, title, summary, tags, body_md, body_html, meta, author_id, status) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'published')",
    )
    .bind(&kind)
    .bind(&slug)
    .bind(title.trim())
    .bind(summary.trim())
    .bind(&tags_json)
    .bind(body.trim())
    .bind(&body_html)
    .bind(&meta_json)
    .bind(identity.id)
    .execute(&app.pool)
    .await
    .map_err(|e| ServerFnError::new(format!("保存社区内容失败: {e}")))?;

    let username: String = sqlx::query_scalar("SELECT username FROM users WHERE id = ?1")
        .bind(identity.id)
        .fetch_one(&app.pool)
        .await
        .map_err(|e| ServerFnError::new(format!("读取用户失败: {e}")))?;

    Ok(Ok(format!("/community/{username}/{slug}")))
}

/// 更新自己的社区内容（管理员也可）。成功时返回详情页地址。
#[server]
#[allow(clippy::too_many_arguments)]
pub async fn update_community(
    id: i64,
    title: String,
    summary: String,
    body: String,
    slug: String,
    tags: String,
    project_kind: String,
    repo: String,
    demo: String,
    category: String,
    cover: String,
) -> Result<Result<String, String>, ServerFnError> {
    use crate::state::AppState;
    use sqlx::Row;

    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");
    let Some(identity) = crate::auth::current_identity().await else {
        return Ok(Err("请先登录。".to_string()));
    };

    let row = sqlx::query("SELECT kind, author_id FROM community_posts WHERE id = ?1")
        .bind(id)
        .fetch_optional(&app.pool)
        .await
        .map_err(|e| ServerFnError::new(format!("读取社区内容失败: {e}")))?;
    let Some(row) = row else {
        return Ok(Err("内容不存在或已被删除。".to_string()));
    };
    let kind: String = row.get("kind");
    let author_id: i64 = row.get("author_id");
    if author_id != identity.id && !identity.is_admin() {
        return Ok(Err("只能编辑自己的内容。".to_string()));
    }

    let (slug, tags_json, meta_json) = match validate_input(
        &kind,
        &title,
        &summary,
        &body,
        &slug,
        &tags,
        &project_kind,
        &repo,
        &demo,
        &category,
        &cover,
    ) {
        Ok(values) => values,
        Err(message) => return Ok(Err(message)),
    };

    let slug = match unique_slug(&app.pool, author_id, slug, Some(id)).await {
        Ok(slug) => slug,
        Err(message) => return Ok(Err(message)),
    };

    let body_html = crate::content::render_markdown(body.trim(), false);

    sqlx::query(
        "UPDATE community_posts SET slug = ?1, title = ?2, summary = ?3, tags = ?4, \
             body_md = ?5, body_html = ?6, meta = ?7, updated_at = datetime('now') \
         WHERE id = ?8",
    )
    .bind(&slug)
    .bind(title.trim())
    .bind(summary.trim())
    .bind(&tags_json)
    .bind(body.trim())
    .bind(&body_html)
    .bind(&meta_json)
    .bind(id)
    .execute(&app.pool)
    .await
    .map_err(|e| ServerFnError::new(format!("更新社区内容失败: {e}")))?;

    let username: String = sqlx::query_scalar("SELECT username FROM users WHERE id = ?1")
        .bind(author_id)
        .fetch_one(&app.pool)
        .await
        .map_err(|e| ServerFnError::new(format!("读取用户失败: {e}")))?;

    Ok(Ok(format!("/community/{username}/{slug}")))
}

/// 删除自己的社区内容（管理员也可）。
#[server]
pub async fn delete_community(id: i64) -> ActionResult {
    use crate::state::AppState;
    use sqlx::Row;

    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");
    let Some(identity) = crate::auth::current_identity().await else {
        return Ok(Err("请先登录。".to_string()));
    };

    let row = sqlx::query(
        "SELECT author_id, \
                (SELECT u.username || '/' || c.slug FROM users u WHERE u.id = c.author_id) AS target \
         FROM community_posts c WHERE c.id = ?1",
    )
    .bind(id)
    .fetch_optional(&app.pool)
    .await
    .map_err(|e| ServerFnError::new(format!("读取社区内容失败: {e}")))?;
    let Some(row) = row else {
        return Ok(Err("内容不存在或已被删除。".to_string()));
    };
    let author_id: i64 = row.get("author_id");
    if author_id != identity.id && !identity.is_admin() {
        return Ok(Err("只能删除自己的内容。".to_string()));
    }

    // 评论是按 (kind, slug) 多态关联的，没有外键；这里手动一并清掉，避免留下孤儿评论
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
