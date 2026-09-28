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
use crate::components::PageHeader;

use super::set_status;

/// 统一执行一个后台动作：成功就刷新列表并提示，失败把服务端的原话显示出来。
fn run_action<F>(revision: RwSignal<u32>, message: RwSignal<String>, action: F)
where
    F: std::future::Future<Output = Result<Result<(), String>, leptos::prelude::ServerFnError>>
        + 'static,
{
    leptos::task::spawn_local(async move {
        match action.await {
            Ok(Ok(())) => {
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
        <Title text="无权访问 — Felix Homelab" />
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
                        <section class="wrap">
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
        <div class="field-row">
            <button
                class="btn"
                class:active=move || only_pending.get()
                on:click=move |_| only_pending.set(true)
            >
                "只看待审"
            </button>
            <button
                class="btn"
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
        <Title text="后台 — Felix Homelab" />
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
                <div class="field-row">
                    <a class="btn btn-small" href="/admin/comments">"评论审核"</a>
                    <a class="btn btn-small" href="/admin/users">"用户管理"</a>
                    <a class="btn btn-small" href="/admin/pod">"Pod 管理"</a>
                    <a class="btn btn-small" href="/admin/backup">"备份与同步"</a>
                    <a
                        class="btn btn-small"
                        href="http://dash.localhost:5729/"
                        target="_blank"
                        rel="noreferrer"
                    >
                        "控制台看板"
                    </a>
                    <a
                        class="btn btn-small"
                        href="http://localhost:5730/-/admin"
                        target="_blank"
                        rel="noreferrer"
                    >
                        "Forgejo 后台"
                    </a>
                    <a class="btn btn-small" href="/" target="_blank" rel="noreferrer">
                        "打开主站"
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
        <Title text="评论审核 — Felix Homelab" />
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
                        view! { <p class="muted">"没有需要处理的评论。"</p> }.into_any()
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
                class="btn"
                class:active=move || filter.get() == value
                on:click=move |_| filter.set(value.to_string())
            >
                {label}
            </button>
        }
    };

    view! {
        <Title text="社区管理 — Felix Homelab" />
        <AdminPage title="社区管理" lede="发布即公开；这里负责下架、恢复与删除。".to_string() perm="community">
            <div class="field-row">
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
                        view! { <p class="muted">"这个筛选下没有内容。"</p> }.into_any()
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
        <Title text="评价审核 — Felix Homelab" />
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
                        view! { <p class="muted">"没有需要处理的评价。"</p> }.into_any()
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
        <Title text="用户管理 — Felix Homelab" />
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
                <span class="status">
                    {if is_admin { role_label("admin") } else { role_label("user") }}
                </span>
                {scopes
                    .into_iter()
                    .map(|scope| {
                        let value = scope.clone();
                        view! {
                            <span class="status">
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
        <Title text="Pod 管理 — Felix Homelab" />
        <AdminPage
            title="Pod 管理"
            lede="Felix-Homelab 内的容器状态；重启会短暂中断对应服务。".to_string()
            perm="super"
        >
            <p class="notice" role="status">{move || message.get()}</p>

            <div class="field-row">
                <button class="btn btn-small" on:click=move |_| revision.update(|n| *n += 1)>
                    "刷新"
                </button>
                <a
                    class="btn btn-small"
                    href="http://dash.localhost:5729/"
                    target="_blank"
                    rel="noreferrer"
                >
                    "控制台看板"
                </a>
                <a
                    class="btn btn-small"
                    href="http://localhost:5730/-/admin"
                    target="_blank"
                    rel="noreferrer"
                >
                    "Forgejo 后台"
                </a>
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

    view! {
        <article class="admin-row">
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
        <Title text="Agent 管理 — Felix Homelab" />
        <AdminPage
            title="Agent 管理"
            lede="每个授权账号一个独立容器（模板镜像 + 独立数据卷 + 独立子域）。授权/停用后由宿主脚本自动执行。".to_string()
            perm="agent"
        >
            <p class="notice" role="status">{move || message.get()}</p>

            <article class="channel-card">
                <div class="channel-head">
                    <strong>"开通新实例"</strong>
                </div>
                <div class="field-row">
                    <label class="field field-grow">
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
                    <label class="field field-grow">
                        <span>"备注"</span>
                        <input
                            type="text"
                            placeholder="付款记录等（可选）"
                            prop:value=move || note.get()
                            on:input=move |ev| note.set(event_target_value(&ev))
                        />
                    </label>
                    <button class="btn" on:click=submit_grant>"开通"</button>
                </div>
                <p class="muted">
                    "为指定用户新建该类型的 N 个实例（新的随机域名，与现有实例互不影响）。"
                    "0 天 = 长期有效。续费 / 复活已撤销的实例请在下方每一行操作。"
                    "用户登录后在首页能看到自己的入口；DeepSeek Harness 首次进入用卡片上的带令牌链接。"
                </p>
            </article>

            <div class="field-row">
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
                        <p class="muted">"还没有开通任何 Agent。"</p>
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
        <article class="admin-row">
            <p class="admin-meta">
                <span class=status_class>{status_label.to_string()}</span>
                {(!purged)
                    .then(|| {
                        view! { <span class=runtime_class>{runtime_label.clone()}</span> }
                    })}
                {(!purged)
                    .then(|| health.map(|health| view! { <span class=health_class>{health}</span> }))}
                <strong>{agent.username.clone()}</strong>
                <span class="comment-time">"实例 #" {slot}</span>
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

/// 一个备份源的开关行。
#[component]
fn SourceToggle(
    label: &'static str,
    hint: &'static str,
    checked: RwSignal<bool>,
) -> impl IntoView {
    view! {
        <label class="toggle-row">
            <input
                type="checkbox"
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
        <Title text="备份 — Felix Homelab" />
        <AdminPage
            title="备份"
            lede="选择要备份的内容；每个备份源、每个异地渠道都能单独开关。".to_string()
            perm="super"
        >
            <p class="notice" role="status">{move || message.get()}</p>

            <section class="admin-quick">
                <h2 class="admin-section-title">"1. 备份内容"</h2>
                <p class="muted">
                    "每天 03:00 自动备份（关机/休眠错过后开机补跑）。需要哪一项就打开哪一项。"
                </p>
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
                <div class="field-row">
                    <button class="btn btn-primary btn-small" on:click=save>
                        "保存设置"
                    </button>
                    <button class="btn btn-small" on:click=do_backup>
                        "立即备份"
                    </button>
                    <button class="btn btn-small" on:click=move |_| revision.update(|n| *n += 1)>
                        "刷新归档"
                    </button>
                </div>
                <p class="muted">
                    "「立即备份」按已保存的设置执行；刚改过开关的话先点「保存设置」。"
                </p>
            </section>

            <section class="admin-quick">
                <h2 class="admin-section-title">"2. 备份渠道（异地）"</h2>
                <p class="muted">
                    "每个渠道独立开关；可同时开启多个，也可以全部关闭。全部关闭时只保留本机备份。"
                </p>
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
                        .then(|| view! { <p class="muted">"还没有渠道，点下面「添加渠道」。"</p> })
                }}
                <div class="field-row">
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
                    <button class="btn btn-primary btn-small" on:click=save>
                        "保存设置"
                    </button>
                    <button class="btn btn-small" on:click=do_sync>
                        "立即同步"
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
                <div class="field-row">
                    <button
                        class="btn btn-small"
                        on:click=move |_: leptos::ev::MouseEvent| config.refetch()
                    >
                        "刷新状态"
                    </button>
                </div>
            </section>

            <section class="admin-quick">
                <h2 class="admin-section-title">"3. 备份归档（本机）"</h2>
                <p class="muted">
                    "归档保存在宿主机 ~/.local/share/felix-homelab/backups/；按上面保存的保留天数自动清理。"
                </p>
                <Suspense fallback=|| view! { <p class="muted">"载入中…"</p> }>
                    {move || match backups.get() {
                        None => view! { <p class="muted">"载入中…"</p> }.into_any(),
                        Some(Err(e)) => {
                            view! { <p class="error">"读取失败："{e.to_string()}</p> }.into_any()
                        }
                        Some(Ok(list)) if list.is_empty() => {
                            view! { <p class="muted">"还没有备份，点上面「立即备份」。"</p> }
                                .into_any()
                        }
                        Some(Ok(list)) => {
                            view! {
                                <div class="admin-list">
                                    {list
                                        .into_iter()
                                        .map(|f| {
                                            let source = backup_source(&f.name);
                                            view! {
                                                <article class="admin-row">
                                                    <p class="admin-meta">
                                                        <span class="badge">{source}</span>
                                                        <strong>{f.name.clone()}</strong>
                                                        <span class="comment-time">{f.size.clone()}</span>
                                                        <span class="comment-time">{f.time.clone()}</span>
                                                    </p>
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
            </section>

            <section class="admin-quick">
                <h2 class="admin-section-title">"4. 恢复"</h2>
                <p class="muted">
                    "在宿主机执行 make restore（默认恢复最新归档，会先做一次安全备份再覆盖）。"
                    " 各备份源恢复细节见 README「备份与恢复」。"
                </p>
            </section>
        </AdminPage>
    }
}
