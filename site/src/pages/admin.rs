//! 后台页面。
//!
//! 页面级的身份判断只决定**显示什么**，授权由 `crate::admin` 里每个 server function
//! 自己完成。这里做检查是为了给非管理员一个像样的 403，而不是一屏报错。

use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::hooks::use_location;

use crate::admin::{
    admin_backup_config, admin_backup_list, admin_backup_now, admin_backup_save_config,
    admin_backup_trigger_sync, admin_delete_comment, admin_delete_community, admin_delete_review,
    admin_list_comments, admin_list_community, admin_list_pod, admin_list_reviews,
    admin_list_users, admin_load_overview, admin_reply_review, admin_restart_container,
    admin_set_comment_status, admin_set_community_status, admin_set_review_status,
    admin_set_user_role, admin_set_user_scope, admin_set_user_status, AdminComment,
    AdminCommunityPost, AdminReview, AdminUser, BackupChannel, PodContainer,
};
use crate::agents::{
    admin_agent_action, admin_grant_agent, admin_list_agents, admin_purge_agent,
    admin_renew_agent, agent_kind_label, AgentRow,
};
use crate::roles::{admin_permissions, role_label, SCOPED_ROLES};
use crate::community::kind_label as community_kind_label;
use crate::community::{
    admin_list_tags, admin_merge_tag, admin_save_tag, CommunityTag,
};
use crate::sky::{
    admin_delete_sky_official, admin_feature_sky_post, admin_list_sky_official,
    admin_list_sky_posts, admin_pin_review, admin_save_sky_boosting, admin_save_sky_official,
    sky_boosting,
};
use crate::orders::{
    admin_cancel_order, admin_confirm_order, admin_orders, format_cents, order_status_label,
    product_label,
};
use crate::components::PageHeader;

use super::set_status;

/// 统一执行一个后台动作：成功就刷新列表并提示，失败把服务端的原话显示出来。
fn run_action<F, T>(revision: RwSignal<u32>, message: RwSignal<String>, action: F)
where
    F: std::future::Future<Output = Result<Result<T, String>, leptos::prelude::ServerFnError>>
        + 'static,
    T: 'static,
{
    leptos::task::spawn_local(async move {
        match action.await {
            Ok(Ok(_)) => {
                message.set("已处理。".to_string());
                // 自增触发列表重新拉取
                revision.update(|n| *n += 1);
            }
            Ok(Err(text)) => message.set(text),
            Err(error) => message.set(format!("请求失败：{error}")),
        }
    });
}

/// 非管理员看到的页面。
fn forbidden() -> impl IntoView {
    set_status(403);
    view! {
        <Title text="无权访问 — Wraindrock" />
        <section class="wrap">
            <PageHeader title="无权访问" lede="这个页面只有管理员能看。".to_string() />
            <p><a href="/">"← 回首页"</a></p>
        </section>
    }
}

/// 后台各页共用的导航（当前页高亮；按权限过滤）。
#[component]
fn AdminNav(perms: Vec<String>) -> impl IntoView {
    // 归一化尾斜杠：/admin/ 与 /admin 视为同一页
    let path = use_location().pathname;
    let active = move |href: &str| path.get().trim_end_matches('/') == href;
    let perms = StoredValue::new(perms);
    let has = move |need: &str| {
        perms.with_value(|list| {
            list.iter().any(|p| p == "super")
                || (need == "staff" && !list.is_empty())
                || list.iter().any(|p| p == need)
        })
    };

    view! {
        <nav class="admin-nav">
            {has("staff")
                .then(|| view! { <a href="/admin" class:active=move || active("/admin")>"概览"</a> })}
            {has("community")
                .then(|| {
                    view! {
                        <a href="/admin/comments" class:active=move || active("/admin/comments")>
                            "评论"
                        </a>
                    }
                })}
            {has("community")
                .then(|| {
                    view! {
                        <a
                            href="/admin/community"
                            class:active=move || active("/admin/community")
                        >
                            "社区"
                        </a>
                    }
                })}
            {has("sky")
                .then(|| {
                    view! {
                        <a
                            href="/admin/sky-reviews"
                            class:active=move || active("/admin/sky-reviews")
                        >
                            "评价"
                        </a>
                    }
                })}
            {has("sky")
                .then(|| {
                    view! { <a href="/admin/sky" class:active=move || active("/admin/sky")>"光遇"</a> }
                })}
            {has("super")
                .then(|| {
                    view! { <a href="/admin/users" class:active=move || active("/admin/users")>"用户"</a> }
                })}
            {has("super")
                .then(|| {
                    view! { <a href="/admin/pod" class:active=move || active("/admin/pod")>"Pod"</a> }
                })}
            {has("agent")
                .then(|| {
                    view! {
                        <a href="/admin/agents" class:active=move || active("/admin/agents")>"Agent"</a>
                    }
                })}
            {has("super")
                .then(|| {
                    view! { <a href="/admin/backup" class:active=move || active("/admin/backup")>"备份"</a> }
                })}
        </nav>
    }
}

/// 后台页面的外壳：先确认是管理员，再把内容渲染出来。
///
/// **守卫必须写在响应式闭包（`move || ...`）里，不能写成组件体里的 `if ... return`。**
/// 组件体在资源解析之前就执行了，那时读「当前用户」只会拿到 `None`——结果是管理员
/// 也被自己的后台挡在门外。
///
/// 这里也**不复用 App 级的那个「当前用户」资源**：它在路由子树里读会时有时无
/// （连打 5 次能出现 403），改成页面内部自己的资源后稳定。
#[component]
fn AdminPage(
    #[prop(into)] title: String,
    #[prop(into)] lede: String,
    /// 需要的权限 scope：staff（任意管理）/ community / sky / agent / super
    #[prop(into)]
    perm: &'static str,
    children: ChildrenFn,
) -> impl IntoView {
    let perms = Resource::new_blocking(|| (), |_| admin_permissions());

    view! {
        <Suspense fallback=|| view! { <p class="muted">"载入中…"</p> }>
            {move || match perms.get() {
                Some(Ok(list)) => {
                    let allowed = list.iter().any(|p| p == "super")
                        || (perm == "staff" && !list.is_empty())
                        || list.iter().any(|p| p == perm);
                    if !allowed {
                        return forbidden().into_any();
                    }
                    let children = children.clone();
                    view! {
                        <section class="wrap admin">
                            <PageHeader title=title.clone() lede=lede.clone() />
                            <AdminNav perms=list />
                            {children()}
                        </section>
                    }
                    .into_any()
                }
                Some(_) => forbidden().into_any(),
                None => view! { <p class="muted">"载入中…"</p> }.into_any(),
            }}
        </Suspense>
    }
}

/// 待审 / 全部 的筛选开关。后台最常做的动作是清待审队列，所以默认只看待审。
#[component]
fn StatusFilter(only_pending: RwSignal<bool>) -> impl IntoView {
    view! {
        <div class="segmented">
            <button
                class="segmented-btn"
                class:active=move || only_pending.get()
                on:click=move |_| only_pending.set(true)
            >
                "只看待审"
            </button>
            <button
                class="segmented-btn"
                class:active=move || !only_pending.get()
                on:click=move |_| only_pending.set(false)
            >
                "全部"
            </button>
        </div>
    }
}

/// 后台概览。
#[component]
pub fn AdminDashboardPage() -> impl IntoView {
    let overview = Resource::new_blocking(|| (), |_| admin_load_overview());
    let pod = Resource::new_blocking(|| (), |_| admin_list_pod());

    view! {
        <Title text="后台 — Wraindrock" />
        <AdminPage title="后台" lede="审核评论、管理用户、回复评价、看护容器。".to_string() perm="staff">
            <Suspense fallback=|| view! { <p class="muted">"载入中…"</p> }>
                {move || match overview.get() {
                    None => view! { <p class="muted">"载入中…"</p> }.into_any(),
                    Some(Err(error)) => view! {
                        <p class="error">"载入统计失败："{error.to_string()}</p>
                    }
                    .into_any(),
                    Some(Ok(data)) => view! {
                        <div class="stat-grid">
                            <a class="stat" href="/admin/comments">
                                <span class="stat-value">{data.pending_comments}</span>
                                <span class="stat-label">"待审评论"</span>
                            </a>
                            <a class="stat" href="/admin/sky-reviews">
                                <span class="stat-value">{data.pending_reviews}</span>
                                <span class="stat-label">"待审评价"</span>
                            </a>
                            <a class="stat" href="/admin/users">
                                <span class="stat-value">{data.users}</span>
                                <span class="stat-label">"注册用户"</span>
                            </a>
                            <a class="stat" href="/admin/community">
                                <span class="stat-value">{data.community}</span>
                                <span class="stat-label">"社区投稿"</span>
                            </a>
                            <a class="stat" href="/admin/users">
                                <span class="stat-value">{data.banned}</span>
                                <span class="stat-label">"已封禁"</span>
                            </a>
                            <Suspense fallback=|| ()>
                                {move || match pod.get() {
                                    Some(Ok(list)) => {
                                        let total = list.len();
                                        let running = list
                                            .iter()
                                            .filter(|c| c.state == "running")
                                            .count();
                                        let healthy = list
                                            .iter()
                                            .filter(|c| c.health.as_deref() == Some("healthy"))
                                            .count();
                                        view! {
                                            <a class="stat" href="/admin/pod">
                                                <span class="stat-value">
                                                    {format!("{running}/{total}")}
                                                </span>
                                                <span class="stat-label">"容器运行中"</span>
                                            </a>
                                            <a class="stat" href="/admin/pod">
                                                <span class="stat-value">{healthy.to_string()}</span>
                                                <span class="stat-label">"健康检查通过"</span>
                                            </a>
                                        }
                                            .into_any()
                                    }
                                    _ => ().into_any(),
                                }}
                            </Suspense>
                        </div>
                    }
                    .into_any(),
                }}
            </Suspense>

            <section class="admin-quick">
                <h2 class="admin-section-title">"快捷入口"</h2>
                <div class="quick-grid">
                    <a class="quick-tile" href="/admin/comments">
                        <strong>"评论审核"</strong>
                        <span>"待审队列，通过 / 拒绝 / 删除"</span>
                    </a>
                    <a class="quick-tile" href="/admin/community">
                        <strong>"社区管理"</strong>
                        <span>"投稿下架、恢复与删除"</span>
                    </a>
                    <a class="quick-tile" href="/admin/users">
                        <strong>"用户管理"</strong>
                        <span>"封禁、超级管理员与协作角色"</span>
                    </a>
                    <a class="quick-tile" href="/admin/pod">
                        <strong>"Pod 管理"</strong>
                        <span>"容器运行状态与重启"</span>
                    </a>
                    <a class="quick-tile" href="/admin/agents">
                        <strong>"Agent 管理"</strong>
                        <span>"开通、续费与实例状态"</span>
                    </a>
                    <a class="quick-tile" href="/admin/orders">
                        <strong>"服务订单"</strong>
                        <span>"时间池/容量池/外置存储，人工确认收款"</span>
                    </a>
                    <a class="quick-tile" href="/admin/backup">
                        <strong>"备份与同步"</strong>
                        <span>"备份源、异地渠道与归档"</span>
                    </a>
                    <a
                        class="quick-tile"
                        href="https://opencloud.wraindrock.com/"
                        target="_blank"
                        rel="noreferrer"
                    >
                        <strong>"OpenCloud"</strong>
                        <span>"文件同步 / 分享（opencloud.wraindrock.com）"</span>
                    </a>
                </div>
            </section>
        </AdminPage>
    }
}

