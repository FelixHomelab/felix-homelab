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
    /// 光遇投稿的“加精”（其它类型恒为 false）。
    #[serde(default)]
    pub featured: bool,
}

/// 按类型存放的附加字段。
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct CommunityMeta {
    /// 光遇投稿加精标记。
    #[serde(default)]
    pub featured: bool,
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

/// 标签（聚合页与后台管理共用）。
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct CommunityTag {
    /// 规范化后的标签原文（投稿里存的值）。
    pub tag: String,
    /// 显示名（可被管理员重命名）。
    pub display_name: String,
    /// 使用该标签的已发布内容数。
    pub count: i64,
    /// 是否为预置分类（发布页会展示）。
    pub preset: bool,
    /// 是否置顶（聚合页与管理列表优先展示）。
    pub pinned: bool,
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

/// 从正文里提取 `#分类`。
///
/// 只在空白/行首/左括号/另一个 # 之后识别，避免把 URL 锚点或 Markdown 标题当成标签；
/// 汉字、字母数字与 -_ 都允许，长度受 TAG_LEN_MAX 限制。
#[cfg(feature = "ssr")]
fn collect_hashtags(body: &str) -> Vec<String> {
    let chars: Vec<char> = body.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '#' {
            i += 1;
            continue;
        }
        let prev_ok = i == 0
            || chars[i - 1].is_whitespace()
            || matches!(chars[i - 1], '(' | '（' | '#' | '>' | '"' | '“');
        let mut j = i + 1;
        let mut buf = String::new();
        while j < chars.len() {
            let c = chars[j];
            if c.is_whitespace()
                || matches!(
                    c,
                    '#' | ',' | '，' | '、' | '。' | '！' | '？' | '!' | '?' | '；' | ';' | '：'
                        | ':' | '）' | ')'
                )
            {
                break;
            }
            buf.push(c);
            j += 1;
        }
        let tag = buf.trim_matches(|c: char| matches!(c, ')' | '）' | '"' | '\'' | '“' | '”'));
        if prev_ok && !tag.is_empty() && tag.chars().count() <= TAG_LEN_MAX {
            out.push(tag.to_string());
        }
        i = j.max(i + 1);
    }
    out
}

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

    // 标签：预选（逗号/顿号分隔）+ 正文里的 `#分类`，去重后最多 10 个
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
    for tag in collect_hashtags(body) {
        if !tags.iter().any(|existing| existing == &tag) {
            tags.push(tag);
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
                       COALESCE(json_extract(c.meta, '$.featured'), 0) AS featured, \
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
            // 光遇是独立板块：默认列表（社区/首页概览）不混入光遇内容
            sqlx::query(&format!(
                "{base} AND c.kind != 'sky' ORDER BY c.created_at DESC, c.id DESC LIMIT ?1"
            ))
            .bind(LIST_LIMIT)
            .fetch_all(&app.pool)
            .await
        }
    }
    .map_err(|e| ServerFnError::new(format!("查询社区内容失败: {e}")))?;

    Ok(rows.iter().map(summary_from_row).collect())
}

/// 行 → 列表项（各查询共用）。
#[cfg(feature = "ssr")]
pub(crate) fn summary_from_row(row: &sqlx::sqlite::SqliteRow) -> CommunitySummary {
    use sqlx::Row;
    CommunitySummary {
        id: row.get("id"),
        kind: row.get("kind"),
        slug: row.get("slug"),
        title: row.get("title"),
        summary: row.get("summary"),
        tags: serde_json::from_str(&row.get::<String, _>("tags")).unwrap_or_default(),
        author: row.get("display_name"),
        author_username: row.get("username"),
        created_at: row.get("created_at"),
        featured: row.get::<i64, _>("featured") != 0,
    }
}

