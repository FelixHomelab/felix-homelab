//! SEO：RSS、sitemap、robots.txt（仅服务端）。
//!
//! 三者都由 Axum 直接提供而不是做成 Leptos 页面：产出的是 XML 与纯文本，过一遍
//! 模板层没有收益，反而容易把 HTML 的转义规则带进来。
//!
//! **站点根地址从 `SITE_URL` 读**，默认是本机开发地址。RSS 与 sitemap 都要求绝对
//! URL，没有它就生成不出合法产出——部署时必须配置（见 `DESIGN.md` 第十节）。

use axum::http::{header, HeaderValue};
use axum::response::{IntoResponse, Response};

/// 站点根地址，末尾不带斜杠。
pub fn site_url() -> String {
    std::env::var("SITE_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:8080".to_string())
        .trim_end_matches('/')
        .to_string()
}

/// XML 文本转义。
///
/// 标题或摘要里出现 `&` 或 `<` 就会让整份 feed 变成非法 XML，所以这不是可选加固：
/// 内容虽然由站长自己写，但一个 `&` 就足以让订阅器报错。
fn escape_xml(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}

/// 把内容里的 `YYYY-MM-DD` 转成 RSS 要求的 RFC 822 形式。
///
/// 内容只精确到日，所以时间取当天 00:00，时区固定 +0800（站点面向中文读者）。
/// 解析不出来时退回 1970-01-01：一个明显的哨兵值，比悄悄填当前时间更容易发现。
fn rfc822(date: &str) -> String {
    use chrono::{FixedOffset, NaiveDate, TimeZone};

    let offset = FixedOffset::east_opt(8 * 3600).expect("+0800 是合法偏移");
    let parse = |text: &str| {
        NaiveDate::parse_from_str(text.trim(), "%Y-%m-%d")
            .ok()
            .and_then(|day| day.and_hms_opt(0, 0, 0))
            .and_then(|naive| offset.from_local_datetime(&naive).single())
    };

    let moment = parse(date).or_else(|| parse("1970-01-01"));
    // 上面两条都失败在实践中不可能发生（1970-01-01 一定可解析）
    moment
        .map(|dt| dt.to_rfc2822())
        .unwrap_or_else(|| "Thu, 01 Jan 1970 00:00:00 +0800".to_string())
}

/// 拼一个带正确 Content-Type 的响应。
fn xml_response(body: String, content_type: &'static str) -> Response {
    let mut response = body.into_response();
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    response
}

/// `GET /rss.xml`：博客文章的 RSS 2.0 feed。
pub async fn rss() -> Response {
    let base = site_url();
    let index = crate::content::store::get();

    let mut items = String::new();
    for post in index.posts.iter().filter(|post| !post.draft) {
        let url = format!("{base}/blog/{}", post.summary.slug);
        // 正文一并放进 content:encoded：只在 feed 里给摘要的话，阅读器还得跳回站点，
        // 「订阅」这件事就没什么意义了。
        let html = crate::content::render_markdown(&post.body, true);
        items.push_str(&format!(
            "<item>\
             <title>{title}</title>\
             <link>{url}</link>\
             <guid isPermaLink=\"true\">{url}</guid>\
             <pubDate>{date}</pubDate>\
             <dc:creator>{author}</dc:creator>\
             <description>{summary}</description>\
             <content:encoded>{content}</content:encoded>\
             </item>",
            title = escape_xml(&post.summary.title),
            url = escape_xml(&url),
            date = rfc822(&post.summary.date),
            // RSS 2.0 的 <author> 要求写成邮箱，所以用 Dublin Core 的 creator 带发布者名
            author = escape_xml(&post.summary.author),
            summary = escape_xml(&post.summary.summary),
            content = escape_xml(&html),
        ));
    }

    // 最新一篇的日期作为 lastBuildDate；没有文章时用当前月的第一天兜底
    let last_build = index
        .posts
        .iter()
        .filter(|post| !post.draft)
        .filter_map(|post| {
            chrono::NaiveDate::parse_from_str(post.summary.date.trim(), "%Y-%m-%d").ok()
        })
        .max();

    let body = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
         <rss version=\"2.0\" \
              xmlns:atom=\"http://www.w3.org/2005/Atom\" \
              xmlns:content=\"http://purl.org/rss/1.0/modules/content/\" \
              xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\
         <channel>\
         <title>Wraindrock</title>\
         <link>{base}/</link>\
         <description>Wraindrock 官方博客：官方内容与项目随仓库版本化。</description>\
         <language>zh-CN</language>\
         <atom:link href=\"{base}/rss.xml\" rel=\"self\" type=\"application/rss+xml\"/>\
         <lastBuildDate>{last_build}</lastBuildDate>\
         {items}\
         </channel>\
         </rss>",
        base = escape_xml(&base),
        last_build = last_build
            .map(|day| rfc822(&day.to_string()))
            .unwrap_or_else(|| rfc822("1970-01-01")),
        items = items,
    );

    xml_response(body, "application/rss+xml; charset=utf-8")
}