/// 评论审核。
#[component]
pub fn AdminCommentsPage() -> impl IntoView {
    let revision = RwSignal::new(0u32);
    let message = RwSignal::new(String::new());
    let only_pending = RwSignal::new(true);

    let comments = Resource::new_blocking(
        move || (revision.get(), only_pending.get()),
        |(_, pending)| admin_list_comments(if pending { "pending".into() } else { "all".into() }),
    );

    view! {
        <Title text="评论审核 — Wraindrock" />
        <AdminPage title="评论审核" lede="通过后才会显示在页面上。".to_string() perm="community">
            <StatusFilter only_pending=only_pending />
            <p class="notice" role="status">{move || message.get()}</p>

            <Suspense fallback=|| view! { <p class="muted">"载入中…"</p> }>
                {move || match comments.get() {
                    None => view! { <p class="muted">"载入中…"</p> }.into_any(),
                    Some(Err(error)) => view! {
                        <p class="error">"载入评论失败："{error.to_string()}</p>
                    }
                    .into_any(),
                    Some(Ok(list)) if list.is_empty() => {
                        view! { <p class="admin-empty">"没有需要处理的评论。"</p> }.into_any()
                    }
                    Some(Ok(list)) => view! {
                        <div class="admin-list">
                            {list
                                .into_iter()
                                .map(|comment| {
                                    view! {
                                        <AdminCommentRow
                                            comment=comment
                                            revision=revision
                                            message=message
                                        />
                                    }
                                })
                                .collect_view()}
                        </div>
                    }
                    .into_any(),
                }}
            </Suspense>
        </AdminPage>
    }
}

/// 后台里的一行评论。
#[component]
fn AdminCommentRow(
    comment: AdminComment,
    revision: RwSignal<u32>,
    message: RwSignal<String>,
) -> impl IntoView {
    let id = comment.id;
    let target = format!("/{}/{}", comment.target_kind, comment.target_slug);
    let reply_count = comment.reply_count;
    // 删除会连带删掉回复（外键 CASCADE），动手前先把后果说清楚
    let delete_hint = if reply_count > 0 {
        format!("删除（连同 {reply_count} 条回复）")
    } else {
        "删除".to_string()
    };

    view! {
        <article class="admin-row">
            <p class="admin-meta">
                <span class=format!("status status-{}", comment.status)>{comment.status.clone()}</span>
                <strong>{comment.author.clone()}</strong>
                <span class="comment-time">{comment.created_at.clone()}</span>
                <a href=target>{format!("{}/{}", comment.target_kind, comment.target_slug)}</a>
                {comment.parent_id.map(|p| view! { <span class="comment-time">"回复 #"{p}</span> })}
            </p>
            <p class="admin-body">{comment.body_md.clone()}</p>
            <div class="admin-actions">
                <button
                    class="btn btn-small"
                    on:click=move |_| {
                        run_action(revision, message, admin_set_comment_status(id, "approved".into()))
                    }
                >
                    "通过"
                </button>
                <button
                    class="btn btn-small"
                    on:click=move |_| {
                        run_action(revision, message, admin_set_comment_status(id, "rejected".into()))
                    }
                >
                    "拒绝"
                </button>
                <button
                    class="btn btn-small btn-danger"
                    on:click=move |_| run_action(revision, message, admin_delete_comment(id))
                >
                    {delete_hint}
                </button>
            </div>
        </article>
    }
}

/// 社区投稿管理：发布即公开，这里负责下架 / 恢复 / 删除。
#[component]
pub fn AdminCommunityPage() -> impl IntoView {
    let revision = RwSignal::new(0u32);
    let message = RwSignal::new(String::new());
    let filter = RwSignal::new("published".to_string());

    let posts = Resource::new_blocking(
        move || (revision.get(), filter.get()),
        |(_, status)| admin_list_community(status),
    );

    let filter_tab = move |label: &'static str, value: &'static str| {
        view! {
            <button
                class="segmented-btn"
                class:active=move || filter.get() == value
                on:click=move |_| filter.set(value.to_string())
            >
                {label}
            </button>
        }
    };

    view! {
        <Title text="社区管理 — Wraindrock" />
        <AdminPage title="社区管理" lede="发布即公开；这里负责下架、恢复与删除。".to_string() perm="community">
            <div class="segmented">
                {filter_tab("已发布", "published")}
                {filter_tab("已下架", "hidden")}
                {filter_tab("全部", "all")}
            </div>
            <p class="notice" role="status">{move || message.get()}</p>

            <Suspense fallback=|| view! { <p class="muted">"载入中…"</p> }>
                {move || match posts.get() {
                    None => view! { <p class="muted">"载入中…"</p> }.into_any(),
                    Some(Err(error)) => view! {
                        <p class="error">"载入社区内容失败："{error.to_string()}</p>
                    }
                    .into_any(),
                    Some(Ok(list)) if list.is_empty() => {
                        view! { <p class="admin-empty">"这个筛选下没有内容。"</p> }.into_any()
                    }
                    Some(Ok(list)) => view! {
                        <div class="admin-list">
                            {list
                                .into_iter()
                                .map(|post| {
                                    view! {
                                        <AdminCommunityRow
                                            post=post
                                            revision=revision
                                            message=message
                                        />
                                    }
                                })
                                .collect_view()}
                        </div>
                    }
                    .into_any(),
                }}
            </Suspense>

            <TagAdminPanel revision=revision message=message />
        </AdminPage>
    }
}

/// 后台里的一行社区内容。
#[component]
fn AdminCommunityRow(
    post: AdminCommunityPost,
    revision: RwSignal<u32>,
    message: RwSignal<String>,
) -> impl IntoView {
    let id = post.id;
    let kind = community_kind_label(&post.kind).to_string();
    let href = format!("/community/{}/{}", post.author_username, post.slug);
    let published = post.status == "published";

    view! {
        <article class="admin-row">
            <p class="admin-meta">
                <span class=format!("status status-{}", post.status)>{post.status.clone()}</span>
                <span class="badge">{kind}</span>
                <strong>{post.author.clone()}</strong>
                <span class="comment-time">{post.created_at.clone()}</span>
                <a href=href>{post.title.clone()}</a>
            </p>
            <div class="admin-actions">
                {published.then(|| view! {
                    <button
                        class="btn btn-small"
                        on:click=move |_| {
                            run_action(revision, message, admin_set_community_status(id, "hidden".into()))
                        }
                    >
                        "下架"
                    </button>
                })}
                {(!published).then(|| view! {
                    <button
                        class="btn btn-small"
                        on:click=move |_| {
                            run_action(revision, message, admin_set_community_status(id, "published".into()))
                        }
                    >
                        "恢复"
                    </button>
                })}
                <button
                    class="btn btn-small btn-danger"
                    on:click=move |_| run_action(revision, message, admin_delete_community(id))
                >
                    "删除"
                </button>
            </div>
        </article>
    }
}

