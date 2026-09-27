//! 社区投稿页面：列表、详情、发布 / 编辑。
//!
//! 官方内容（Markdown，随仓库版本化）与社区内容（注册用户发布，存 SQLite）
//! 分区展示：官方入口在博客 / 项目 / 光遇，社区入口统一在 `/community`。

use leptos::prelude::*;
use leptos_meta::{Meta, Title};
use leptos_router::hooks::{use_location, use_navigate, use_params_map};

use super::{description_or_default, load_error, loading, route_param};
use crate::auth::UserState;
use crate::community::{
    delete_community, get_community, kind_label, list_community, submit_community, update_community,
    CommunityDetail,
};
use crate::components::comments::CommentSection;
use crate::components::community::{CommunityCard, ContentTabs};
use crate::components::PageHeader;

/// 从当前路径判断要展示的类型（`/community/posts` → 文章）。
fn kind_from_path(path: &str) -> Option<String> {
    let path = path.trim_end_matches('/');
    if path.ends_with("/posts") {
        Some("post".to_string())
    } else if path.ends_with("/projects") {
        Some("project".to_string())
    } else if path.ends_with("/sky") {
        Some("sky".to_string())
    } else {
        None
    }
}

/// 类型筛选条（全部 / 文章 / 项目 / 光遇）。
#[component]
fn KindTabs(active: Signal<Option<String>>) -> impl IntoView {
    let tab = move |label: &'static str, href: &'static str, value: Option<&'static str>| {
        let is_active = move || active.get().as_deref() == value;
        view! {
            <a class="segmented-btn" class:active=is_active href=href>
                {label}
            </a>
        }
    };

    view! {
        <nav class="segmented content-tabs" aria-label="社区内容类型">
            {tab("全部", "/community", None)}
            {tab("文章", "/community/posts", Some("post"))}
            {tab("项目", "/community/projects", Some("project"))}
            {tab("光遇", "/community/sky", Some("sky"))}
        </nav>
    }
}

/// 社区内容列表。
#[component]
pub fn CommunityIndex() -> impl IntoView {
    let user_state = use_context::<UserState>().expect("UserState 应由 App 提供");
    let path = use_location().pathname;
    let kind = Signal::derive(move || kind_from_path(&path.get()));
    let items = Resource::new_blocking(move || kind.get(), |kind| list_community(kind));

    view! {
        <Title text="社区 — Felix Homelab" />
        <Meta
            name="description"
            content="社区投稿：注册用户发布的文章、项目与光遇内容。"
        />
        <section class="wrap">
            <PageHeader
                title="社区"
                lede="注册用户发布的内容；发布即公开，违规会被下架。".to_string()
            />
            <ContentTabs active="community" official_href="/blog" community_href="/community" />
            <Suspense fallback=loading>
                {move || {
                    let logged_in = matches!(user_state.get(), Some(Ok(Some(_))));
                    let new_href = if logged_in { "/community/new" } else { "/login" };
                    view! {
                        <div class="community-bar">
                            <KindTabs active=kind />
                            <a class="btn btn-primary btn-small" href=new_href>
                                {if logged_in { "发布新内容" } else { "登录后发布" }}
                            </a>
                        </div>
                    }
                }}
            </Suspense>

            <Suspense fallback=loading>
                {move || items.get().map(|res| match res {
                    Ok(list) if list.is_empty() => view! {
                        <p class="muted">"这个分区还没有内容，来发第一条吧。"</p>
                    }.into_any(),
                    Ok(list) => view! {
                        <div class="card-list">
                            {list.into_iter().map(|item| view! { <CommunityCard item=item /> }).collect_view()}
                        </div>
                    }.into_any(),
                    Err(e) => load_error(e.to_string()).into_any(),
                })}
            </Suspense>
        </section>
    }
}

/// 社区内容详情。
#[component]
pub fn CommunityDetailPage() -> impl IntoView {
    #[cfg(feature = "ssr")]
    let response_options = use_context::<leptos_axum::ResponseOptions>();

    let username = route_param("username");
    let slug = route_param("slug");
    let item = Resource::new_blocking(
        move || (username(), slug()),
        |(username, slug)| get_community(username, slug),
    );

    view! {
        <section class="wrap">
            <Suspense fallback=loading>
                {move || item.get().map(|res| match res {
                    Ok(Some(detail)) => community_detail_view(detail).into_any(),
                    Ok(None) => {
                        #[cfg(feature = "ssr")]
                        if let Some(options) = &response_options {
                            options.set_status(axum::http::StatusCode::NOT_FOUND);
                        }
                        view! {
                            <Title text="找不到内容 — Felix Homelab" />
                            <PageHeader
                                title="找不到这条内容"
                                lede="它可能被删除、被下架，或者链接不对。".to_string()
                            />
                            <p><a href="/community">"← 回社区"</a></p>
                        }
                        .into_any()
                    }
                    Err(e) => load_error(e.to_string()).into_any(),
                })}
            </Suspense>
        </section>
    }
}

