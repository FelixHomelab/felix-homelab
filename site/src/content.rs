//! 内容层：把仓库里的 Markdown 变成页面能用的数据。
//!
//! 分三块，边界很清楚：
//!
//! 1. **数据结构**（`PostSummary`、`PostDetail` 等）——两端都编译，因为它们是
//!    server function 的返回类型，wasm 侧也要能反序列化。
//! 2. **载入与索引**（`store` 模块）——只在服务端编译。内容在进程启动时一次性
//!    读入内存，代价是 `git pull` 后要重启进程，这与发布流程本来就是一回事。
//! 3. **server function**——喂给前端的唯一入口。前端不直接读文件，
//!    于是 wasm 包里不会夹带整站内容。
//!
//! 设计见 `DESIGN.md` 第三节。

// `#[server]` 属性宏由 leptos_macro 提供，经 leptos::prelude 带进来
use leptos::prelude::*;

/// 内容没写 `author` 时的默认发布者——填**站点账号的用户名**。
///
/// 单人站点的内容都归站长，给个默认值比逼每篇都写一行更省事；将来多人发布时，
/// 各自在自己的文件里写自己的用户名即可，这个默认值只兜底。
/// 解析不到同名账号时，页面上只显示这个名字、不给链接。
pub const DEFAULT_AUTHOR: &str = "Felix";
use serde::{Deserialize, Serialize};

/// 解析到的发布者账号。
///
/// 页面上要的是**显示名**与**能用的链接**，而这两个都不是 front matter 里那串
/// 用户名本身：显示名在 `users` 表里，用户名的大小写也以库里的写法为准。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AuthorAccount {
    /// 库里存的用户名。链接用它，别用 front matter 里的写法——否则同一个账号会
    /// 同时存在 `/user/Felix` 与 `/user/felix` 两个网址。
    pub username: String,
    pub display_name: String,
}

/// 列表页用的文章摘要，不含正文——列表页没必要把全文传下去。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PostSummary {
    pub slug: String,
    pub title: String,
    pub date: String,
    pub summary: String,
    pub tags: Vec<String>,
    pub reading_minutes: u32,
    /// 发布者的站点账号用户名（front matter 的 `author`）。
    pub author: String,
}

/// 正文页用的文章详情：摘要字段加渲染后的 HTML。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PostDetail {
    pub summary: PostSummary,
    pub html: String,
    /// 发布者的显示名，从站点账号解析而来。
    ///
    /// `None` 表示站内没有这个账号——页面上就只显示用户名、不给链接，
    /// 而不是把人链到一个 404。
    pub author_account: Option<AuthorAccount>,
}

/// 项目条目。`kind` 与旧站的分类保持一致（开源 / 私有 / 团队）。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProjectSummary {
    pub slug: String,
    pub name: String,
    pub kind: String,
    pub summary: String,
    pub stack: Vec<String>,
    pub repo: Option<String>,
    pub demo: Option<String>,
    pub weight: i64,
    /// 发布者的站点账号用户名。
    pub author: String,
}

/// 项目详情：摘要字段加渲染后的 HTML。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProjectDetail {
    pub summary: ProjectSummary,
    pub html: String,
    /// 同 [`PostDetail::author_account`]。
    pub author_account: Option<AuthorAccount>,
}

/// 一个静态页（关于、联系等）。内容在 `content/pages/` 下。
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct PageDetail {
    pub slug: String,
    pub title: String,
    /// 最后更新日期，`YYYY-MM-DD`；页脚会显示它。
    pub updated: String,
    pub html: String,
}

/// 光遇的一条内容（攻略或画廊条目）。内容在 `content/sky/` 下。
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct SkyItem {
    pub slug: String,
    pub title: String,
    pub date: String,
    /// `gameplay`（攻略）或 `gallery`（画廊）。
    pub category: String,
    pub summary: String,
    pub cover: Option<String>,
    pub html: String,
    /// 发布者的站点账号用户名。
    pub author: String,
    /// 发布者的显示名；站内没有该账号时为 None。
    pub author_account: Option<AuthorAccount>,
}