/// 标签管理面板：重命名 / 置顶 / 合并。
#[component]
fn TagAdminPanel(revision: RwSignal<u32>, message: RwSignal<String>) -> impl IntoView {
    let tags = Resource::new_blocking(move || revision.get(), |_| admin_list_tags());

    view! {
        <section class="panel">
            <div class="panel-head">
                <div class="panel-head-main">
                    <h2 class="panel-title">"标签管理"</h2>
                    <p class="panel-desc">
                        "重命名、置顶，或把冷门标签合并到常用标签；合并会改写所有投稿的标签。"
                    </p>
                </div>
                <div class="panel-actions">
                    <button
                        class="btn btn-small"
                        on:click=move |_| revision.update(|n| *n += 1)
                    >
                        "刷新"
                    </button>
                </div>
            </div>
            <div class="panel-body">
                <Suspense fallback=|| view! { <p class="muted">"载入中…"</p> }>
                    {move || match tags.get() {
                        None => view! { <p class="muted">"载入中…"</p> }.into_any(),
                        Some(Err(error)) => view! {
                            <p class="error">"载入标签失败："{error.to_string()}</p>
                        }
                        .into_any(),
                        Some(Ok(list)) if list.is_empty() => view! {
                            <p class="admin-empty">"还没有标签，发布内容时会自动产生。"</p>
                        }
                        .into_any(),
                        Some(Ok(list)) => view! {
                            <div class="admin-list">
                                {list
                                    .iter()
                                    .map(|tag| {
                                        let others = list
                                            .iter()
                                            .filter(|t| t.tag != tag.tag)
                                            .cloned()
                                            .collect::<Vec<_>>();
                                        view! {
                                            <TagAdminRow
                                                tag=tag.clone()
                                                others=others
                                                revision=revision
                                                message=message
                                            />
                                        }
                                    })
                                    .collect_view()}
                            </div>
                        }
                        .into_any(),
                    }}
                </Suspense>
            </div>
        </section>
    }
}

/// 标签管理里的一行。
#[component]
fn TagAdminRow(
    tag: CommunityTag,
    others: Vec<CommunityTag>,
    revision: RwSignal<u32>,
    message: RwSignal<String>,
) -> impl IntoView {
    let raw = tag.tag.clone();
    let name = RwSignal::new(tag.display_name.clone());
    let pinned = RwSignal::new(tag.pinned);
    let target = RwSignal::new(String::new());
    let save_name = raw.clone();
    let save_pin = raw.clone();
    let merge_from = raw.clone();

    view! {
        <article class="admin-row">
            <p class="admin-meta">
                <span class="status">"#"{tag.tag.clone()}</span>
                <span class="comment-time">{format!("{} 条内容", tag.count)}</span>
                {tag.preset.then(|| view! { <span class="status status-active">"预置"</span> })}
                {tag.pinned.then(|| view! { <span class="status status-role-admin">"置顶"</span> })}
            </p>
            <div class="admin-actions">
                <input
                    class="text-input tag-rename-input"
                    type="text"
                    prop:value=move || name.get()
                    on:input=move |ev| name.set(event_target_value(&ev))
                />
                <button
                    class="btn btn-small"
                    on:click=move |_| {
                        let raw = save_name.clone();
                        run_action(
                            revision,
                            message,
                            admin_save_tag(raw, name.get(), pinned.get()),
                        );
                    }
                >
                    "保存"
                </button>
                <button
                    class="btn btn-small"
                    on:click=move |_| {
                        let raw = save_pin.clone();
                        let next = !pinned.get();
                        pinned.set(next);
                        run_action(
                            revision,
                            message,
                            admin_save_tag(raw, name.get(), next),
                        );
                    }
                >
                    {move || if pinned.get() { "取消置顶" } else { "置顶" }}
                </button>
                <select
                    prop:value=move || target.get()
                    on:change=move |ev| target.set(event_target_value(&ev))
                >
                    <option value="">"合并到…"</option>
                    {others
                        .iter()
                        .map(|t| {
                            view! { <option value=t.tag.clone()>{t.display_name.clone()}</option> }
                        })
                        .collect_view()}
                </select>
                <button
                    class="btn btn-small btn-danger"
                    on:click=move |_| {
                        let into = target.get();
                        if into.trim().is_empty() {
                            message.set("请先选择要合并到的目标标签。".to_string());
                            return;
                        }
                        run_action(
                            revision,
                            message,
                            admin_merge_tag(merge_from.clone(), into),
                        );
                    }
                >
                    "合并"
                </button>
            </div>
        </article>
    }
}

/// 评价审核与回复。
#[component]
pub fn AdminReviewsPage() -> impl IntoView {
    let revision = RwSignal::new(0u32);
    let message = RwSignal::new(String::new());
    let only_pending = RwSignal::new(true);

    let reviews = Resource::new_blocking(
        move || (revision.get(), only_pending.get()),
        |(_, pending)| admin_list_reviews(if pending { "pending".into() } else { "all".into() }),
    );

    view! {
        <Title text="评价审核 — Wraindrock" />
        <AdminPage title="评价审核" lede="通过后才会显示在代跑页上。".to_string() perm="sky">
            <StatusFilter only_pending=only_pending />
            <p class="notice" role="status">{move || message.get()}</p>

            <Suspense fallback=|| view! { <p class="muted">"载入中…"</p> }>
                {move || match reviews.get() {
                    None => view! { <p class="muted">"载入中…"</p> }.into_any(),
                    Some(Err(error)) => view! {
                        <p class="error">"载入评价失败："{error.to_string()}</p>
                    }
                    .into_any(),
                    Some(Ok(list)) if list.is_empty() => {
                        view! { <p class="admin-empty">"没有需要处理的评价。"</p> }.into_any()
                    }
                    Some(Ok(list)) => view! {
                        <div class="admin-list">
                            {list
                                .into_iter()
                                .map(|review| {
                                    view! {
                                        <AdminReviewRow
                                            review=review
                                            revision=revision
                                            message=message
                                        />
                                    }
                                })
                                .collect_view()}
                        </div>
                    }
                    .into_any(),
                }}
            </Suspense>
        </AdminPage>
    }
}

/// 后台里的一行评价。
#[component]
fn AdminReviewRow(
    review: AdminReview,
    revision: RwSignal<u32>,
    message: RwSignal<String>,
) -> impl IntoView {
    let id = review.id;
    let stars = "★".repeat(review.rating as usize);
    let featured = RwSignal::new(review.featured);
    // 已有回复必须出现在服务端渲染的 HTML 里：`prop:value` 只设 JS 属性，
    // 那样管理员会以为原本没回复过，一保存就把旧回复覆盖掉。
    let initial_reply = review.reply.clone().unwrap_or_default();
    let reply_draft = RwSignal::new(initial_reply.clone());

    view! {
        <article class="admin-row">
            <p class="admin-meta">
                <span class=format!("status status-{}", review.status)>{review.status.clone()}</span>
                <span class="stars">{stars}</span>
                <strong>{review.author.clone()}</strong>
                <span class="comment-time">{review.created_at.clone()}</span>
            </p>
            <p class="admin-body">{review.body.clone()}</p>

            <label class="field">
                <span>"回复（留空则撤销回复）"</span>
                <textarea
                    class="comment-input"
                    rows="2"
                    on:input=move |ev| reply_draft.set(event_target_value(&ev))
                >{initial_reply}</textarea>
            </label>

            <div class="admin-actions">
                <button
                    class="btn btn-small"
                    on:click=move |_| {
                        let next = !featured.get();
                        featured.set(next);
                        run_action(revision, message, admin_pin_review(id, next))
                    }
                >
                    {move || if featured.get() { "取消精选" } else { "精选" }}
                </button>
                <button
                    class="btn btn-small"
                    on:click=move |_| {
                        run_action(revision, message, admin_set_review_status(id, "approved".into()))
                    }
                >
                    "通过"
                </button>
                <button
                    class="btn btn-small"
                    on:click=move |_| {
                        run_action(revision, message, admin_set_review_status(id, "rejected".into()))
                    }
                >
                    "拒绝"
                </button>
                <button
                    class="btn btn-small btn-primary"
                    on:click=move |_| {
                        let reply = reply_draft.get_untracked();
                        run_action(revision, message, admin_reply_review(id, reply))
                    }
                >
                    "保存回复"
                </button>
                <button
                    class="btn btn-small btn-danger"
                    on:click=move |_| run_action(revision, message, admin_delete_review(id))
                >
                    "删除"
                </button>
            </div>
        </article>
    }
}

/// 用户管理。
#[component]
pub fn AdminUsersPage() -> impl IntoView {
    let revision = RwSignal::new(0u32);
    let message = RwSignal::new(String::new());
    let users = Resource::new_blocking(move || revision.get(), |_| admin_list_users());

    view! {
        <Title text="用户管理 — Wraindrock" />
        <AdminPage
            title="用户管理"
            lede="封禁后该账号立刻无法登录，已登录的会话也会立即失效；可授予细分管理角色。".to_string()
            perm="super"
        >
            <p class="notice" role="status">{move || message.get()}</p>

            <Suspense fallback=|| view! { <p class="muted">"载入中…"</p> }>
                {move || match users.get() {
                    None => view! { <p class="muted">"载入中…"</p> }.into_any(),
                    Some(Err(error)) => view! {
                        <p class="error">"载入用户失败："{error.to_string()}</p>
                    }
                    .into_any(),
                    Some(Ok(list)) => view! {
                        <div class="admin-list">
                            {list
                                .into_iter()
                                .map(|user| {
                                    view! {
                                        <AdminUserRow
                                            user=user
                                            revision=revision
                                            message=message
                                        />
                                    }
                                })
                                .collect_view()}
                        </div>
                    }
                    .into_any(),
                }}
            </Suspense>
        </AdminPage>
    }
}

