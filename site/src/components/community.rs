//! 社区投稿的界面组件。

use leptos::prelude::*;

use crate::community::{kind_label, CommunitySummary};

/// 「官方内容 / 社区投稿」分区切换条。
///
/// 官方内容随仓库版本化（Markdown），社区投稿由注册用户发布；两类内容分区展示，
/// 互不混淆。
#[component]
pub fn ContentTabs(
    /// 当前所在分区：`official` 或 `community`。
    active: &'static str,
    official_href: &'static str,
    community_href: &'static str,
) -> impl IntoView {
    view! {
        <nav class="segmented content-tabs" aria-label="内容分区">
            <a
                class="segmented-btn"
                class:active=active == "official"
                href=official_href
            >
                "官方内容"
            </a>
            <a
                class="segmented-btn"
                class:active=active == "community"
                href=community_href
            >
                "社区投稿"
            </a>
        </nav>
    }
}

/// 社区内容卡片。
#[component]
pub fn CommunityCard(item: CommunitySummary) -> impl IntoView {
    let href = format!("/community/{}/{}", item.author_username, item.slug);
    let kind = kind_label(&item.kind);
    let author_href = format!("/user/{}", item.author_username);
    // 展示到日即可，时间戳对列表没有意义
    let date: String = item.created_at.chars().take(10).collect();
    let summary = item.summary.clone();
    let author = item.author.clone();

    view! {
        <article class="card">
            <div class="card-head">
                <a class="card-title" href=href>{item.title.clone()}</a>
                <span class="badge">{kind}</span>
            </div>
            <p class="card-meta">
                <a href=author_href>{author}</a>
                " · "{date}
            </p>
            <p class="card-summary">{summary}</p>
            <ul class="tag-row">
                {item.tags.into_iter().map(|tag| {
                    view! { <li><span class="tag tag-plain">{tag}</span></li> }
                }).collect_view()}
            </ul>
        </article>
    }
}