/// Markdown 转 HTML。
///
/// `allow_html` 决定是否放行原文里的裸 HTML：仓库里的文章由站长自己写，可以放行；
/// **用户提交的内容（评论、评价）必须传 `false`**，否则等于把 XSS 直接送给访客。
/// pulldown-cmark 默认会原样透传裸 HTML，所以这里的过滤是必须的，不是可选加固。
#[cfg(feature = "ssr")]
pub fn render_markdown(md: &str, allow_html: bool) -> String {
    use pulldown_cmark::{html, Event, Options, Parser, Tag, TagEnd};

    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_FOOTNOTES);
    // 刻意不开 ENABLE_SMART_PUNCTUATION：它会把中文引号与破折号改写成英文标点
    let parser = Parser::new_ext(md, options).map(move |event| match event {
        Event::Html(_) | Event::InlineHtml(_) if !allow_html => Event::Text("".into()),
        other => other,
    });

    // 媒体链接改写：指向本站 /media/ 的音频链接渲染成 <audio> 播放条
    // （语音消息）。URL 白名单 + 转义，用户内容（allow_html=false）同样安全。
    let events: Vec<Event> = parser.collect();
    let mut rewritten: Vec<Event> = Vec::with_capacity(events.len());
    let mut index = 0;
    while index < events.len() {
        if let Event::Start(Tag::Link {
            dest_url,
            title: _,
            id: _,
            ..
        }) = &events[index]
        {
            if let Some(tag) = media_tag(dest_url) {
                let mut label = String::new();
                let mut depth = 1usize;
                let mut cursor = index + 1;
                while cursor < events.len() && depth > 0 {
                    match &events[cursor] {
                        Event::Start(_) => depth += 1,
                        Event::End(TagEnd::Link) => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        Event::Text(text) | Event::Code(text) => label.push_str(text),
                        _ => {}
                    }
                    cursor += 1;
                }
                let url = escape_attr(dest_url);
                rewritten.push(Event::Html(
                    format!(
                        "<{tag} controls preload=\"metadata\" src=\"{url}\"></{tag}>"
                    )
                    .into(),
                ));
                let caption = label.trim();
                if !caption.is_empty() && caption != "🎤 语音" {
                    rewritten
                        .push(Event::Html(format!("<p class=\"media-caption\">{}</p>", escape_text(caption)).into()));
                }
                index = cursor + 1;
                continue;
            }
        }
        rewritten.push(events[index].clone());
        index += 1;
    }

    let mut out = String::new();
    html::push_html(&mut out, rewritten.into_iter());
    out
}

/// 媒体链接 → 播放器标签：音频 `audio`、视频 `video`；其余 None。
///
/// 仅认可本站 `/media/` 路径 + 扩展名白名单，放在用户内容里也安全。
#[cfg(feature = "ssr")]
fn media_tag(url: &str) -> Option<&'static str> {
    if !url.starts_with("/media/") {
        return None;
    }
    let lower = url.to_ascii_lowercase();
    if [".webm", ".mp3", ".wav", ".ogg", ".opus", ".m4a", ".aac"]
        .iter()
        .any(|ext| lower.ends_with(ext))
    {
        return Some("audio");
    }
    if [".mp4", ".webm", ".m4v", ".mov", ".ogv"]
        .iter()
        .any(|ext| lower.ends_with(ext))
    {
        return Some("video");
    }
    None
}