/// 详情正文。单独拆一个函数，避免 Suspense 闭包里堆太多逻辑。
fn community_detail_view(detail: CommunityDetail) -> impl IntoView {
    let kind = detail.summary.kind.clone();
    let kind_text = kind_label(&kind);
    let title = detail.summary.title.clone();
    let author = detail.summary.author.clone();
    let author_username = detail.summary.author_username.clone();
    let author_href = format!("/user/{author_username}");
    let date: String = detail.summary.created_at.chars().take(10).collect();
    let hidden = detail.status != "published";
    let can_edit = detail.can_edit;
    let id = detail.summary.id;
    let repo = detail.meta.repo.clone();
    let demo = detail.meta.demo.clone();
    let cover = detail.meta.cover.clone();
    let project_kind = kind_label_project(&detail.meta.project_kind).to_string();
    let comment_slug = format!("{author_username}/{}", detail.summary.slug);
    let edit_href = format!("/community/{author_username}/{}/edit", detail.summary.slug);
    let message = RwSignal::new(String::new());
    let navigate = use_navigate();

    let on_delete = {
        let navigate = navigate.clone();
        move |_| {
            #[cfg(not(feature = "ssr"))]
            {
                let confirmed = web_sys::window()
                    .and_then(|window| {
                        window
                            .confirm_with_message("确定删除这条内容？此操作不可撤销。")
                            .ok()
                    })
                    .unwrap_or(false);
                if !confirmed {
                    return;
                }
            }
            let navigate = navigate.clone();
            leptos::task::spawn_local(async move {
                match delete_community(id).await {
                    Ok(Ok(())) => navigate("/community", Default::default()),
                    Ok(Err(text)) => message.set(text),
                    Err(error) => message.set(format!("请求失败：{error}")),
                }
            });
        }
    };

    view! {
        <article class="article">
            <Title text=format!("{title} — Felix Homelab") />
            <Meta name="description" content=description_or_default(&detail.summary.summary) />
            <header class="page-header">
                <h1>{title}</h1>
                <p class="card-meta">
                    <span class="badge">{kind_text}</span>
                    " · "
                    <a href=author_href.clone()>{author}</a>
                    " · "{date}
                    {hidden.then(|| view! {
                        " · "<span class="badge badge-hidden">"已下架（仅自己可见）"</span>
                    })}
                </p>
                <ul class="tag-row">
                    {detail.summary.tags.clone().into_iter().map(|tag| {
                        view! { <li><span class="tag tag-plain">{tag}</span></li> }
                    }).collect_view()}
                </ul>
                {(kind == "project").then(|| view! { <span class="badge">{project_kind}</span> })}
            </header>

            {(!repo.is_empty() || !demo.is_empty()).then(|| view! {
                <div class="hero-actions">
                    {(!repo.is_empty()).then(|| view! {
                        <a class="btn" href=repo.clone() target="_blank" rel="noreferrer">"代码仓库"</a>
                    })}
                    {(!demo.is_empty()).then(|| view! {
                        <a class="btn" href=demo.clone() target="_blank" rel="noreferrer">"在线访问"</a>
                    })}
                </div>
            })}

            {(!cover.is_empty()).then(|| view! {
                <img class="community-cover" src=cover.clone() alt="封面" />
            })}

            <div class="prose" inner_html=detail.body_html.clone()></div>

            {move || {
                let text = message.get();
                (!text.is_empty()).then(|| view! {
                    <p class="error" role="alert">{text}</p>
                })
            }}
            {can_edit.then(|| view! {
                <div class="admin-actions">
                    <a class="btn btn-small" href=edit_href.clone()>"编辑"</a>
                    <button class="btn btn-small btn-danger" on:click=on_delete.clone()>"删除"</button>
                </div>
            })}
        </article>

        // 项目没有评论区（与官方项目页一致）；文章与光遇共用一套评论
        {(kind != "project").then(|| view! {
            <CommentSection target_kind="community" target_slug=comment_slug />
        })}
    }
}

/// 项目类型的标签（社区项目复用官方的三分类）。
fn kind_label_project(kind: &str) -> &str {
    match kind {
        "open" => "开源",
        "private" => "私有",
        "team" => "团队",
        other => other,
    }
}