/// 标签聚合的内部实现（server fn 与后台共用）。
#[cfg(feature = "ssr")]
async fn list_tags_impl(pool: &sqlx::SqlitePool) -> Result<Vec<CommunityTag>, String> {
    use sqlx::Row;

    let counts = sqlx::query(
        "SELECT json_each.value AS tag, COUNT(*) AS n \
         FROM community_posts, json_each(community_posts.tags) \
         WHERE community_posts.status = 'published' \
         GROUP BY json_each.value",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("统计标签失败: {e}"))?;

    let metas = sqlx::query("SELECT tag, display_name, preset, pinned FROM community_tags")
        .fetch_all(pool)
        .await
        .map_err(|e| format!("读取标签失败: {e}"))?;

    let mut list: Vec<CommunityTag> = metas
        .iter()
        .map(|row| CommunityTag {
            tag: row.get("tag"),
            display_name: row.get("display_name"),
            count: 0,
            preset: row.get::<i64, _>("preset") != 0,
            pinned: row.get::<i64, _>("pinned") != 0,
        })
        .collect();

    for row in counts {
        let tag: String = row.get("tag");
        let n: i64 = row.get("n");
        match list.iter_mut().find(|t| t.tag == tag) {
            Some(entry) => entry.count = n,
            None => list.push(CommunityTag {
                display_name: tag.clone(),
                tag,
                count: n,
                preset: false,
                pinned: false,
            }),
        }
    }

    // 置顶优先，其次常用优先，再次按名字
    list.sort_by(|a, b| {
        b.pinned
            .cmp(&a.pinned)
            .then(b.count.cmp(&a.count))
            .then(a.display_name.cmp(&b.display_name))
    });
    Ok(list)
}

/// 标签聚合：预置分类 + 使用中的标签（置顶/常用优先）。
#[server]
pub async fn list_community_tags() -> Result<Vec<CommunityTag>, ServerFnError> {
    use crate::state::AppState;

    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");
    list_tags_impl(&app.pool)
        .await
        .map_err(ServerFnError::new)
}

/// 按标签筛选的社区内容（已发布）。
#[server]
pub async fn list_community_by_tag(tag: String) -> Result<Vec<CommunitySummary>, ServerFnError> {
    use crate::state::AppState;

    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");
    let tag = tag.trim();
    if tag.is_empty() {
        return Ok(Vec::new());
    }

    let rows = sqlx::query(
        "SELECT c.id, c.kind, c.slug, c.title, c.summary, c.tags, c.created_at, \
                COALESCE(json_extract(c.meta, '$.featured'), 0) AS featured, \
                u.display_name, u.username \
         FROM community_posts c JOIN users u ON u.id = c.author_id \
         WHERE c.status = 'published' \
           AND EXISTS (SELECT 1 FROM json_each(c.tags) WHERE json_each.value = ?1) \
         ORDER BY c.created_at DESC, c.id DESC LIMIT ?2",
    )
    .bind(tag)
    .bind(LIST_LIMIT)
    .fetch_all(&app.pool)
    .await
    .map_err(|e| ServerFnError::new(format!("查询标签内容失败: {e}")))?;

    Ok(rows.iter().map(summary_from_row).collect())
}

/// 社区页的“光遇随机”：随机几则光遇内容当作娱乐缓冲。
#[server]
pub async fn random_sky_teasers(limit: i64) -> Result<Vec<CommunitySummary>, ServerFnError> {
    use crate::state::AppState;

    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");
    let limit = limit.clamp(1, 10);

    let rows = sqlx::query(
        "SELECT c.id, c.kind, c.slug, c.title, c.summary, c.tags, c.created_at, \
                COALESCE(json_extract(c.meta, '$.featured'), 0) AS featured, \
                u.display_name, u.username \
         FROM community_posts c JOIN users u ON u.id = c.author_id \
         WHERE c.status = 'published' AND c.kind = 'sky' \
         ORDER BY RANDOM() LIMIT ?1",
    )
    .bind(limit)
    .fetch_all(&app.pool)
    .await
    .map_err(|e| ServerFnError::new(format!("查询光遇内容失败: {e}")))?;

    Ok(rows.iter().map(summary_from_row).collect())
}

/// 后台：标签列表（与前台同一套口径）。
#[server]
pub async fn admin_list_tags() -> Result<Vec<CommunityTag>, ServerFnError> {
    use crate::state::AppState;

    if let Err(message) = crate::roles::require_permission("community").await {
        return Err(ServerFnError::new(message));
    }
    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");
    list_tags_impl(&app.pool)
        .await
        .map_err(ServerFnError::new)
}