/// 后台里的一行用户。
#[component]
fn AdminUserRow(
    user: AdminUser,
    revision: RwSignal<u32>,
    message: RwSignal<String>,
) -> impl IntoView {
    let id = user.id;
    let banned = user.status == "banned";
    let is_admin = user.role == "admin";
    let scopes = user.scopes.clone();

    // 「这个动作做完会变成什么」直接写在按钮上，不必让操作者心算
    let status_label = if banned { "解封" } else { "封禁" };
    let next_status = if banned { "active" } else { "banned" };
    let super_label = if is_admin { "取消超级管理员" } else { "设为超级管理员" };
    let next_role = if is_admin { "user" } else { "admin" };

    let scope_role = RwSignal::new(SCOPED_ROLES[0].to_string());

    view! {
        <article class="admin-row">
            <p class="admin-meta">
                <span class=if is_admin { "status status-role-admin" } else { "status" }>
                    {if is_admin { role_label("admin") } else { role_label("user") }}
                </span>
                {scopes
                    .into_iter()
                    .map(|scope| {
                        let value = scope.clone();
                        view! {
                            <span class="status status-role-admin">
                                {role_label(&scope)}
                                <button
                                    class="link-button"
                                    title="撤销该角色"
                                    on:click=move |_| {
                                        run_action(
                                            revision,
                                            message,
                                            admin_set_user_scope(id, value.clone(), false),
                                        );
                                    }
                                >
                                    "×"
                                </button>
                            </span>
                        }
                    })
                    .collect_view()}
                <span class=format!("status status-{}", user.status)>{user.status.clone()}</span>
                <strong>{user.display_name.clone()}</strong>
                <span class="comment-time">"@"{user.username.clone()}</span>
                <span class="comment-time">{format!("评论 {} 条", user.comment_count)}</span>
                {user
                    .last_login_at
                    .clone()
                    .map(|time| view! { <span class="comment-time">"上次登录 "{time}</span> })}
            </p>
            <div class="admin-actions">
                <select
                    prop:value=move || scope_role.get()
                    on:change=move |ev| scope_role.set(event_target_value(&ev))
                >
                    {SCOPED_ROLES
                        .iter()
                        .map(|role| view! { <option value=*role>{role_label(role)}</option> })
                        .collect_view()}
                </select>
                <button
                    class="btn btn-small"
                    on:click=move |_| {
                        run_action(
                            revision,
                            message,
                            admin_set_user_scope(id, scope_role.get(), true),
                        );
                    }
                >
                    "授予角色"
                </button>
                <button
                    class="btn btn-small"
                    on:click=move |_| {
                        run_action(revision, message, admin_set_user_status(id, next_status.into()))
                    }
                >
                    {status_label}
                </button>
                <button
                    class="btn btn-small"
                    on:click=move |_| {
                        run_action(revision, message, admin_set_user_role(id, next_role.into()))
                    }
                >
                    {super_label}
                </button>
            </div>
        </article>
    }
}

/// Pod 管理：查看 Felix-Homelab 内的容器状态，并可重启。
///
/// 复用站点后台的布局与样式，和「评论 / 用户」等页面保持一致的观感。
#[component]
pub fn AdminPodPage() -> impl IntoView {
    let revision = RwSignal::new(0u32);
    let message = RwSignal::new(String::new());
    let containers = Resource::new_blocking(move || revision.get(), |_| admin_list_pod());

    view! {
        <Title text="Pod 管理 — Wraindrock" />
        <AdminPage
            title="Pod 管理"
            lede="Felix-Homelab 内的容器状态；重启会短暂中断对应服务。".to_string()
            perm="super"
        >
            <p class="notice" role="status">{move || message.get()}</p>

            <div class="toolbar">
                <button class="btn btn-small" on:click=move |_| revision.update(|n| *n += 1)>
                    "刷新"
                </button>
            </div>

            <Suspense fallback=|| view! { <p class="muted">"载入中…"</p> }>
                {move || match containers.get() {
                    None => view! { <p class="muted">"载入中…"</p> }.into_any(),
                    Some(Err(error)) => view! {
                        <p class="error">"读取容器失败："{error.to_string()}</p>
                    }
                    .into_any(),
                    Some(Ok(list)) => view! {
                        <div class="admin-list">
                            {list
                                .into_iter()
                                .map(|container| {
                                    view! {
                                        <PodRow
                                            container=container
                                            revision=revision
                                            message=message
                                        />
                                    }
                                })
                                .collect_view()}
                        </div>
                    }
                    .into_any(),
                }}
            </Suspense>
        </AdminPage>
    }
}

/// Pod 管理里的一行容器。
#[component]
fn PodRow(
    container: PodContainer,
    revision: RwSignal<u32>,
    message: RwSignal<String>,
) -> impl IntoView {
    let running = container.state == "running";
    let state_class = if running { "status status-running" } else { "status status-exited" };
    let health_class = match container.health.as_deref() {
        Some("healthy") => "status status-healthy",
        Some("unhealthy") => "status status-unhealthy",
        _ => "status",
    };
    let restart_name = container.name.clone();
    let image = container.image.clone();
    let row_class = if running && container.health.as_deref() != Some("unhealthy") {
        "admin-row row-ok"
    } else {
        "admin-row row-bad"
    };

    view! {
        <article class=row_class>
            <p class="admin-meta">
                <span class=state_class>{container.state.clone()}</span>
                {container
                    .health
                    .clone()
                    .map(|health| view! { <span class=health_class>{health}</span> })}
                <strong>{container.name.clone()}</strong>
                <span class="comment-time">{container.status.clone()}</span>
                <span class="admin-image" title=image.clone()>
                    {image.clone()}
                </span>
            </p>
            <div class="admin-actions">
                <button
                    class="btn btn-small"
                    disabled=!running
                    on:click=move |_| {
                        if !running {
                            return;
                        }
                        run_action(
                            revision,
                            message,
                            admin_restart_container(restart_name.clone()),
                        );
                    }
                >
                    "重启"
                </button>
            </div>
        </article>
    }
}

/// Agent 管理（多租户试点）：授权/续费、启停、删除与运行状态。
///
/// 站点只改订阅与请求文件，真正建容器/路由由宿主 `scripts/agent-ctl.sh`
/// 在 systemd 触发后完成，状态通过 `/agents/status.json` 回显。
#[component]
pub fn AdminAgentPage() -> impl IntoView {
    let revision = RwSignal::new(0u32);
    let message = RwSignal::new(String::new());
    let show_deleted = RwSignal::new(false);
    let agents = Resource::new_blocking(
        move || (revision.get(), show_deleted.get()),
        |(_, include_deleted)| admin_list_agents(include_deleted),
    );

    let name = RwSignal::new(String::new());
    let kind = RwSignal::new("opencode".to_string());
    let count = RwSignal::new(1i64);
    let days = RwSignal::new(30i64);
    let note = RwSignal::new(String::new());

    let submit_grant = move |_| {
        if name.get().trim().is_empty() {
            message.set("请填写要开通的用户名。".to_string());
            return;
        }
        run_action(
            revision,
            message,
            admin_grant_agent(name.get(), kind.get(), count.get(), days.get(), note.get()),
        );
    };

    view! {
        <Title text="Agent 管理 — Wraindrock" />
        <AdminPage
            title="Agent 管理"
            lede="每个授权账号一个独立容器（模板镜像 + 独立数据卷 + 独立子域）。授权/停用后由宿主脚本自动执行。".to_string()
            perm="agent"
        >
            <p class="notice" role="status">{move || message.get()}</p>

            <section class="panel">
                <div class="panel-head">
                    <div class="panel-head-main">
                        <h2 class="panel-title">"开通新实例"</h2>
                        <p class="panel-desc">
                            "为指定用户新建该类型的 N 个实例（新的随机域名，与现有实例互不影响）。"
                            "0 天 = 长期有效；续费 / 复活已撤销的实例请在下方每一行操作。"
                            "DeepSeek Harness 首次进入用卡片上的带令牌链接。"
                        </p>
                    </div>
                </div>
                <div class="panel-body">
                    <div class="form-grid">
                        <label class="field">
                            <span>"用户名"</span>
                            <input
                                type="text"
                                placeholder="站点账号（大小写不敏感）"
                                prop:value=move || name.get()
                                on:input=move |ev| name.set(event_target_value(&ev))
                            />
                        </label>
                        <label class="field">
                            <span>"模板"</span>
                            <select
                                prop:value=move || kind.get()
                                on:change=move |ev| kind.set(event_target_value(&ev))
                            >
                                <option value="opencode">"OpenCode"</option>
                                <option value="dsh">"DeepSeek Harness"</option>
                                <option value="zeroclaw">"ZeroClaw"</option>
                                <option value="kilocode">"Kilo Code"</option>
                                <option value="pi">"Pi"</option>
                            </select>
                        </label>
                        <label class="field">
                            <span>"数量"</span>
                            <input
                                type="number"
                                min="1"
                                max="9"
                                prop:value=move || count.get().to_string()
                                on:input=move |ev| {
                                    count.set(event_target_value(&ev).parse::<i64>().unwrap_or(1));
                                }
                            />
                        </label>
                        <label class="field">
                            <span>"有效天数"</span>
                            <input
                                type="number"
                                min="0"
                                max="3650"
                                prop:value=move || days.get().to_string()
                                on:input=move |ev| {
                                    days.set(event_target_value(&ev).parse::<i64>().unwrap_or(30));
                                }
                            />
                        </label>
                        <label class="field">
                            <span>"备注"</span>
                            <input
                                type="text"
                                placeholder="付款记录等（可选）"
                                prop:value=move || note.get()
                                on:input=move |ev| note.set(event_target_value(&ev))
                            />
                        </label>
                        <div class="field field-action">
                            <button class="btn btn-primary" on:click=submit_grant>"开通"</button>
                        </div>
                    </div>
                </div>
            </section>

            <div class="toolbar">
                <button class="btn btn-small" on:click=move |_| revision.update(|n| *n += 1)>
                    "刷新状态"
                </button>
                <label class="toggle-row">
                    <input
                        type="checkbox"
                        prop:checked=move || show_deleted.get()
                        on:change=move |ev| show_deleted.set(event_target_checked(&ev))
                    />
                    <span class="toggle-text">"显示已删除 / 彻底删除记录（留存 30 天，默认隐藏）"</span>
                </label>
            </div>

            <Suspense fallback=|| view! { <p class="muted">"载入中…"</p> }>
                {move || match agents.get() {
                    None => view! { <p class="muted">"载入中…"</p> }.into_any(),
                    Some(Err(error)) => view! {
                        <p class="error">"读取 Agent 列表失败："{error.to_string()}</p>
                    }
                    .into_any(),
                    Some(Ok(list)) if list.is_empty() => view! {
                        <p class="admin-empty">"还没有开通任何 Agent。"</p>
                    }
                    .into_any(),
                    Some(Ok(list)) => view! {
                        <div class="admin-list">
                            {list
                                .into_iter()
                                .map(|agent| {
                                    view! {
                                        <AgentRowView
                                            agent=agent
                                            revision=revision
                                            message=message
                                        />
                                    }
                                })
                                .collect_view()}
                        </div>
                    }
                    .into_any(),
                }}
            </Suspense>
        </AdminPage>
    }
}

