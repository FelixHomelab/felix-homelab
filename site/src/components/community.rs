//! 社区投稿的界面组件。

use leptos::prelude::*;

use crate::community::{kind_label, CommunitySummary};

/// 「官方内容 / 社区投稿」分区切换条。
///
/// 社区内容卡片。
#[component]
pub fn CommunityCard(item: CommunitySummary) -> impl IntoView {
    let href = if item.kind == "sky" {
        format!("/sky/community/{}/{}", item.author_username, item.slug)
    } else {
        format!("/community/{}/{}", item.author_username, item.slug)
    };
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
                {item.featured.then(|| view! { <span class="badge badge-featured">"精选"</span> })}
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