#[cfg(feature = "ssr")]
fn escape_attr(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(feature = "ssr")]
fn escape_text(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// 服务端的内容索引与载入逻辑。
#[cfg(feature = "ssr")]
pub mod store {
    use super::*;
    use anyhow::{Context, Result};
    use gray_matter::{engine::YAML, Matter};
    use std::path::Path;
    use std::sync::OnceLock;

    /// 一篇文章的完整内部表示。
    pub struct Post {
        pub summary: PostSummary,
        pub body: String,
        pub draft: bool,
    }

    /// 一个项目的完整内部表示。
    pub struct Project {
        pub summary: ProjectSummary,
        pub body: String,
    }

    /// 一个静态页。
    pub struct Page {
        pub title: String,
        pub updated: String,
        pub body: String,
    }

    /// 一条光遇内容。
    pub struct SkyEntry {
        pub slug: String,
        pub title: String,
        pub date: String,
        pub category: String,
        pub summary: String,
        pub cover: Option<String>,
        pub author: String,
        pub body: String,
    }

    /// 载入后的全站内容索引。
    pub struct ContentIndex {
        /// 已按日期倒序排列。
        pub posts: Vec<Post>,
        /// 已按 weight 升序、名称升序排列。
        pub projects: Vec<Project>,
        /// 静态页，键是 slug（文件名主干）。
        pub pages: std::collections::BTreeMap<String, Page>,
        /// 光遇内容，已按日期倒序。
        pub sky: Vec<SkyEntry>,
    }

    /// 文章 front matter。字段全部可选：写文章时不该被一堆必填项卡住。
    #[derive(Deserialize)]
    struct PostFrontMatter {
        title: Option<String>,
        date: Option<String>,
        summary: Option<String>,
        /// 发布者的站点账号用户名；不写就用 DEFAULT_AUTHOR。
        author: Option<String>,
        #[serde(default)]
        tags: Vec<String>,
        #[serde(default)]
        draft: bool,
    }

    /// 项目 front matter。
    #[derive(Deserialize)]
    struct ProjectFrontMatter {
        name: Option<String>,
        kind: Option<String>,
        summary: Option<String>,
        author: Option<String>,
        #[serde(default)]
        stack: Vec<String>,
        repo: Option<String>,
        demo: Option<String>,
        #[serde(default)]
        weight: i64,
    }

    /// 静态页 front matter。
    #[derive(Deserialize)]
    struct PageFrontMatter {
        title: Option<String>,
        updated: Option<String>,
    }

    /// 光遇内容 front matter。
    #[derive(Deserialize)]
    struct SkyFrontMatter {
        title: Option<String>,
        date: Option<String>,
        category: Option<String>,
        summary: Option<String>,
        cover: Option<String>,
        author: Option<String>,
    }

    static INDEX: OnceLock<ContentIndex> = OnceLock::new();

    /// 取得内容索引。由 `main.rs` 在启动时载入，因此这里绝不会为空。
    pub fn get() -> &'static ContentIndex {
        INDEX
            .get()
            .expect("内容索引尚未载入：应在 main 启动时调用 store::load()")
    }

    /// 扫描内容目录并建立索引。启动时调用一次；这里出错应当直接让进程起不来，
    /// 而不是等到第一个请求才发现某篇文章的 front matter 写坏了。
    pub fn load(root: impl AsRef<Path>) -> Result<()> {
        let root = root.as_ref();
        let mut posts = load_posts(&root.join("posts"))?;
        let projects = load_projects(&root.join("projects"))?;
        let pages = load_pages(&root.join("pages"))?;
        let mut sky = load_sky(&root.join("sky"))?;

        // 日期倒序；同日则按 slug 升序，保证顺序稳定可复现
        posts.sort_by(|a, b| {
            b.summary
                .date
                .cmp(&a.summary.date)
                .then_with(|| a.summary.slug.cmp(&b.summary.slug))
        });

        // 光遇内容同样按日期倒序，同日按 slug 升序
        sky.sort_by(|a, b| {
            b.date
                .cmp(&a.date)
                .then_with(|| a.slug.cmp(&b.slug))
        });

        INDEX
            .set(ContentIndex {
                posts,
                projects,
                pages,
                sky,
            })
            .map_err(|_| anyhow::anyhow!("内容索引被重复载入"))
    }

    fn load_posts(dir: &Path) -> Result<Vec<Post>> {
        let mut out = Vec::new();
        for (slug, raw) in read_markdown_dir(dir)? {
            let parsed = Matter::<YAML>::new()
                .parse::<PostFrontMatter>(&raw)
                .with_context(|| format!("解析文章 front matter 失败: {slug}"))?;
            let fm = parsed.data.unwrap_or(PostFrontMatter {
                title: None,
                date: None,
                summary: None,
                author: None,
                tags: Vec::new(),
                draft: false,
            });

            let body = parsed.content;
            let title = fm.title.unwrap_or_else(|| slug.clone());
            out.push(Post {
                summary: PostSummary {
                    slug,
                    title,
                    date: fm.date.unwrap_or_default(),
                    summary: fm.summary.unwrap_or_default(),
                    tags: fm.tags,
                    reading_minutes: reading_minutes(&body),
                    author: fm.author.unwrap_or_else(|| DEFAULT_AUTHOR.to_string()),
                },
                body,
                draft: fm.draft,
            });
        }
        Ok(out)
    }

    /// 载入静态页。键是文件名主干，所以 `content/pages/about.md` 对应 slug `about`。
    fn load_pages(
        dir: &Path,
    ) -> Result<std::collections::BTreeMap<String, Page>> {
        let mut out = std::collections::BTreeMap::new();
        for (slug, raw) in read_markdown_dir(dir)? {
            let parsed = Matter::<YAML>::new()
                .parse::<PageFrontMatter>(&raw)
                .with_context(|| format!("解析静态页 front matter 失败: {slug}"))?;
            let fm = parsed.data.unwrap_or(PageFrontMatter {
                title: None,
                updated: None,
            });

            out.insert(
                slug.clone(),
                Page {
                    title: fm.title.unwrap_or_else(|| slug.clone()),
                    updated: fm.updated.unwrap_or_default(),
                    body: parsed.content,
                },
            );
        }
        Ok(out)
    }

    /// 载入光遇内容。
    fn load_sky(dir: &Path) -> Result<Vec<SkyEntry>> {
        let mut out = Vec::new();
        for (slug, raw) in read_markdown_dir(dir)? {
            let parsed = Matter::<YAML>::new()
                .parse::<SkyFrontMatter>(&raw)
                .with_context(|| format!("解析光遇内容 front matter 失败: {slug}"))?;
            let fm = parsed.data.unwrap_or(SkyFrontMatter {
                title: None,
                date: None,
                category: None,
                summary: None,
                cover: None,
                author: None,
            });

            // 分类必须归一化到白名单里。只是「字段缺失时给默认值」不够——值写错
            // （比如拼错）时原样保留，结果两个分类页都匹配不上，内容就**静默消失**了。
            let raw_category = fm.category.unwrap_or_default();
            let category = if super::SKY_CATEGORIES.contains(&raw_category.as_str()) {
                raw_category
            } else {
                if !raw_category.is_empty() {
                    tracing::warn!(
                        "光遇内容 {slug} 的分类 `{raw_category}` 不在白名单里，按 gameplay 处理"
                    );
                }
                "gameplay".to_string()
            };

            out.push(SkyEntry {
                title: fm.title.unwrap_or_else(|| slug.clone()),
                date: fm.date.unwrap_or_default(),
                category,
                summary: fm.summary.unwrap_or_default(),
                cover: fm.cover,
                author: fm.author.unwrap_or_else(|| DEFAULT_AUTHOR.to_string()),
                slug,
                body: parsed.content,
            });
        }
        Ok(out)
    }

    fn load_projects(dir: &Path) -> Result<Vec<Project>> {
        let mut out = Vec::new();
        for (slug, raw) in read_markdown_dir(dir)? {
            let parsed = Matter::<YAML>::new()
                .parse::<ProjectFrontMatter>(&raw)
                .with_context(|| format!("解析项目 front matter 失败: {slug}"))?;
            let fm = parsed.data.unwrap_or(ProjectFrontMatter {
                name: None,
                kind: None,
                summary: None,
                author: None,
                stack: Vec::new(),
                repo: None,
                demo: None,
                weight: 0,
            });

            let body = parsed.content;
            out.push(Project {
                summary: ProjectSummary {
                    name: fm.name.unwrap_or_else(|| slug.clone()),
                    kind: fm.kind.unwrap_or_else(|| "open".to_string()),
                    summary: fm.summary.unwrap_or_default(),
                    stack: fm.stack,
                    repo: fm.repo,
                    demo: fm.demo,
                    weight: fm.weight,
                    author: fm.author.unwrap_or_else(|| DEFAULT_AUTHOR.to_string()),
                    slug,
                },
                body,
            });
        }
        // weight 小的排前面；同权重按名称，避免顺序随文件系统而变
        out.sort_by(|a, b| {
            a.summary
                .weight
                .cmp(&b.summary.weight)
                .then_with(|| a.summary.name.cmp(&b.summary.name))
        });
        Ok(out)
    }

    /// 读取目录下所有 `.md`，返回 (slug, 原始内容)。slug 取文件名主干。
    /// 目录不存在时返回空集合——新克隆的仓库不该因为缺目录就起不来。
    fn read_markdown_dir(dir: &Path) -> Result<Vec<(String, String)>> {
        let mut out = Vec::new();
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
            Err(e) => return Err(e).with_context(|| format!("读取内容目录失败: {}", dir.display())),
        };

        for entry in entries {
            let path = entry?.path();
            if path.extension().and_then(|s| s.to_str()) != Some("md") {
                continue;
            }
            let Some(slug) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let raw = std::fs::read_to_string(&path)
                .with_context(|| format!("读取内容文件失败: {}", path.display()))?;
            out.push((slug.to_string(), raw));
        }
        Ok(out)
    }

    /// 阅读时长按中文阅读速度粗估（约每分钟 400 字），仅供列表页展示。
    fn reading_minutes(body: &str) -> u32 {
        let chars = body.chars().filter(|c| !c.is_whitespace()).count();
        ((chars as f32 / 400.0).ceil() as u32).max(1)
    }
}