/// Agent 管理里的一行。
#[component]
fn AgentRowView(
    agent: AgentRow,
    revision: RwSignal<u32>,
    message: RwSignal<String>,
) -> impl IntoView {
    let purged = agent.purged_at.is_some();
    let status_label = if purged {
        "已彻底删除"
    } else {
        match agent.status.as_str() {
            "active" => "已授权",
            "stopped" => "已暂停",
            "revoked" => "已撤销（30 天内可续期）",
            other => other,
        }
    };
    let status_class = if purged {
        "status"
    } else {
        match agent.status.as_str() {
            "active" => "status status-running",
            "stopped" => "status status-exited",
            _ => "status",
        }
    };
    let runtime_label = match &agent.runtime {
        Some(runtime) if runtime.desired == "sleeping" => "睡眠中".to_string(),
        Some(runtime) => match runtime.state.as_str() {
            "running" => "运行中",
            "exited" => "已停止",
            "created" => "已创建",
            other => other,
        }
        .to_string(),
        None => "未创建（等待宿主执行）".to_string(),
    };
    let runtime_class = match agent.runtime.as_ref().map(|r| r.state.as_str()) {
        Some("running") => "status status-running",
        Some(_) => "status status-exited",
        None => "status",
    };
    let health = agent.runtime.as_ref().and_then(|r| r.health.clone());
    let health_class = match health.as_deref() {
        Some("healthy") => "status status-healthy",
        Some("unhealthy") => "status status-unhealthy",
        _ => "status",
    };
    let port = agent.runtime.as_ref().map(|r| r.port).unwrap_or(0);

    let slot = agent.slot;
    let revoked = agent.status == "revoked" && !purged;
    let row_class = if purged {
        "admin-row"
    } else if revoked {
        "admin-row row-bad"
    } else if agent.status == "stopped"
        || agent.runtime.as_ref().map(|r| r.desired.as_str()) == Some("sleeping")
    {
        "admin-row row-warn"
    } else if agent.runtime.is_some() {
        "admin-row row-ok"
    } else {
        "admin-row row-info"
    };
    let renew_days = RwSignal::new(30i64);
    let confirming = RwSignal::new(false);
    let confirm_text = RwSignal::new(String::new());
    let confirm_expected = format!(
        "我确认永久删除{}",
        agent.subdomain.split('.').next().unwrap_or("")
    );
    let confirm_expected_check = confirm_expected.clone();
    let username_start = agent.username.clone();
    let username_stop = agent.username.clone();
    let username_remove = agent.username.clone();
    let username_renew = agent.username.clone();
    let username_purge = agent.username.clone();

    view! {
        <article class=row_class>
            <p class="admin-meta">
                <span class=status_class>{status_label.to_string()}</span>
                {(!purged)
                    .then(|| {
                        view! { <span class=runtime_class>{runtime_label.clone()}</span> }
                    })}
                {(!purged)
                    .then(|| health.map(|health| view! { <span class=health_class>{health}</span> }))}
                <strong>{agent.username.clone()}</strong>
                <span class="admin-image" title="实例唯一标识">{agent.subdomain.clone()}</span>
                <span class="admin-image">{agent_kind_label(&agent.kind)}</span>
                {agent
                    .expires_at
                    .clone()
                    .map(|expires| {
                        view! { <span class="comment-time">"到期 " {expires}</span> }
                    })}
                {(!purged && port > 0)
                    .then(|| view! { <span class="comment-time">"回环 " {port}</span> })}
                {agent
                    .purged_at
                    .clone()
                    .map(|time| {
                        view! { <span class="comment-time">"彻底删除于 " {time}</span> }
                    })}
            </p>
            <p class="admin-body">
                {if purged {
                    view! { <span>{agent.url.clone()}</span> }.into_any()
                } else {
                    let href = agent
                        .login_url
                        .clone()
                        .unwrap_or_else(|| agent.url.clone());
                    view! { <a href=href target="_blank" rel="noreferrer">{agent.url.clone()}</a> }
                        .into_any()
                }}
                {(agent.kind == "dsh")
                    .then(|| view! { <span class="comment-time">"（带令牌登录链接）"</span> })}
                {(!agent.note.trim().is_empty())
                    .then(|| {
                        view! {
                            <span class="comment-time">"　" {agent.note.clone()}</span>
                        }
                    })}
            </p>
            {purged
                .then(|| {
                    view! {
                        <div class="admin-actions">
                            <span class="muted">"记录留存中（30 天后自动清理）"</span>
                        </div>
                    }
                })}
            {(!purged)
                .then(|| {
                    view! {
                        <div class="admin-actions">
                            <input
                                class="text-input renew-days"
                                type="number"
                                min="1"
                                max="3650"
                                title="续费天数"
                                prop:value=move || renew_days.get().to_string()
                                on:input=move |ev| {
                                    renew_days.set(event_target_value(&ev).parse::<i64>().unwrap_or(30));
                                }
                            />
                            <button
                                class="btn btn-small"
                                on:click=move |_| {
                                    let username = username_renew.clone();
                                    let days = renew_days.get();
                                    run_action(
                                        revision,
                                        message,
                                        admin_renew_agent(username, slot, days, String::new()),
                                    );
                                }
                            >
                                {if revoked { "续费恢复" } else { "续费" }}
                            </button>
                            {(!revoked)
                                .then(|| {
                                    view! {
                                        <button
                                            class="btn btn-small"
                                            on:click=move |_| {
                                                run_action(
                                                    revision,
                                                    message,
                                                    admin_agent_action(
                                                        username_start.clone(),
                                                        slot,
                                                        "start".to_string(),
                                                    ),
                                                );
                                            }
                                        >
                                            "启动"
                                        </button>
                                        <button
                                            class="btn btn-small"
                                            on:click=move |_| {
                                                run_action(
                                                    revision,
                                                    message,
                                                    admin_agent_action(
                                                        username_stop.clone(),
                                                        slot,
                                                        "stop".to_string(),
                                                    ),
                                                );
                                            }
                                        >
                                            "暂停"
                                        </button>
                                        <button
                                            class="btn btn-small btn-danger"
                                            on:click=move |_| {
                                                run_action(
                                                    revision,
                                                    message,
                                                    admin_agent_action(
                                                        username_remove.clone(),
                                                        slot,
                                                        "remove".to_string(),
                                                    ),
                                                );
                                            }
                                        >
                                            "删除"
                                        </button>
                                    }
                                })}
                            {revoked
                                .then(|| {
                                    view! {
                                        <button
                                            class="btn btn-small btn-danger"
                                            on:click=move |_| confirming.set(true)
                                        >
                                            "永久删除"
                                        </button>
                                    }
                                })}
                        </div>
                    }
                })}
            {move || {
                // 外层是响应式闭包（会多次执行）：每次克隆内层要 move 的捕获
                let username = username_purge.clone();
                let expected_display = confirm_expected.clone();
                let expected_check = confirm_expected_check.clone();
                (confirming.get() && revoked)
                    .then(|| {
                        view! {
                            <div class="agent-key">
                                <p class="muted">
                                    "永久删除会立即销毁容器、数据卷、工作区与子域路由，不可恢复。"
                                    "请原样输入："
                                    <code>{expected_display.clone()}</code>
                                </p>
                                <div class="field-row">
                                    <input
                                        class="text-input"
                                        type="text"
                                        placeholder=expected_display.clone()
                                        prop:value=move || confirm_text.get()
                                        on:input=move |ev| confirm_text.set(event_target_value(&ev))
                                    />
                                    <button
                                        class="btn btn-small btn-danger"
                                        disabled=move || {
                                            confirm_text.get().trim() != expected_check
                                        }
                                        on:click=move |_| {
                                            let username = username.clone();
                                            let text = confirm_text.get();
                                            confirming.set(false);
                                            confirm_text.set(String::new());
                                            run_action(
                                                revision,
                                                message,
                                                admin_purge_agent(username, slot, text),
                                            );
                                        }
                                    >
                                        "确认永久删除"
                                    </button>
                                    <button
                                        class="btn btn-small"
                                        on:click=move |_| {
                                            confirming.set(false);
                                            confirm_text.set(String::new());
                                        }
                                    >
                                        "取消"
                                    </button>
                                </div>
                            </div>
                        }
                    })
            }}
        </article>
    }
}

/// 一个备份源的开关行。`disabled` 用于始终开启、不由页面控制的源（如 OpenCloud）。
#[component]
fn SourceToggle(
    label: &'static str,
    hint: &'static str,
    checked: RwSignal<bool>,
    #[prop(optional)] disabled: bool,
) -> impl IntoView {
    view! {
        <label class="toggle-row" class:is-disabled=disabled>
            <input
                type="checkbox"
                disabled=disabled
                prop:checked=move || checked.get()
                on:change=move |ev| checked.set(event_target_checked(&ev))
            />
            <span class="toggle-text">
                <strong>{label}</strong>
                <small>{hint}</small>
            </span>
        </label>
    }
}