/// 发布 / 编辑社区内容。
#[component]
pub fn CommunitySubmitPage() -> impl IntoView {
    let user_state = use_context::<UserState>().expect("UserState 应由 App 提供");
    let params = use_params_map();
    let username = route_param("username");
    let slug = route_param("slug");
    let is_edit = move || !params.with(|map| map.get("slug").unwrap_or_default()).is_empty();
    let page_title = if is_edit() { "编辑内容" } else { "发布内容" }.to_string();

    let kind = RwSignal::new("post".to_string());
    let title = RwSignal::new(String::new());
    let slug_input = RwSignal::new(String::new());
    let summary = RwSignal::new(String::new());
    let tags = RwSignal::new(String::new());
    let body = RwSignal::new(String::new());
    let project_kind = RwSignal::new("open".to_string());
    let repo = RwSignal::new(String::new());
    let demo = RwSignal::new(String::new());
    let category = RwSignal::new("gameplay".to_string());
    let cover = RwSignal::new(String::new());
    let post_id = RwSignal::new(None::<i64>);
    let message = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let prefilled = RwSignal::new(false);
    let navigate = use_navigate();

    // 编辑模式：按路由参数拉取原文回填（水合后在 Effect 里做，SSR 先渲染空表单）
    let prefill = Resource::new_blocking(
        move || (username(), slug(), is_edit()),
        |(username, slug, edit)| async move {
            if !edit {
                return None;
            }
            get_community(username, slug).await.ok().flatten()
        },
    );

    Effect::new(move |_| {
        if prefilled.get() {
            return;
        }
        if let Some(Some(detail)) = prefill.get() {
            post_id.set(Some(detail.summary.id));
            kind.set(detail.summary.kind.clone());
            title.set(detail.summary.title.clone());
            slug_input.set(detail.summary.slug.clone());
            summary.set(detail.summary.summary.clone());
            tags.set(detail.summary.tags.join(", "));
            project_kind.set(if detail.meta.project_kind.is_empty() {
                "open".to_string()
            } else {
                detail.meta.project_kind.clone()
            });
            repo.set(detail.meta.repo.clone());
            demo.set(detail.meta.demo.clone());
            category.set(if detail.meta.category.is_empty() {
                "gameplay".to_string()
            } else {
                detail.meta.category.clone()
            });
            cover.set(detail.meta.cover.clone());
            body.set(detail.body_md.clone());
            prefilled.set(true);
        }
    });

    view! {
        <Title text="发布内容 — Felix Homelab" />
        <section class="wrap">
            <PageHeader
                title=page_title
                lede="支持 Markdown；发布即公开，作者本人与管理员可以编辑或删除。".to_string()
            />
            <Suspense fallback=loading>
                {move || {
                    let logged_in = matches!(user_state.get(), Some(Ok(Some(_))));
                    if !logged_in {
                        return view! {
                            <p class="muted">
                                "发布内容需要先登录。" <a href="/login">"去登录"</a>
                            </p>
                        }
                        .into_any();
                    }
                    if matches!(prefill.get(), Some(Some(detail)) if !detail.can_edit) {
                        return view! {
                            <p class="error" role="alert">"只能编辑自己的内容。"</p>
                        }
                        .into_any();
                    }

                    let navigate = navigate.clone();
                    let on_submit = move |ev: leptos::ev::SubmitEvent| {
                        ev.prevent_default();

                        let kind_value = kind.get_untracked();
                        let title_value = title.get_untracked();
                        let summary_value = summary.get_untracked();
                        let body_value = body.get_untracked();
                        let slug_value = slug_input.get_untracked();
                        let tags_value = tags.get_untracked();
                        let project_kind_value = project_kind.get_untracked();
                        let repo_value = repo.get_untracked();
                        let demo_value = demo.get_untracked();
                        let category_value = category.get_untracked();
                        let cover_value = cover.get_untracked();

                        if title_value.trim().is_empty() || body_value.trim().is_empty() {
                            message.set("标题和正文不能为空。".to_string());
                            return;
                        }

                        message.set(String::new());
                        busy.set(true);
                        let navigate = navigate.clone();
                        let editing = post_id.get_untracked();
                        leptos::task::spawn_local(async move {
                            let result = match editing {
                                Some(id) => {
                                    update_community(
                                        id,
                                        title_value,
                                        summary_value,
                                        body_value,
                                        slug_value,
                                        tags_value,
                                        project_kind_value,
                                        repo_value,
                                        demo_value,
                                        category_value,
                                        cover_value,
                                    )
                                    .await
                                }
                                None => {
                                    submit_community(
                                        kind_value,
                                        title_value,
                                        summary_value,
                                        body_value,
                                        slug_value,
                                        tags_value,
                                        project_kind_value,
                                        repo_value,
                                        demo_value,
                                        category_value,
                                        cover_value,
                                    )
                                    .await
                                }
                            };

                            match result {
                                Ok(Ok(url)) => navigate(&url, Default::default()),
                                Ok(Err(text)) => {
                                    message.set(text);
                                    busy.set(false);
                                }
                                Err(error) => {
                                    message.set(format!("请求失败：{error}"));
                                    busy.set(false);
                                }
                            }
                        });
                    };

                    view! {
                        <form class="auth-form community-form" on:submit=on_submit>
                            <label class="field">
                                <span>"类型"</span>
                                <select on:change=move |ev| kind.set(event_target_value(&ev))>
                                    <option value="post" selected=move || kind.get() == "post">"文章"</option>
                                    <option value="project" selected=move || kind.get() == "project">"项目"</option>
                                    <option value="sky" selected=move || kind.get() == "sky">"光遇"</option>
                                </select>
                                <small class="muted">"编辑时类型不可更改。"</small>
                            </label>
                            <label class="field">
                                <span>"标题"</span>
                                <input
                                    type="text"
                                    prop:value=move || title.get()
                                    on:input=move |ev| title.set(event_target_value(&ev))
                                />
                            </label>
                            <label class="field">
                                <span>"链接名（可留空，由标题自动生成）"</span>
                                <input
                                    type="text"
                                    prop:value=move || slug_input.get()
                                    on:input=move |ev| slug_input.set(event_target_value(&ev))
                                />
                                <small class="muted">"详情页地址：/community/用户名/链接名"</small>
                            </label>
                            <label class="field">
                                <span>"摘要（可留空）"</span>
                                <input
                                    type="text"
                                    prop:value=move || summary.get()
                                    on:input=move |ev| summary.set(event_target_value(&ev))
                                />
                            </label>
                            <label class="field">
                                <span>"标签（可留空，逗号分隔，最多 10 个）"</span>
                                <input
                                    type="text"
                                    prop:value=move || tags.get()
                                    on:input=move |ev| tags.set(event_target_value(&ev))
                                />
                            </label>

                            {move || (kind.get() == "project").then(|| view! {
                                <label class="field">
                                    <span>"项目分类"</span>
                                    <select on:change=move |ev| project_kind.set(event_target_value(&ev))>
                                        <option value="open" selected=move || project_kind.get() == "open">"开源"</option>
                                        <option value="private" selected=move || project_kind.get() == "private">"私有"</option>
                                        <option value="team" selected=move || project_kind.get() == "team">"团队"</option>
                                    </select>
                                </label>
                            })}
                            {move || (kind.get() == "project").then(|| view! {
                                <label class="field">
                                    <span>"仓库链接（可留空）"</span>
                                    <input
                                        type="text"
                                        prop:value=move || repo.get()
                                        on:input=move |ev| repo.set(event_target_value(&ev))
                                    />
                                </label>
                            })}
                            {move || (kind.get() == "project").then(|| view! {
                                <label class="field">
                                    <span>"演示链接（可留空）"</span>
                                    <input
                                        type="text"
                                        prop:value=move || demo.get()
                                        on:input=move |ev| demo.set(event_target_value(&ev))
                                    />
                                </label>
                            })}

                            {move || (kind.get() == "sky").then(|| view! {
                                <label class="field">
                                    <span>"光遇分类"</span>
                                    <select on:change=move |ev| category.set(event_target_value(&ev))>
                                        <option value="gameplay" selected=move || category.get() == "gameplay">"攻略"</option>
                                        <option value="gallery" selected=move || category.get() == "gallery">"画廊"</option>
                                    </select>
                                </label>
                            })}
                            {move || (kind.get() == "sky").then(|| view! {
                                <label class="field">
                                    <span>"封面图地址（可留空）"</span>
                                    <input
                                        type="text"
                                        prop:value=move || cover.get()
                                        on:input=move |ev| cover.set(event_target_value(&ev))
                                    />
                                </label>
                            })}

                            <label class="field">
                                <span>"正文（Markdown）"</span>
                                <textarea
                                    class="comment-input"
                                    rows="14"
                                    prop:value=move || body.get()
                                    on:input=move |ev| body.set(event_target_value(&ev))
                                ></textarea>
                            </label>

                            {move || {
                                let text = message.get();
                                (!text.is_empty()).then(|| view! {
                                    <p class="error" role="alert">{text}</p>
                                })
                            }}
                            <button class="btn btn-primary" type="submit" disabled=move || busy.get()>
                                {move || if busy.get() { "提交中…" } else { "发布" }}
                            </button>
                        </form>
                    }
                    .into_any()
                }}
            </Suspense>
        </section>
    }
}