// ---------------------------------------------------------------------------
// server function：前端取内容的唯一入口
// ---------------------------------------------------------------------------

/// 列出全部已发布文章（摘要）。
#[server]
pub async fn list_posts() -> Result<Vec<PostSummary>, leptos::prelude::ServerFnError> {
    Ok(store::get()
        .posts
        .iter()
        .filter(|p| !p.draft)
        .map(|p| p.summary.clone())
        .collect())
}

/// 按 slug 取一篇文章；草稿对所有人不可见。
#[server]
pub async fn get_post(slug: String) -> Result<Option<PostDetail>, leptos::prelude::ServerFnError> {
    let found = store::get()
        .posts
        .iter()
        .find(|p| p.summary.slug == slug && !p.draft);

    let Some(post) = found else {
        return Ok(None);
    };

    let authors = resolve_authors(std::slice::from_ref(&post.summary.author)).await;

    Ok(Some(PostDetail {
        summary: post.summary.clone(),
        html: render_markdown(&post.body, true),
        author_account: authors.get(&post.summary.author.to_lowercase()).cloned(),
    }))
}

/// 按标签筛选文章。
#[server]
pub async fn list_posts_by_tag(
    tag: String,
) -> Result<Vec<PostSummary>, leptos::prelude::ServerFnError> {
    Ok(store::get()
        .posts
        .iter()
        .filter(|p| !p.draft && p.summary.tags.iter().any(|t| *t == tag))
        .map(|p| p.summary.clone())
        .collect())
}