/// 单个异地备份渠道（可编辑卡片）。
#[component]
fn BackupChannelCard(index: usize, channels: RwSignal<Vec<BackupChannel>>) -> impl IntoView {
    let name = move || {
        channels
            .get()
            .get(index)
            .map(|c| c.name.clone())
            .unwrap_or_default()
    };
    let kind = move || {
        channels
            .get()
            .get(index)
            .map(|c| c.kind.clone())
            .unwrap_or_default()
    };
    let target = move || {
        channels
            .get()
            .get(index)
            .map(|c| c.target.clone())
            .unwrap_or_default()
    };
    let enabled = move || channels.get().get(index).map(|c| c.enable).unwrap_or(false);

    view! {
        <article class="channel-card">
            <div class="channel-head">
                <input
                    class="channel-name"
                    type="text"
                    placeholder="渠道名称（如：WebDAV 网盘）"
                    prop:value=name
                    on:input=move |ev| {
                        let value = event_target_value(&ev);
                        channels
                            .update(|list| {
                                if let Some(channel) = list.get_mut(index) {
                                    channel.name = value;
                                }
                            });
                    }
                />
                <label class="channel-toggle">
                    <input
                        type="checkbox"
                        prop:checked=enabled
                        on:change=move |ev| {
                            let value = event_target_checked(&ev);
                            channels
                                .update(|list| {
                                    if let Some(channel) = list.get_mut(index) {
                                        channel.enable = value;
                                    }
                                });
                        }
                    />
                    <span>"启用"</span>
                </label>
                <button
                    class="btn btn-small btn-danger"
                    on:click=move |_| channels.update(|list| {
                        list.remove(index);
                    })
                >
                    "删除"
                </button>
            </div>
            <div class="field-row">
                <label class="field">
                    <span>"类型"</span>
                    <select
                        prop:value=kind
                        on:change=move |ev| {
                            let value = event_target_value(&ev);
                            channels
                                .update(|list| {
                                    if let Some(channel) = list.get_mut(index) {
                                        channel.kind = value;
                                    }
                                });
                        }
                    >
                        <option value="rclone">"rclone（WebDAV / S3 / R2 / OSS…）"</option>
                        <option value="rsync">"rsync（外置盘 / NAS / SSH）"</option>
                    </select>
                </label>
                <label class="field field-grow">
                    <span>"目标"</span>
                    <input
                        type="text"
                        placeholder="webdav:我的网盘/felix-backups 或 /run/media/felix/外置盘/felix"
                        prop:value=target
                        on:input=move |ev| {
                            let value = event_target_value(&ev);
                            channels
                                .update(|list| {
                                    if let Some(channel) = list.get_mut(index) {
                                        channel.target = value;
                                    }
                                });
                        }
                    />
                </label>
            </div>
        </article>
    }
}

/// 由归档文件名判断备份源（兼容旧命名 felix-ws-*）。
fn backup_source(name: &str) -> &'static str {
    if name.starts_with("felix-homelab-site-") || name.starts_with("felix-ws-site-") {
        "主站"
    } else if name.starts_with("felix-homelab-opencloud-") {
        "OpenCloud"
    } else {
        "Forgejo"
    }
}

/// 备份（分源 + 多渠道）。
#[component]
pub fn AdminBackupPage() -> impl IntoView {
    let revision = RwSignal::new(0u32);
    let message = RwSignal::new(String::new());
    let backups = Resource::new_blocking(move || revision.get(), |_| admin_backup_list());
    let config = Resource::new_blocking(|| (), |_| admin_backup_config());

    let forgejo = RwSignal::new(true);
    let site = RwSignal::new(true);
    let keep_days = RwSignal::new("7".to_string());
    let channels = RwSignal::new(Vec::<BackupChannel>::new());
    let sync_status = RwSignal::new(String::new());
    let loaded = RwSignal::new(false);
    // OpenCloud 的备份由宿主脚本无条件执行，这里只是展示，不参与保存
    let opencloud_always = RwSignal::new(true);

    // 读取配置后填充表单（只填一次，避免覆盖正在编辑的内容）
    Effect::new(move |_| {
        if let Some(Ok(cfg)) = config.get() {
            if !loaded.get_untracked() {
                forgejo.set(cfg.forgejo);
                site.set(cfg.site);
                keep_days.set(cfg.keep_days.to_string());
                channels.set(cfg.channels.clone());
                loaded.set(true);
            }
            sync_status.set(cfg.sync_status.clone());
        }
    });

    let save = move |_: leptos::ev::MouseEvent| {
        let days = keep_days
            .get_untracked()
            .trim()
            .parse::<u32>()
            .unwrap_or(7)
            .clamp(1, 90);
        keep_days.set(days.to_string());
        let (forgejo, site, list) = (
            forgejo.get_untracked(),
            site.get_untracked(),
            channels.get_untracked(),
        );
        leptos::task::spawn_local(async move {
            match admin_backup_save_config(forgejo, site, days, Some(list)).await {
                Ok(Ok(())) => {
                    message.set("设置已保存。".to_string());
                    config.refetch();
                }
                Ok(Err(e)) => message.set(e),
                Err(e) => message.set(format!("请求失败：{e}")),
            }
        });
    };

    let do_backup = move |_: leptos::ev::MouseEvent| {
        message.set("正在备份已启用的备份源…".to_string());
        leptos::task::spawn_local(async move {
            match admin_backup_now().await {
                Ok(Ok(name)) => {
                    message.set(format!("备份完成：{name}"));
                    revision.update(|n| *n += 1);
                }
                Ok(Err(e)) => message.set(e),
                Err(e) => message.set(format!("请求失败：{e}")),
            }
        });
    };

    let do_sync = move |_: leptos::ev::MouseEvent| {
        message.set("已请求同步；渠道较大时需要一会儿，可点「刷新状态」查看结果。".to_string());
        leptos::task::spawn_local(async move {
            match admin_backup_trigger_sync().await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => message.set(e),
                Err(e) => message.set(format!("请求失败：{e}")),
            }
        });
    };

    view! {
        <Title text="备份 — Wraindrock" />
        <AdminPage
            title="备份"
            lede="选择要备份的内容；每个备份源、每个异地渠道都能单独开关。".to_string()
            perm="super"
        >
            <p class="notice" role="status">{move || message.get()}</p>

            <section class="panel">
                <div class="panel-head">
                    <span class="step-badge">"1"</span>
                    <div class="panel-head-main">
                        <h2 class="panel-title">"备份内容"</h2>
                        <p class="panel-desc">
                            "每天 03:00 自动备份（关机 / 休眠错过后开机补跑）。「立即备份」按已保存的设置执行；改过开关先保存。"
                        </p>
                    </div>
                    <div class="panel-actions">
                        <button class="btn btn-small" on:click=do_backup>"立即备份"</button>
                        <button class="btn btn-primary btn-small" on:click=save>"保存设置"</button>
                    </div>
                </div>
                <div class="panel-body">
                    <div class="toggle-list">
                        <SourceToggle
                            label="Forgejo"
                            hint="代码仓库、账号、Issue、Actions 运行记录"
                            checked=forgejo
                        />
                        <SourceToggle
                            label="主站"
                            hint="主站文章、社区投稿、评论与上传图片"
                            checked=site
                        />
                        <SourceToggle
                            label="OpenCloud"
                            hint="文件同步 / 分享：数据 + 配置；由宿主脚本打包（带 xattr），始终开启"
                            checked=opencloud_always
                            disabled=true
                        />
                    </div>
                    <label class="field field-inline">
                        <span>"保留天数"</span>
                        <input
                            type="number"
                            min="1"
                            max="90"
                            prop:value=move || keep_days.get()
                            on:input=move |ev| keep_days.set(event_target_value(&ev))
                        />
                    </label>
                    <div class="toolbar">
                        <button class="btn btn-small" on:click=move |_| revision.update(|n| *n += 1)>
                            "刷新归档"
                        </button>
                    </div>
                </div>
            </section>

            <section class="panel">
                <div class="panel-head">
                    <span class="step-badge">"2"</span>
                    <div class="panel-head-main">
                        <h2 class="panel-title">"异地备份渠道"</h2>
                        <p class="panel-desc">
                            "每个渠道独立开关；可同时开启多个，也可以全部关闭。全部关闭时只保留本机备份。"
                        </p>
                    </div>
                    <div class="panel-actions">
                        <button class="btn btn-small" on:click=do_sync>"立即同步"</button>
                        <button class="btn btn-primary btn-small" on:click=save>"保存设置"</button>
                    </div>
                </div>
                <div class="panel-body">
                <div class="channel-list">
                    {move || {
                        channels
                            .get()
                            .iter()
                            .enumerate()
                            .map(|(index, _)| {
                                view! { <BackupChannelCard index=index channels=channels /> }
                            })
                            .collect_view()
                    }}
                </div>
                {move || {
                    channels
                        .get()
                        .is_empty()
                        .then(|| view! { <p class="admin-empty">"还没有渠道，点下面「添加渠道」。"</p> })
                }}
                <div class="toolbar">
                    <button
                        class="btn btn-small"
                        on:click=move |_| channels
                            .update(|list| {
                                list.push(BackupChannel {
                                    name: String::new(),
                                    kind: "rclone".to_string(),
                                    target: String::new(),
                                    enable: true,
                                });
                            })
                    >
                        "添加渠道"
                    </button>
                </div>
                <h3 class="admin-subsection-title">"最近同步"</h3>
                <pre class="sync-status">
                    {move || {
                        let status = sync_status.get();
                        if status.trim().is_empty() {
                            "（还没有同步记录；渠道全部关闭时也不会同步）".to_string()
                        } else {
                            status
                        }
                    }}
                </pre>
                <div class="toolbar">
                    <button
                        class="btn btn-small"
                        on:click=move |_: leptos::ev::MouseEvent| config.refetch()
                    >
                        "刷新状态"
                    </button>
                </div>
                </div>
            </section>

            <section class="panel">
                <div class="panel-head">
                    <span class="step-badge">"3"</span>
                    <div class="panel-head-main">
                        <h2 class="panel-title">"本机归档"</h2>
                        <p class="panel-desc">
                            "归档保存在宿主机 ~/.local/share/felix-homelab/backups/；按上面保存的保留天数自动清理。"
                        </p>
                    </div>
                </div>
                <div class="panel-body">
                    <Suspense fallback=|| view! { <p class="muted">"载入中…"</p> }>
                        {move || match backups.get() {
                            None => view! { <p class="muted">"载入中…"</p> }.into_any(),
                            Some(Err(e)) => {
                                view! { <p class="error">"读取失败："{e.to_string()}</p> }
                                    .into_any()
                            }
                            Some(Ok(list)) if list.is_empty() => {
                                view! {
                                    <p class="admin-empty">"还没有备份，点上面「立即备份」。"</p>
                                }
                                    .into_any()
                            }
                            Some(Ok(list)) => {
                                view! {
                                    <div class="archive-list">
                                        {list
                                            .into_iter()
                                            .map(|f| {
                                                let source = backup_source(&f.name);
                                                view! {
                                                    <div class="archive-row">
                                                        <span class="badge">{source}</span>
                                                        <span class="archive-name">
                                                            {f.name.clone()}
                                                        </span>
                                                        <span class="archive-meta">{f.size.clone()}</span>
                                                        <span class="archive-meta">{f.time.clone()}</span>
                                                    </div>
                                                }
                                            })
                                            .collect_view()}
                                    </div>
                                }
                                    .into_any()
                            }
                        }}
                    </Suspense>
                </div>
            </section>

            <section class="panel">
                <div class="panel-head">
                    <span class="step-badge">"4"</span>
                    <div class="panel-head-main">
                        <h2 class="panel-title">"恢复"</h2>
                        <p class="panel-desc">
                            "在宿主机执行 make restore（默认恢复最新归档，会先做一次安全备份再覆盖）。"
                            " 各备份源恢复细节见 README「备份与恢复」。"
                        </p>
                    </div>
                </div>
            </section>
        </AdminPage>
    }
}