/// 后台：重命名 / 置顶标签（标签不存在则创建元数据行）。
#[server]
pub async fn admin_save_tag(
    tag: String,
    display_name: String,
    pinned: bool,
) -> Result<Result<(), String>, ServerFnError> {
    use crate::state::AppState;

    if let Err(message) = crate::roles::require_permission("community").await {
        return Ok(Err(message));
    }
    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");

    let tag = tag.trim();
    let name = display_name.trim();
    if tag.is_empty() {
        return Ok(Err("标签不能为空。".to_string()));
    }
    if name.is_empty() {
        return Ok(Err("显示名不能为空。".to_string()));
    }
    if name.chars().count() > TAG_LEN_MAX {
        return Ok(Err(format!("标签不能超过 {TAG_LEN_MAX} 个字符。")));
    }

    sqlx::query(
        "INSERT INTO community_tags (tag, display_name, preset, pinned) VALUES (?1, ?2, 0, ?3) \
         ON CONFLICT(tag) DO UPDATE SET display_name = excluded.display_name, \
                                        pinned = excluded.pinned",
    )
    .bind(tag)
    .bind(name)
    .bind(i64::from(pinned))
    .execute(&app.pool)
    .await
    .map_err(|e| ServerFnError::new(format!("保存标签失败: {e}")))?;

    Ok(Ok(()))
}

/// 后台：把 `from` 合并进 `into`（改写所有投稿的标签数组，删除旧标签元数据）。
#[server]
pub async fn admin_merge_tag(
    from: String,
    into: String,
) -> Result<Result<(), String>, ServerFnError> {
    use crate::state::AppState;
    use sqlx::Row;

    if let Err(message) = crate::roles::require_permission("community").await {
        return Ok(Err(message));
    }
    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");

    let from = from.trim();
    let into = into.trim();
    if from.is_empty() || into.is_empty() {
        return Ok(Err("请选择要合并的标签。".to_string()));
    }
    if from == into {
        return Ok(Err("目标标签不能和原标签相同。".to_string()));
    }
    if into.chars().count() > TAG_LEN_MAX {
        return Ok(Err(format!("标签不能超过 {TAG_LEN_MAX} 个字符。")));
    }

    let rows = sqlx::query("SELECT id, tags FROM community_posts")
        .fetch_all(&app.pool)
        .await
        .map_err(|e| ServerFnError::new(format!("读取投稿失败: {e}")))?;

    for row in rows {
        let tags: Vec<String> =
            serde_json::from_str(&row.get::<String, _>("tags")).unwrap_or_default();
        if !tags.iter().any(|t| t == from) {
            continue;
        }
        let mut next: Vec<String> = tags.into_iter().filter(|t| t != from).collect();
        if !next.iter().any(|t| t == into) {
            next.push(into.to_string());
        }
        let id: i64 = row.get("id");
        sqlx::query("UPDATE community_posts SET tags = ?1 WHERE id = ?2")
            .bind(serde_json::to_string(&next).unwrap_or_else(|_| "[]".to_string()))
            .bind(id)
            .execute(&app.pool)
            .await
            .map_err(|e| ServerFnError::new(format!("更新投稿标签失败: {e}")))?;
    }

    sqlx::query("DELETE FROM community_tags WHERE tag = ?1")
        .bind(from)
        .execute(&app.pool)
        .await
        .map_err(|e| ServerFnError::new(format!("删除旧标签失败: {e}")))?;
    sqlx::query(
        "INSERT OR IGNORE INTO community_tags (tag, display_name, preset) VALUES (?1, ?1, 0)",
    )
    .bind(into)
    .execute(&app.pool)
    .await
    .map_err(|e| ServerFnError::new(format!("写入目标标签失败: {e}")))?;

    Ok(Ok(()))
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
                COALESCE(json_extract(c.meta, '$.featured'), 0) AS featured, \
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
            featured: row.get::<i64, _>("featured") != 0,
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

    let prefix = if kind == "sky" { "/sky/community" } else { "/community" };
    Ok(Ok(format!("{prefix}/{username}/{slug}")))
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

    let prefix = if kind == "sky" { "/sky/community" } else { "/community" };
    Ok(Ok(format!("{prefix}/{username}/{slug}")))
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