/// 全站标签及各自文章数，按数量倒序。
#[server]
pub async fn list_tags() -> Result<Vec<(String, usize)>, leptos::prelude::ServerFnError> {
    let mut counts: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for post in store::get().posts.iter().filter(|p| !p.draft) {
        for tag in &post.summary.tags {
            *counts.entry(tag.clone()).or_default() += 1;
        }
    }
    let mut tags: Vec<(String, usize)> = counts.into_iter().collect();
    tags.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    Ok(tags)
}

/// 列出全部项目（摘要）。
#[server]
pub async fn list_projects() -> Result<Vec<ProjectSummary>, leptos::prelude::ServerFnError> {
    Ok(store::get()
        .projects
        .iter()
        .map(|p| p.summary.clone())
        .collect())
}

/// 按 slug 取一个项目。
#[server]
pub async fn get_project(
    slug: String,
) -> Result<Option<ProjectDetail>, leptos::prelude::ServerFnError> {
    let found = store::get()
        .projects
        .iter()
        .find(|p| p.summary.slug == slug);

    let Some(project) = found else {
        return Ok(None);
    };

    let authors = resolve_authors(std::slice::from_ref(&project.summary.author)).await;

    Ok(Some(ProjectDetail {
        summary: project.summary.clone(),
        html: render_markdown(&project.body, true),
        author_account: authors.get(&project.summary.author.to_lowercase()).cloned(),
    }))
}