/// 光遇管理：官方内容（攻略/画廊）、代跑展示、光遇投稿。
///
/// 光遇是独立板块：投稿在这里管理（加精/下架/删除），评价精选在「评价」页。
#[component]
pub fn AdminSkyPage() -> impl IntoView {
    let revision = RwSignal::new(0u32);
    let message = RwSignal::new(String::new());

    let officials = Resource::new_blocking(move || revision.get(), |_| admin_list_sky_official());
    let show_hidden = RwSignal::new(false);
    let posts = Resource::new_blocking(
        move || (revision.get(), show_hidden.get()),
        |(_, hidden)| {
            admin_list_sky_posts(if hidden { "hidden".into() } else { "published".into() })
        },
    );
    let boosting = Resource::new_blocking(move || revision.get(), |_| sky_boosting());

    // —— 官方内容表单 ——
    let edit_id = RwSignal::new(None::<i64>);
    let f_category = RwSignal::new("gameplay".to_string());
    let f_slug = RwSignal::new(String::new());
    let f_title = RwSignal::new(String::new());
    let f_summary = RwSignal::new(String::new());
    let f_body = RwSignal::new(String::new());
    let f_cover = RwSignal::new(String::new());
    let f_sort = RwSignal::new(0i64);
    let f_status = RwSignal::new("published".to_string());

    let save_official = move |_| {
        let id = edit_id.get_untracked();
        run_action(
            revision,
            message,
            admin_save_sky_official(
                id,
                f_category.get_untracked(),
                f_slug.get_untracked(),
                f_title.get_untracked(),
                f_summary.get_untracked(),
                f_body.get_untracked(),
                f_cover.get_untracked(),
                f_sort.get_untracked(),
                f_status.get_untracked(),
            ),
        );
        edit_id.set(None);
        f_title.set(String::new());
        f_slug.set(String::new());
        f_summary.set(String::new());
        f_body.set(String::new());
        f_cover.set(String::new());
        f_sort.set(0);
    };

    // —— 公告 ——
    let announcement = RwSignal::new(String::new());
    let loaded = RwSignal::new(false);
    Effect::new(move |_| {
        if let Some(Ok(b)) = boosting.get() {
            if !loaded.get_untracked() {
                announcement.set(b.announcement.clone());
                loaded.set(true);
            }
        }
    });
    let save_announcement = move |_| {
        run_action(
            revision,
            message,
            admin_save_sky_boosting(announcement.get_untracked()),
        );
    };

    view! {
        <Title text="光遇管理 — Wraindrock" />
        <AdminPage
            title="光遇管理"
            lede="官方内容（攻略/画廊）、代跑展示与光遇投稿；评价精选在「评价」页。".to_string()
            perm="sky"
        >
            <p class="notice" role="status">{move || message.get()}</p>

            <section class="panel">
                <div class="panel-head">
                    <span class="step-badge">"1"</span>
                    <div class="panel-head-main">
                        <h2 class="panel-title">"官方内容"</h2>
                        <p class="panel-desc">"攻略与画廊的站内内容；发布后即出现在对应分类页。"</p>
                    </div>
                    <div class="panel-actions">
                        <button class="btn btn-small" on:click=move |_| revision.update(|n| *n += 1)>
                            "刷新"
                        </button>
                    </div>
                </div>
                <div class="panel-body">
                    <div class="form-grid">
                        <label class="field">
                            <span>"分类"</span>
                            <select
                                prop:value=move || f_category.get()
                                on:change=move |ev| f_category.set(event_target_value(&ev))
                            >
                                <option value="gameplay">"攻略"</option>
                                <option value="gallery">"画廊"</option>
                            </select>
                        </label>
                        <label class="field">
                            <span>"标题"</span>
                            <input
                                type="text"
                                prop:value=move || f_title.get()
                                on:input=move |ev| f_title.set(event_target_value(&ev))
                            />
                        </label>
                        <label class="field">
                            <span>"链接名（留空自动生成）"</span>
                            <input
                                type="text"
                                prop:value=move || f_slug.get()
                                on:input=move |ev| f_slug.set(event_target_value(&ev))
                            />
                        </label>
                        <label class="field">
                            <span>"摘要（可留空）"</span>
                            <input
                                type="text"
                                prop:value=move || f_summary.get()
                                on:input=move |ev| f_summary.set(event_target_value(&ev))
                            />
                        </label>
                        <label class="field">
                            <span>"封面地址（可留空）"</span>
                            <input
                                type="text"
                                prop:value=move || f_cover.get()
                                on:input=move |ev| f_cover.set(event_target_value(&ev))
                            />
                        </label>
                        <label class="field">
                            <span>"排序（大的在前）"</span>
                            <input
                                type="number"
                                prop:value=move || f_sort.get().to_string()
                                on:input=move |ev| {
                                    f_sort.set(event_target_value(&ev).parse::<i64>().unwrap_or(0));
                                }
                            />
                        </label>
                        <label class="field">
                            <span>"状态"</span>
                            <select
                                prop:value=move || f_status.get()
                                on:change=move |ev| f_status.set(event_target_value(&ev))
                            >
                                <option value="published">"发布"</option>
                                <option value="hidden">"隐藏"</option>
                            </select>
                        </label>
                        <div class="field field-action">
                            <button class="btn btn-primary" on:click=save_official>
                                {move || if edit_id.get().is_some() { "保存修改" } else { "新建内容" }}
                            </button>
                        </div>
                    </div>
                    <label class="field">
                        <span>"正文（Markdown）"</span>
                        <textarea
                            class="comment-input"
                            rows="8"
                            prop:value=move || f_body.get()
                            on:input=move |ev| f_body.set(event_target_value(&ev))
                        ></textarea>
                    </label>

                    <Suspense fallback=|| view! { <p class="muted">"载入中…"</p> }>
                        {move || match officials.get() {
                            None => view! { <p class="muted">"载入中…"</p> }.into_any(),
                            Some(Err(e)) => view! {
                                <p class="error">"读取失败："{e.to_string()}</p>
                            }
                                .into_any(),
                            Some(Ok(list)) if list.is_empty() => view! {
                                <p class="admin-empty">"还没有站内官方内容。"</p>
                            }
                                .into_any(),
                            Some(Ok(list)) => {
                                view! {
                                    <div class="admin-list">
                                        {list
                                            .into_iter()
                                            .map(|o| {
                                                let o_edit = o.clone();
                                                let category = o.category.clone();
                                                let status = o.status.clone();
                                                view! {
                                                    <article class="admin-row">
                                                        <p class="admin-meta">
                                                            <span class=format!("status status-{}", status)>
                                                                {o.status.clone()}
                                                            </span>
                                                            <span class="badge">
                                                                {if category == "gallery" { "画廊" } else { "攻略" }}
                                                            </span>
                                                            <strong>{o.title.clone()}</strong>
                                                            <span class="comment-time">
                                                                {format!("#{}", o.slug)}
                                                            </span>
                                                            <span class="comment-time">
                                                                {format!("排序 {}", o.sort)}
                                                            </span>
                                                        </p>
                                                        <div class="admin-actions">
                                                            <button
                                                                class="btn btn-small"
                                                                on:click=move |_| {
                                                                    let o = o_edit.clone();
                                                                    edit_id.set(Some(o.id));
                                                                    f_category.set(o.category.clone());
                                                                    f_slug.set(o.slug.clone());
                                                                    f_title.set(o.title.clone());
                                                                    f_summary.set(o.summary.clone());
                                                                    f_body.set(o.body_md.clone());
                                                                    f_cover.set(o.cover.clone());
                                                                    f_sort.set(o.sort);
                                                                    f_status.set(o.status.clone());
                                                                    message.set("已载入表单，可修改后保存。".to_string());
                                                                }
                                                            >
                                                                "编辑"
                                                            </button>
                                                            <button
                                                                class="btn btn-small btn-danger"
                                                                on:click=move |_| {
                                                                    run_action(
                                                                        revision,
                                                                        message,
                                                                        admin_delete_sky_official(o.id),
                                                                    )
                                                                }
                                                            >
                                                                "删除"
                                                            </button>
                                                        </div>
                                                    </article>
                                                }
                                            })
                                            .collect_view()}
                                    </div>
                                }
                                    .into_any()
                            }
                        }}
                    </Suspense>
                </div>
            </section>

            <section class="panel">
                <div class="panel-head">
                    <span class="step-badge">"2"</span>
                    <div class="panel-head-main">
                        <h2 class="panel-title">"代跑展示"</h2>
                        <p class="panel-desc">
                            "公告显示在光遇首页与代跑页；评价“精选”在「评价」页操作（精选置顶）。"
                        </p>
                    </div>
                    <div class="panel-actions">
                        <button class="btn btn-primary btn-small" on:click=save_announcement>
                            "保存公告"
                        </button>
                    </div>
                </div>
                <div class="panel-body">
                    <label class="field">
                        <span>"公告（留空 = 不显示）"</span>
                        <textarea
                            class="comment-input"
                            rows="4"
                            prop:value=move || announcement.get()
                            on:input=move |ev| announcement.set(event_target_value(&ev))
                        ></textarea>
                    </label>
                </div>
            </section>

            <section class="panel">
                <div class="panel-head">
                    <span class="step-badge">"3"</span>
                    <div class="panel-head-main">
                        <h2 class="panel-title">"光遇投稿"</h2>
                        <p class="panel-desc">"光遇社区的投稿：加精置顶、下架恢复与删除。"</p>
                    </div>
                    <div class="panel-actions">
                        <label class="toggle-row">
                            <input
                                type="checkbox"
                                prop:checked=move || show_hidden.get()
                                on:change=move |ev| show_hidden.set(event_target_checked(&ev))
                            />
                            <span class="toggle-text">"显示已下架"</span>
                        </label>
                        <button class="btn btn-small" on:click=move |_| revision.update(|n| *n += 1)>
                            "刷新"
                        </button>
                    </div>
                </div>
                <div class="panel-body">
                    <Suspense fallback=|| view! { <p class="muted">"载入中…"</p> }>
                        {move || match posts.get() {
                            None => view! { <p class="muted">"载入中…"</p> }.into_any(),
                            Some(Err(e)) => view! {
                                <p class="error">"读取失败："{e.to_string()}</p>
                            }
                                .into_any(),
                            Some(Ok(list)) if list.is_empty() => view! {
                                <p class="admin-empty">"这里还没有投稿。"</p>
                            }
                                .into_any(),
                            Some(Ok(list)) => {
                                let hidden = show_hidden.get();
                                view! {
                                    <div class="admin-list">
                                        {list
                                            .into_iter()
                                            .map(|p| {
                                                let featured = p.featured;
                                                let id = p.id;
                                                let title = p.title.clone();
                                                let author = p.author.clone();
                                                let href = format!(
                                                    "/sky/community/{}/{}",
                                                    p.author_username,
                                                    p.slug,
                                                );
                                                let featured_next = !featured;
                                                view! {
                                                    <article class="admin-row">
                                                        <p class="admin-meta">
                                                            <span class="status">"光遇"</span>
                                                            {featured
                                                                .then(|| {
                                                                    view! {
                                                                        <span class="badge badge-featured">"精选"</span>
                                                                    }
                                                                })}
                                                            <strong>{title}</strong>
                                                            <span class="comment-time">{author}</span>
                                                            <a href=href>"打开"</a>
                                                        </p>
                                                        <div class="admin-actions">
                                                            {(!hidden)
                                                                .then(|| {
                                                                    view! {
                                                                        <button
                                                                            class="btn btn-small"
                                                                            on:click=move |_| {
                                                                                run_action(
                                                                                    revision,
                                                                                    message,
                                                                                    admin_feature_sky_post(id, featured_next),
                                                                                )
                                                                            }
                                                                        >
                                                                            {if featured { "取消精选" } else { "精选" }}
                                                                        </button>
                                                                    }
                                                                })}
                                                            {hidden
                                                                .then(|| {
                                                                    view! {
                                                                        <button
                                                                            class="btn btn-small"
                                                                            on:click=move |_| {
                                                                                run_action(
                                                                                    revision,
                                                                                    message,
                                                                                    admin_set_community_status(id, "published".into()),
                                                                                )
                                                                            }
                                                                        >
                                                                            "恢复"
                                                                        </button>
                                                                    }
                                                                })}
                                                            {(!hidden)
                                                                .then(|| {
                                                                    view! {
                                                                        <button
                                                                            class="btn btn-small"
                                                                            on:click=move |_| {
                                                                                run_action(
                                                                                    revision,
                                                                                    message,
                                                                                    admin_set_community_status(id, "hidden".into()),
                                                                                )
                                                                            }
                                                                        >
                                                                            "下架"
                                                                        </button>
                                                                    }
                                                                })}
                                                            <button
                                                                class="btn btn-small btn-danger"
                                                                on:click=move |_| {
                                                                    run_action(
                                                                        revision,
                                                                        message,
                                                                        admin_delete_community(id),
                                                                    )
                                                                }
                                                            >
                                                                "删除"
                                                            </button>
                                                        </div>
                                                    </article>
                                                }
                                            })
                                            .collect_view()}
                                    </div>
                                }
                                    .into_any()
                            }
                        }}
                    </Suspense>
                </div>
            </section>
        </AdminPage>
    }
}