/// `GET /sitemap.xml`：所有公开页面的 sitemap。
///
/// 只列**对外可访问且值得收录**的地址。后台、个人设置、上传目录都由 `robots.txt`
/// 挡在外面，这里也不列。
pub async fn sitemap(
    axum::extract::State(state): axum::extract::State<crate::state::AppState>,
) -> Response {
    let base = site_url();
    let index = crate::content::store::get();

    let mut urls = String::new();
    let mut push = |path: &str, lastmod: Option<&str>| {
        let loc = if path == "/" {
            format!("{base}/")
        } else {
            format!("{base}{path}")
        };
        urls.push_str("<url><loc>");
        urls.push_str(&escape_xml(&loc));
        urls.push_str("</loc>");
        if let Some(day) = lastmod {
            if !day.trim().is_empty() {
                urls.push_str("<lastmod>");
                urls.push_str(&escape_xml(day.trim()));
                urls.push_str("</lastmod>");
            }
        }
        urls.push_str("</url>");
    };

    // 固定页面
    push("/", None);
    push("/blog", None);
    push("/projects", None);
    push("/sky", None);
    push("/sky/boosting", None);
    push("/about", None);
    push("/contact", None);
    // 光遇分类页只有真的有内容时才列，避免给搜索引擎一个空页面
    for category in crate::content::SKY_CATEGORIES {
        if index.sky.iter().any(|entry| entry.category == category) {
            push(&format!("/sky/{category}"), None);
        }
    }

    // 文章
    for post in index.posts.iter().filter(|post| !post.draft) {
        push(
            &format!("/blog/{}", post.summary.slug),
            Some(&post.summary.date),
        );
    }

    // 项目
    for project in index.projects.iter() {
        push(&format!("/projects/{}", project.summary.slug), None);
    }

    // 社区投稿：固定入口 + 已发布内容（查询失败不影响其余 sitemap）
    push("/community", None);
    push("/community/posts", None);
    push("/community/projects", None);
    push("/community/sky", None);
    if let Ok(rows) = sqlx::query_as::<_, (String, String, String)>(
        "SELECT u.username, c.slug, substr(c.created_at, 1, 10) \
         FROM community_posts c JOIN users u ON u.id = c.author_id \
         WHERE c.status = 'published' ORDER BY c.created_at DESC LIMIT 500",
    )
    .fetch_all(&state.pool)
    .await
    {
        for (username, slug, day) in rows {
            push(&format!("/community/{username}/{slug}"), Some(&day));
        }
    }

    // 标签页：只列真的存在文章的标签
    let mut tags: Vec<&str> = index
        .posts
        .iter()
        .filter(|post| !post.draft)
        .flat_map(|post| post.summary.tags.iter().map(String::as_str))
        .collect();
    tags.sort_unstable();
    tags.dedup();
    for tag in tags {
        push(&format!("/blog/tag/{tag}"), None);
    }

    let body = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
         <urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">{urls}</urlset>"
    );

    xml_response(body, "application/xml; charset=utf-8")
}

/// `GET /robots.txt`。
///
/// 后台、个人设置、接口与上传目录都不该被索引：前两者是私人页面，后两者对爬虫
/// 没有意义，抓了只是浪费带宽。
pub async fn robots() -> Response {
    let body = format!(
        "User-agent: *\n\
         Allow: /\n\
         \n\
         # 私人页面\n\
         Disallow: /admin\n\
         Disallow: /me\n\
         Disallow: /login\n\
         Disallow: /register\n\
         \n\
         # 接口与上传内容\n\
         Disallow: /api/\n\
         Disallow: /uploads/\n\
         \n\
         Sitemap: {}/sitemap.xml\n",
        site_url()
    );

    let mut response = body.into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    // robots 变了要尽快生效，别让爬虫拿着旧规则跑
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=3600"),
    );
    response
}