/// 把发布者用户名解析成站内账号，返回 **小写用户名 → 账号**。
///
/// 用小写作键是因为用户名大小写不敏感（见 `migrations/0002`）：front matter 里写
/// `Felix` 还是 `felix` 都该落到同一个账号上。返回的 `username` 是**库里存的写法**，
/// 调用方拿它拼链接，以免同一账号出现两种大小写的网址。
///
/// 一次查询解决整页，避免列表页按条查（N+1）。查不到账号的名字不会出现在结果里，
/// 调用方据此决定「只显示用户名、不给链接」。
#[cfg(feature = "ssr")]
async fn resolve_authors(usernames: &[String]) -> std::collections::HashMap<String, AuthorAccount> {
    use crate::state::AppState;
    use sqlx::Row;

    // 去重按折叠后的形式做，否则 `Felix` 与 `felix` 会被当成两个名字各绑一次
    let mut unique: Vec<String> = usernames.iter().map(|name| name.to_lowercase()).collect();
    unique.sort_unstable();
    unique.dedup();

    let Some(app) = use_context::<AppState>() else {
        return Default::default();
    };
    if unique.is_empty() {
        return Default::default();
    }

    // SQLite 没有数组参数，按去重后的个数展开占位符——值仍然走绑定，不拼字符串。
    // COLLATE NOCASE 加在列上，才能用上 idx_users_username_nocase。
    let placeholders = vec!["?"; unique.len()].join(",");
    let sql = format!(
        "SELECT username, display_name FROM users \
         WHERE username COLLATE NOCASE IN ({placeholders})"
    );
    let mut query = sqlx::query(&sql);
    for name in &unique {
        query = query.bind(name);
    }

    match query.fetch_all(&app.pool).await {
        Ok(rows) => rows
            .into_iter()
            .map(|row| {
                let username: String = row.get("username");
                let display_name: String = row.get("display_name");
                (
                    username.to_lowercase(),
                    AuthorAccount {
                        username,
                        display_name,
                    },
                )
            })
            .collect(),
        Err(error) => {
            // 解析失败只影响「显示名还是用户名」，不该让整页打不开
            leptos::logging::warn!("解析发布者失败: {error}");
            Default::default()
        }
    }
}

/// 光遇内容的合法分类。
///
/// 与 `comments::TARGET_KINDS` 同理：这是从 URL 传来的用户输入，先白名单化再决定
/// 拿哪一份数据，绝不拿它去拼查询。
pub const SKY_CATEGORIES: [&str; 2] = ["gameplay", "gallery"];

/// 取一个静态页（关于、联系等）。
#[server]
pub async fn get_page(slug: String) -> Result<Option<PageDetail>, leptos::prelude::ServerFnError> {
    Ok(store::get().pages.get(&slug).map(|page| PageDetail {
        slug,
        title: page.title.clone(),
        updated: page.updated.clone(),
        html: render_markdown(&page.body, true),
    }))
}

/// 按分类列出光遇内容。分类不在白名单里就返回空列表。
#[server]
pub async fn list_sky(
    category: String,
) -> Result<Vec<SkyItem>, leptos::prelude::ServerFnError> {
    if !SKY_CATEGORIES.contains(&category.as_str()) {
        return Ok(Vec::new());
    }

    let picked: Vec<&store::SkyEntry> = store::get()
        .sky
        .iter()
        .filter(|entry| entry.category == category)
        .collect();

    let authors =
        resolve_authors(&picked.iter().map(|e| e.author.clone()).collect::<Vec<_>>()).await;

    Ok(picked
        .into_iter()
        .map(|entry| SkyItem {
            slug: entry.slug.clone(),
            title: entry.title.clone(),
            date: entry.date.clone(),
            category: entry.category.clone(),
            summary: entry.summary.clone(),
            cover: entry.cover.clone(),
            author: entry.author.clone(),
            author_account: authors.get(&entry.author.to_lowercase()).cloned(),
            html: render_markdown(&entry.body, true),
        })
        .collect())
}