/// 后台：服务订单（时间池充值 / 容量池 / 外置云存储）。
#[component]
pub fn AdminOrdersPage() -> impl IntoView {
    let revision = RwSignal::new(0_u32);
    let message = RwSignal::new(String::new());
    let orders = Resource::new(move || revision.get(), |_| admin_orders());

    view! {
        <Title text="服务订单 — Wraindrock" />
        <AdminPage
            title="服务订单"
            lede="人工通道在此确认收款；Creem 在线支付由回调自动确认。".to_string()
            perm="admin"
        >
            {move || {
                let text = message.get();
                (!text.is_empty()).then(|| view! { <p class="notice" role="status">{text}</p> })
            }}
            <Suspense fallback=move || view! { <p class="muted">"载入中…"</p> }>
                {move || match orders.get() {
                    None => view! { <p class="muted">"载入中…"</p> }.into_any(),
                    Some(Err(error)) => view! {
                        <p class="error">"载入订单失败："{error.to_string()}</p>
                    }
                    .into_any(),
                    Some(Ok(list)) if list.is_empty() => view! {
                        <p class="muted">"还没有订单。"</p>
                    }
                    .into_any(),
                    Some(Ok(list)) => view! {
                        <div class="panel">
                            <div class="panel-body">
                                <table>
                                    <thead>
                                        <tr>
                                            <th>"订单"</th>
                                            <th>"内容"</th>
                                            <th>"金额"</th>
                                            <th>"通道"</th>
                                            <th>"状态"</th>
                                            <th>"时间"</th>
                                            <th>"操作"</th>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        {list
                                            .into_iter()
                                            .map(|order| {
                                                let order_id = order.id;
                                                let status = order.status.clone();
                                                let is_pending = status == "pending";
                                                view! {
                                                    <tr>
                                                        <td>{format!("#{}", order.id)}</td>
                                                        <td>
                                                            {product_label(&order.product, &order.option)}
                                                        </td>
                                                        <td>{format_cents(order.amount_cents)}</td>
                                                        <td>{order.provider.clone()}</td>
                                                        <td>{order_status_label(&order.status)}</td>
                                                        <td>{order.created_at.clone()}</td>
                                                        <td>
                                                            {is_pending
                                                                .then(|| {
                                                                    view! {
                                                                        <div class="admin-actions">
                                                                            <button
                                                                                class="btn btn-small"
                                                                                type="button"
                                                                                on:click=move |_| {
                                                                                    run_action(
                                                                                        revision,
                                                                                        message,
                                                                                        admin_confirm_order(order_id),
                                                                                    );
                                                                                }
                                                                            >
                                                                                "确认收款"
                                                                            </button>
                                                                            <button
                                                                                class="btn btn-small"
                                                                                type="button"
                                                                                on:click=move |_| {
                                                                                    run_action(
                                                                                        revision,
                                                                                        message,
                                                                                        admin_cancel_order(order_id),
                                                                                    );
                                                                                }
                                                                            >
                                                                                "取消"
                                                                            </button>
                                                                        </div>
                                                                    }
                                                                })}
                                                        </td>
                                                    </tr>
                                                }
                                            })
                                            .collect_view()}
                                    </tbody>
                                </table>
                            </div>
                        </div>
                    }
                    .into_any(),
                }}
            </Suspense>
        </AdminPage>
    }
}
