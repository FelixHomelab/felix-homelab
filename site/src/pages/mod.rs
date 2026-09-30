//! 各路由的页面组件。

pub mod admin;
pub mod community;
pub mod services;

pub use services::ServicesPage;

use crate::auth::UserState;
use crate::agents::{agent_kind_label, my_agents, AgentRow};
use crate::community::list_community;
use crate::components::comments::CommentSection;
use crate::components::community::CommunityCard;
use crate::components::reviews::ReviewSection;
use crate::components::{
    kind_label, PageHeader, PostCard, ProjectCard, SiteFooter, SiteHeader, ThemeToggle,
};
use crate::content::{
    get_page, get_post, get_project, list_posts, list_posts_by_tag, list_projects, list_sky,
    list_tags, AuthorAccount, SKY_CATEGORIES,
};
use crate::sky::{get_sky_official, list_sky_official, sky_boosting};
use crate::theme::{validate_accent, validate_background, ThemeState};
use leptos::prelude::*;
use leptos_meta::{Meta, Title};
use leptos_router::hooks::{use_navigate, use_params_map, use_query_map};

/// 描述兜底：摘要为空时给一句站点说明，免得出现空的 `meta description`。
fn description_or_default(text: &str) -> String {
    match text.trim() {
        "" => "Wraindrock 社区站：官方文章与项目、社区投稿，以及光遇记录。".to_string(),
        trimmed => trimmed.to_string(),
    }
}

/// 发布者一行。
///
/// 解析到站点账号就链到公开资料页；解析不到（站内没有这个用户名）就只显示名字，
/// **不给链接**——链过去是 404，比不链更差。
///
/// `author` 是 front matter 里写的用户名，`account` 是解析到的账号。链接与显示名
/// 都以 `account` 为准：显示名是用户自己设的昵称，用户名的大小写也以库里的写法
/// 为准（用户名大小写不敏感，见 `migrations/0002`）。
#[component]
fn AuthorLine(author: String, account: Option<AuthorAccount>) -> impl IntoView {
    match account {
        Some(account) => {
            let href = format!("/user/{}", account.username);
            let display_name = account.display_name;
            view! {
                <span class="author">
                    "发布者：" <a href=href>{display_name}</a>
                </span>
            }
            .into_any()
        }
        None => view! { <span class="author">"发布者：" {author}</span> }.into_any(),
    }
}

/// 列表加载中的占位。
fn loading() -> impl IntoView {
    view! { <p class="muted">"载入中…"</p> }
}

/// 让这一页带上指定的 HTTP 状态码。
///
/// 只在服务端渲染时真正生效——浏览器端水合时状态码早就发出去了。因此它只用于
/// 「这一页本身就不该被看到」的场景，不承担任何授权职责。
pub(crate) fn set_status(code: u16) {
    #[cfg(feature = "ssr")]
    if let Some(options) = use_context::<leptos_axum::ResponseOptions>() {
        if let Ok(status) = axum::http::StatusCode::from_u16(code) {
            options.set_status(status);
        }
    }
    #[cfg(not(feature = "ssr"))]
    let _ = code;
}

/// 列表加载失败的提示。带上原始错误，方便自助排查。
fn load_error(err: String) -> impl IntoView {
    view! { <p class="error">"载入失败：" {err}</p> }
}

/// 取路由参数里的 slug。
fn route_param(name: &'static str) -> impl Fn() -> String + Clone + Send + Sync {
    let params = use_params_map();
    move || params.get().get(name).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// 首页
// ---------------------------------------------------------------------------

/// 首页：站点定位 + 官方文章 + 社区投稿 + 代表项目。
#[component]
pub fn HomePage() -> impl IntoView {
    let posts = Resource::new(|| (), |_| list_posts());
    let community = Resource::new(|| (), |_| list_community(None));
    let projects = Resource::new(|| (), |_| list_projects());

    view! {
        <Title text="Wraindrock" />
        <Meta
            name="description"
            content="Wraindrock 社区站：官方文章与项目、社区投稿，以及光遇记录。"
        />
        <section class="hero wrap">
            <p class="eyebrow">"Wraindrock"</p>
            <h1 class="hero-title">"自托管，也把过程写下来。"</h1>
            <p class="lede">
                "官方内容由管理员维护，社区内容由注册用户投稿——都跑在自己的服务器上。"
            </p>
            <div class="hero-actions">
                <a class="btn btn-primary" href="/community">"逛社区"</a>
                <a class="btn" href="/blog">"读官方博客"</a>
                <a class="btn" href="/projects">"看项目"</a>
            </div>
        </section>

        <MyAgentsSection />

        <section class="wrap section">
            <div class="section-head">
                <h2>"官方文章"</h2>
                <a class="more" href="/blog">"全部 →"</a>
            </div>
            <Suspense fallback=loading>
                {move || posts.get().map(|res| match res {
                    Ok(list) if list.is_empty() => view! { <p class="muted">"还没有文章。"</p> }.into_any(),
                    Ok(list) => list.into_iter().take(3).map(|p| view! { <PostCard post=p /> }).collect_view().into_any(),
                    Err(e) => load_error(e.to_string()).into_any(),
                })}
            </Suspense>
        </section>

        <section class="wrap section">
            <div class="section-head">
                <h2>"社区投稿"</h2>
                <a class="more" href="/community">"全部 →"</a>
            </div>
            <Suspense fallback=loading>
                {move || community.get().map(|res| match res {
                    Ok(list) if list.is_empty() => view! { <p class="muted">"还没有社区内容。"</p> }.into_any(),
                    Ok(list) => list.into_iter().take(3).map(|item| view! { <CommunityCard item=item /> }).collect_view().into_any(),
                    Err(e) => load_error(e.to_string()).into_any(),
                })}
            </Suspense>
        </section>

        <section class="wrap section">
            <div class="section-head">
                <h2>"代表项目"</h2>
                <a class="more" href="/projects">"全部 →"</a>
            </div>
            <Suspense fallback=loading>
                {move || projects.get().map(|res| match res {
                    Ok(list) if list.is_empty() => view! { <p class="muted">"还没有项目。"</p> }.into_any(),
                    Ok(list) => list.into_iter().take(3).map(|p| view! { <ProjectCard project=p /> }).collect_view().into_any(),
                    Err(e) => load_error(e.to_string()).into_any(),
                })}
            </Suspense>
        </section>
    }
}

// ---------------------------------------------------------------------------
// 我的 Agent（登录用户首屏入口）
// ---------------------------------------------------------------------------

/// 登录用户首页的 Agent 入口：管理员开通后自动出现，未开通/未登录不渲染。
///
/// 数据依赖会话 Cookie，SSR 与客户端水合的资源就绪时机不一致会让 DOM 错位
/// （tachys hydration 失配 → 整站 wasm 水合崩溃，站内跳转失灵、内容不显示）。
/// 因此这里**服务端输出空，水合完成后才在客户端渲染**：
/// SSR 与客户端首帧都是空，天然一致；随后 effect 触发正常更新。
#[component]
fn MyAgentsSection() -> impl IntoView {
    let agents = Resource::new(|| (), |_| my_agents());
    let mounted = RwSignal::new(false);
    // effect 只在客户端运行，且在水合完成后执行
    Effect::new(move |_| mounted.set(true));

    move || {
        if !mounted.get() {
            return None;
        }
        let list = match agents.get().and_then(|result| result.ok()) {
            Some(list) if !list.is_empty() => list,
            // 未登录 / 未开通：给一句提示，避免整块消失让人以为坏了
            _ => {
                return Some(
                    view! {
                        <section class="wrap section">
                            <div class="section-head">
                                <h2>"我的 Agent"</h2>
                                <span class="muted">"登录主站后显示；管理员开通后自动出现在这里"</span>
                            </div>
                        </section>
                    }
                    .into_any(),
                );
            }
        };
        Some(
            view! {
                <section class="wrap section">
                    <div class="section-head">
                        <h2>"我的 Agent"</h2>
                        <span class="muted">"打开卡片即进入；DSH 会自动完成登录"</span>
                    </div>
                    <div class="agent-grid">
                        {list
                            .into_iter()
                            .map(|agent| view! { <MyAgentCard agent=agent /> })
                            .collect_view()}
                    </div>
                </section>
            }
            .into_any(),
        )
    }
}

/// 首屏上的一个 Agent 入口卡片。
#[component]
fn MyAgentCard(agent: AgentRow) -> impl IntoView {
    let status_label = match agent.status.as_str() {
        "active" => "已开通",
        "stopped" => "已暂停",
        other => other,
    };
    let status_class = match agent.status.as_str() {
        "active" => "status status-running",
        "stopped" => "status status-exited",
        _ => "status",
    };
    let runtime_hint = match agent
        .runtime
        .as_ref()
        .map(|r| (r.state.as_str(), r.desired.as_str()))
    {
        Some(("running", _)) => "运行中",
        Some((_, "sleeping")) => "睡眠中（打开即唤醒）",
        Some(("exited", _)) => "已停止",
        Some(_) => "准备中",
        None => "部署中",
    };
    let expires = agent
        .expires_at
        .clone()
        .map(|value| format!("有效期至 {value}"))
        .unwrap_or_else(|| "长期有效".to_string());
    let note = agent.note.trim().to_string();
    let is_dsh = agent.kind == "dsh";
    // DSH 首次进入需要带令牌链接；OpenCode 直接打开即可
    let href = agent.login_url.clone().unwrap_or_else(|| agent.url.clone());

    view! {
        <a class="agent-card" href=href target="_blank" rel="noreferrer">
            <div class="agent-card-head">
                <strong>{agent_kind_label(&agent.kind)}</strong>
                <span class=status_class>{status_label.to_string()}</span>
            </div>
            <p class="agent-card-url">{agent.url.clone()}</p>
            <p class="muted">
                {agent.subdomain.clone()} " · " {runtime_hint} " · " {expires}
            </p>
            {is_dsh
                .then(|| {
                    view! {
                        <p class="muted">
                            "首次进入请点此卡片（自动完成登录）；模型/服务商在站内「设置」里配置。"
                        </p>
                    }
                })}
            {(!note.is_empty())
                .then(|| view! { <p class="muted">{note.clone()}</p> })}
        </a>
    }
}

/// 博客列表。
#[component]
pub fn BlogIndex() -> impl IntoView {
    let posts = Resource::new(|| (), |_| list_posts());
    let tags = Resource::new(|| (), |_| list_tags());
    let projects = Resource::new(|| (), |_| list_projects());

    view! {
        <Title text="博客 — Wraindrock" />
        <Meta name="description" content="官方博客：文章、笔记与项目。社区内容在「社区」。" />
        <section class="wrap">
            <PageHeader
                title="博客"
                lede="官方内容：写下来的才算想过。社区投稿在「社区」。".to_string()
            />

            <Suspense fallback=loading>
                {move || tags.get().map(|res| match res {
                    Ok(list) if list.is_empty() => ().into_any(),
                    Ok(list) => view! {
                        <ul class="tag-row tag-row-lg">
                            {list.into_iter().map(|(tag, count)| {
                                let href = format!("/blog/tag/{tag}");
                                view! { <li><a class="tag" href=href>{tag}" · "{count}</a></li> }
                            }).collect_view()}
                        </ul>
                    }.into_any(),
                    Err(_) => ().into_any(),
                })}
            </Suspense>

            <Suspense fallback=loading>
                {move || posts.get().map(|res| match res {
                    Ok(list) if list.is_empty() => view! { <p class="muted">"还没有文章。"</p> }.into_any(),
                    Ok(list) => view! {
                        <div class="card-list">
                            {list.into_iter().map(|p| view! { <PostCard post=p /> }).collect_view()}
                        </div>
                    }.into_any(),
                    Err(e) => load_error(e.to_string()).into_any(),
                })}
            </Suspense>

            // 官方项目并入博客：官方内容只有一个入口，项目在这里沉底展示。
            <Suspense fallback=loading>
                {move || projects.get().map(|res| match res {
                    Ok(list) if list.is_empty() => ().into_any(),
                    Ok(list) => view! {
                        <section class="section">
                            <div class="section-head">
                                <h2>"官方项目"</h2>
                                <a href="/projects">"全部项目 →"</a>
                            </div>
                            <div class="card-list">
                                {list.into_iter().map(|p| view! { <ProjectCard project=p /> }).collect_view()}
                            </div>
                        </section>
                    }.into_any(),
                    Err(_) => ().into_any(),
                })}
            </Suspense>
        </section>
    }
}

/// 单篇文章。
#[component]
pub fn BlogPost() -> impl IntoView {
    // 在组件体里就把 ResponseOptions 取出来——等资源返回后再找 owner 未必还拿得到。
    #[cfg(feature = "ssr")]
    let response_options = use_context::<leptos_axum::ResponseOptions>();

    let slug = route_param("slug");
    let post = Resource::new(slug, |slug| get_post(slug));

    view! {
        <section class="wrap">
            <Suspense fallback=loading>
                {move || post.get().map(|res| match res {
                    Ok(Some(detail)) => {
                        // 先把 slug 取出来，下面 detail.html 会被移进 inner_html
                        let comment_slug = detail.summary.slug.clone();
                        view! {
                            <article class="article">
                                <Title text=format!("{} — Wraindrock", detail.summary.title) />
                                <Meta
                                    name="description"
                                    content=description_or_default(&detail.summary.summary)
                                />
                                <Meta property="og:type" content="article" />
                                <Meta property="og:title" content=detail.summary.title.clone() />
                                <Meta
                                    property="og:description"
                                    content=description_or_default(&detail.summary.summary)
                                />
                                <header class="page-header">
                                    <h1>{detail.summary.title.clone()}</h1>
                                    <p class="card-meta">
                                        {detail.summary.date.clone()}
                                        " · "{detail.summary.reading_minutes}" 分钟"
                                        " · "
                                        <AuthorLine
                                            author=detail.summary.author.clone()
                                            account=detail.author_account.clone()
                                        />
                                    </p>
                                    <ul class="tag-row">
                                        {detail.summary.tags.clone().into_iter().map(|tag| {
                                            let href = format!("/blog/tag/{tag}");
                                            view! { <li><a class="tag" href=href>{tag}</a></li> }
                                        }).collect_view()}
                                    </ul>
                                </header>
                                <div class="prose" inner_html=detail.html></div>
                            </article>
                            <CommentSection target_kind="post" target_slug=comment_slug />
                        }
                        .into_any()
                    }
                    Ok(None) => {
                        #[cfg(feature = "ssr")]
                        if let Some(options) = &response_options {
                            options.set_status(axum::http::StatusCode::NOT_FOUND);
                        }
                        view! {
                            <Title text="找不到文章 — Wraindrock" />
                            <PageHeader title="找不到这篇文章" lede="它可能被改名或删掉了。".to_string() />
                            <p><a href="/blog">"← 回博客列表"</a></p>
                        }.into_any()
                    },
                    Err(e) => load_error(e.to_string()).into_any(),
                })}
            </Suspense>
        </section>
    }
}

/// 按标签筛选的文章列表。
#[component]
pub fn BlogTag() -> impl IntoView {
    let tag = route_param("tag");
    let posts = {
        let tag = tag.clone();
        Resource::new(tag, |tag| list_posts_by_tag(tag))
    };

    view! {
        <section class="wrap">
            <Title text="标签 — Wraindrock" />
            <PageHeader title="按标签浏览" />
            <p class="lede">"标签：" <strong>{tag}</strong></p>

            <Suspense fallback=loading>
                {move || posts.get().map(|res| match res {
                    Ok(list) if list.is_empty() => view! { <p class="muted">"这个标签下还没有文章。"</p> }.into_any(),
                    Ok(list) => view! {
                        <div class="card-list">
                            {list.into_iter().map(|p| view! { <PostCard post=p /> }).collect_view()}
                        </div>
                    }.into_any(),
                    Err(e) => load_error(e.to_string()).into_any(),
                })}
            </Suspense>
            <p class="back"><a href="/blog">"← 回博客列表"</a></p>
        </section>
    }
}

// ---------------------------------------------------------------------------
// 项目
// ---------------------------------------------------------------------------

/// 项目列表，按 `kind` 分组展示。
#[component]
pub fn ProjectIndex() -> impl IntoView {
    let projects = Resource::new(|| (), |_| list_projects());

    view! {
        <Title text="项目 — Wraindrock" />
        <Meta name="description" content="我做过的东西：开源、私有与团队项目。" />
        <section class="wrap">
            <PageHeader title="项目" lede="开源、私有与团队项目都记在这里。".to_string() />
            <Suspense fallback=loading>
                {move || projects.get().map(|res| match res {
                    Ok(list) if list.is_empty() => view! { <p class="muted">"还没有项目。"</p> }.into_any(),
                    Ok(list) => ["open", "private", "team"].into_iter().filter_map(|kind| {
                        let group: Vec<_> = list.iter().filter(|p| p.kind == kind).cloned().collect();
                        if group.is_empty() {
                            return None;
                        }
                        let label = kind_label(kind);
                        Some(view! {
                            <div class="group">
                                <h2>{label}</h2>
                                <div class="card-list">
                                    {group.into_iter().map(|p| view! { <ProjectCard project=p /> }).collect_view()}
                                </div>
                            </div>
                        })
                    }).collect_view().into_any(),
                    Err(e) => load_error(e.to_string()).into_any(),
                })}
            </Suspense>
        </section>
    }
}

/// 单个项目。
#[component]
pub fn ProjectShow() -> impl IntoView {
    #[cfg(feature = "ssr")]
    let response_options = use_context::<leptos_axum::ResponseOptions>();

    let slug = route_param("slug");
    let project = Resource::new(slug, |slug| get_project(slug));

    view! {
        <section class="wrap">
            <Suspense fallback=loading>
                {move || project.get().map(|res| match res {
                    Ok(Some(detail)) => {
                        let kind = kind_label(&detail.summary.kind).to_string();
                        view! {
                            <article class="article">
                                <Title text=format!("{} — Wraindrock", detail.summary.name) />
                                <Meta
                                    name="description"
                                    content=description_or_default(&detail.summary.summary)
                                />
                                <Meta property="og:type" content="website" />
                                <Meta property="og:title" content=detail.summary.name.clone() />
                                <Meta
                                    property="og:description"
                                    content=description_or_default(&detail.summary.summary)
                                />
                                <header class="page-header">
                                    <h1>{detail.summary.name.clone()}</h1>
                                    <p class="card-meta">
                                        <span class="badge">{kind}</span>
                                        {detail.summary.stack.join(" · ")}
                                    </p>
                                    <p class="lede">{detail.summary.summary.clone()}</p>
                                    <p class="card-meta">
                                        <AuthorLine
                                            author=detail.summary.author.clone()
                                            account=detail.author_account.clone()
                                        />
                                    </p>
                                    <div class="hero-actions">
                                        {detail.summary.repo.clone().map(|url| view! {
                                            <a class="btn" href=url target="_blank" rel="noopener">"代码仓库"</a>
                                        })}
                                        {detail.summary.demo.clone().map(|url| view! {
                                            <a class="btn btn-primary" href=url target="_blank" rel="noopener">"在线访问"</a>
                                        })}
                                    </div>
                                </header>
                                <div class="prose" inner_html=detail.html></div>
                            </article>
                        }.into_any()
                    }
                    Ok(None) => {
                        #[cfg(feature = "ssr")]
                        if let Some(options) = &response_options {
                            options.set_status(axum::http::StatusCode::NOT_FOUND);
                        }
                        view! {
                            <Title text="找不到项目 — Wraindrock" />
                            <PageHeader title="找不到这个项目" lede="它可能被改名或删掉了。".to_string() />
                            <p><a href="/projects">"← 回项目列表"</a></p>
                        }.into_any()
                    }
                    Err(e) => load_error(e.to_string()).into_any(),
                })}
            </Suspense>
        </section>
    }
}

// ---------------------------------------------------------------------------
// 其余页面
// ---------------------------------------------------------------------------

/// 由 `content/pages/<slug>.md` 驱动的静态页。
///
/// 文件不存在时给出**明确的提示而不是 404**：「关于」这类固定入口不该因为少一个
/// 文件就整页打不开，而且提示里直接写了该往哪放文件。
#[component]
fn StaticPage(
    slug: &'static str,
    #[prop(into)] default_title: String,
    #[prop(into)] description: String,
) -> impl IntoView {
    let page = Resource::new_blocking(move || slug.to_string(), |slug| get_page(slug));

    view! {
        <Suspense fallback=loading>
            {move || match page.get() {
                None => loading().into_any(),
                Some(Err(error)) => load_error(error.to_string()).into_any(),
                Some(Ok(None)) => view! {
                    <section class="wrap">
                        <Title text=format!("{default_title} — Wraindrock") />
                        <Meta name="description" content=description.clone() />
                        <PageHeader title=default_title.clone() />
                        <p class="muted">
                            {format!("这一页还没写。在 content/pages/{slug}.md 里添加内容即可。")}
                        </p>
                    </section>
                }
                .into_any(),
                Some(Ok(Some(page))) => view! {
                    <section class="wrap">
                        <Title text=format!("{} — Wraindrock", page.title) />
                        <Meta name="description" content=description.clone() />
                        <PageHeader title=page.title.clone() />
                        <div class="prose" inner_html=page.html></div>
                        {(!page.updated.is_empty())
                            .then(|| {
                                view! {
                                    <p class="muted page-updated">
                                        {format!("最后更新于 {}", page.updated)}
                                    </p>
                                }
                            })}
                    </section>
                }
                .into_any(),
            }}
        </Suspense>
    }
}

/// 关于页。内容在 `content/pages/about.md`。
#[component]
pub fn AboutPage() -> impl IntoView {
    view! {
        <StaticPage
            slug="about"
            default_title="关于"
            description="关于我，以及这个站在做什么。"
        />
    }
}

/// 光遇子站首页。
///
/// 攻略与画廊页要等内容进 `content/sky/` 后再开，现在先只放已经能用的代跑入口，
/// 不放死链。
#[component]
pub fn SkyIndex() -> impl IntoView {
    let boosting = Resource::new(|| (), |_| sky_boosting());

    view! {
        <Title text="光遇 — Wraindrock" />
        <Meta name="description" content="光遇的攻略、画廊、社区与代跑服务。" />
        <section class="wrap">
            <PageHeader
                title="光遇"
                lede="Sky: Children of the Light —— 在云端飞翔，与光相遇。".to_string()
            />
            <div class="prose">
                <p>"攻略、截图、玩家社区，以及代跑服务的说明与评价。"</p>
            </div>
            <Suspense fallback=|| ()>
                {move || boosting.get().map(|res| match res {
                    Ok(b) if !b.announcement.trim().is_empty() => {
                        view! {
                            <div class="sky-announce">{b.announcement.clone()}</div>
                        }
                            .into_any()
                    }
                    _ => ().into_any(),
                })}
            </Suspense>
            <div class="hero-actions">
                <a class="btn btn-primary" href="/sky/gameplay">"攻略"</a>
                <a class="btn" href="/sky/gallery">"画廊"</a>
                <a class="btn" href="/sky/community">"光遇社区"</a>
                <a class="btn" href="/sky/boosting">"代跑与评价"</a>
            </div>
        </section>
    }
}

/// 光遇代跑服务页：说明 + 评价区。
#[component]
pub fn SkyBoostingPage() -> impl IntoView {
    let boosting = Resource::new(|| (), |_| sky_boosting());

    view! {
        <Title text="光遇代跑 — Wraindrock" />
        <Meta name="description" content="光遇代跑服务说明与用户评价。" />
        <section class="wrap">
            <PageHeader title="光遇代跑" lede="跑图、任务与献祭，按你方便的时段来。".to_string() />
            <Suspense fallback=|| ()>
                {move || boosting.get().map(|res| match res {
                    Ok(b) if !b.announcement.trim().is_empty() => {
                        view! {
                            <div class="sky-announce">{b.announcement.clone()}</div>
                        }
                            .into_any()
                    }
                    _ => ().into_any(),
                })}
            </Suspense>
            <div class="prose">
                <p>"服务的具体说明与价格稍后补上。"</p>
                <p>
                    "下方是大家的评价（精选置顶）。评价提交后需要审核，通过之后才会显示在这里——"
                    "开放注册的站点不这么做，很快就会被灌水淹没。"
                </p>
            </div>
            <ReviewSection />
            <p class="back"><a href="/sky">"← 回光遇"</a></p>
        </section>
    }
}

/// 光遇官方内容详情（站内编辑）：`/sky/:category/:slug`。
#[component]
pub fn SkyOfficialPage() -> impl IntoView {
    #[cfg(feature = "ssr")]
    let response_options = use_context::<leptos_axum::ResponseOptions>();

    let category = route_param("category");
    let slug = route_param("slug");
    let page = Resource::new_blocking(
        move || (category(), slug()),
        |(category, slug)| get_sky_official(category, slug),
    );

    view! {
        <section class="wrap">
            <Suspense fallback=loading>
                {move || page.get().map(|res| match res {
                    Ok(Some(detail)) => {
                        let label = if detail.item.category == "gallery" { "画廊" } else { "攻略" };
                        let back = format!("/sky/{}", detail.item.category);
                        view! {
                            <article class="article">
                                <Title
                                    text=format!("{} — 光遇 — Wraindrock", detail.item.title)
                                />
                                <Meta
                                    name="description"
                                    content=description_or_default(&detail.item.summary)
                                />
                                <header class="page-header">
                                    <h1>{detail.item.title.clone()}</h1>
                                    <p class="card-meta">
                                        <span class="badge">{label}</span>
                                        " · "{detail.item.updated_at.clone()}
                                    </p>
                                    {(!detail.item.summary.is_empty())
                                        .then(|| {
                                            view! { <p class="lede">{detail.item.summary.clone()}</p> }
                                        })}
                                </header>
                                {(!detail.item.cover.is_empty())
                                    .then(|| {
                                        view! {
                                            <img
                                                class="sky-cover"
                                                src=detail.item.cover.clone()
                                                alt=detail.item.title.clone()
                                            />
                                        }
                                    })}
                                <div class="prose" inner_html=detail.body_html></div>
                            </article>
                            <p class="back">
                                <a href=back>"← 回" {label}</a>
                            </p>
                        }
                            .into_any()
                    }
                    Ok(None) => {
                        #[cfg(feature = "ssr")]
                        if let Some(options) = &response_options {
                            options.set_status(axum::http::StatusCode::NOT_FOUND);
                        }
                        view! {
                            <Title text="找不到内容 — Wraindrock" />
                            <PageHeader
                                title="找不到这篇内容"
                                lede="它可能被撤下或删掉了。".to_string()
                            />
                            <p><a href="/sky">"← 回光遇"</a></p>
                        }
                            .into_any()
                    }
                    Err(e) => load_error(e.to_string()).into_any(),
                })}
            </Suspense>
        </section>
    }
}

/// 把上传路由回传的结果码翻译成给用户看的话。
///
/// 上传走的是普通 Axum 路由（无 JS 也能用），结果只能靠重定向的查询参数带回来。
fn upload_message(code: &str) -> &'static str {
    match code {
        "ok" => "背景图已更新。",
        "cleared" => "背景图已清除。",
        "auth" => "请先登录再上传图片。",
        "size" => "图片太大，请控制在 2 MB 以内。",
        "type" => "只支持 PNG、JPEG、GIF 与 WebP。",
        "empty" => "没有选到文件。",
        // io 以及任何没预料的取值都归到这里
        _ => "上传失败，请稍后再试。",
    }
}

/// 资料表单：昵称与简介。
///
/// 单独做成组件，是为了让初始值随「当前用户」变化自然重置——保存成功后
/// `user_state.refetch()` 会重建这个组件，输入框随即显示保存后的值。
#[component]
fn ProfileForm(display_name: String, bio: String, user_state: UserState) -> impl IntoView {
    // 初值直接写进 HTML（input 的 value 属性 / textarea 的子文本），而不是绑成
    // 响应式：一来服务端渲染的 HTML 里就能看到已有内容（`prop:value` 只设 JS 属性，
    // SSR 输出里是空的），二来打字时不会有东西跟光标抢。保存成功后组件因
    // refetch 重建，初值随之更新。
    let name_input = RwSignal::new(display_name.clone());
    let bio_input = RwSignal::new(bio.clone());
    let message = RwSignal::new(String::new());
    let busy = RwSignal::new(false);

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let name = name_input.get_untracked();
        let text = bio_input.get_untracked();

        if name.trim().is_empty() {
            message.set("昵称不能为空。".to_string());
            return;
        }

        message.set(String::new());
        busy.set(true);
        // 闭包内克隆：on_submit 会被多次调用，不能把 user_state 移出去
        let user_state = user_state.clone();
        leptos::task::spawn_local(async move {
            match crate::auth::update_profile(name, text).await {
                Ok(Ok(())) => {
                    message.set("已保存。".to_string());
                    // 顶栏显示的就是昵称，重新拉一次让它跟着变，不必整页刷新
                    user_state.refetch();
                }
                Ok(Err(text)) => message.set(text),
                Err(error) => message.set(format!("请求失败：{error}")),
            }
            busy.set(false);
        });
    };

    view! {
        <form class="auth-form" on:submit=on_submit>
            <label class="field">
                <span>"昵称"</span>
                <input
                    type="text"
                    value=display_name
                    on:input=move |ev| name_input.set(event_target_value(&ev))
                />
                <small class="muted">
                    "显示在顶栏与你的评论上，长度不限。"
                </small>
            </label>
            <label class="field">
                <span>"简介"</span>
                <textarea
                    class="comment-input"
                    rows="3"
                    on:input=move |ev| bio_input.set(event_target_value(&ev))
                >{bio}</textarea>
                <small class="muted">
                    {format!("最多 {} 个字符，显示在你的公开资料页上。", crate::auth::BIO_MAX)}
                </small>
            </label>
            {move || {
                let text = message.get();
                (!text.is_empty()).then(|| view! { <p class="notice" role="status">{text}</p> })
            }}
            <button class="btn btn-primary" type="submit" disabled=move || busy.get()>
                {move || if busy.get() { "保存中…" } else { "保存资料" }}
            </button>
        </form>
    }
}

/// 个人设置：外观（主题、主色、背景图）与账号安全（改密码）。
///
/// 未登录访客的偏好只存在自己的浏览器 cookie 里；登录用户的偏好会同步进账号，
/// 并在下次登录时从账号镜像回 cookie（账号是真相来源，cookie 是它的缓存）。
/// 输入框只改本地状态，点「应用」时才校验并落盘——半截输入不该把界面弄坏。
#[component]
pub fn AppearancePage() -> impl IntoView {
    let theme = use_context::<ThemeState>().expect("ThemeState 应由 App 提供");
    let user_state = use_context::<UserState>().expect("UserState 应由 App 提供");

    // 初值另存一份用于渲染：`prop:value` 只设 JS 属性、不写进服务端 HTML，
    // 结果用户明明设过主色与背景图，首屏看到的却是空框。
    let initial_accent = theme
        .get()
        .accent
        .clone()
        .unwrap_or_else(|| "4f46e5".to_string());
    let initial_background = theme.get().background.clone().unwrap_or_default();
    let accent_input = RwSignal::new(initial_accent.clone());
    let background_input = RwSignal::new(initial_background.clone());
    let notice = RwSignal::new(String::new());

    // 上传结果由 /api/me/background 通过 ?e=… 回传
    let upload_notice = use_query_map()
        .get()
        .get("e")
        .map(|code| upload_message(&code));

    let apply_accent = move |_| match validate_accent(&accent_input.get_untracked()) {
        Some(hex) => {
            theme.update(|prefs| prefs.accent = Some(hex));
            notice.set("主色已应用。".to_string());
        }
        None => notice.set("主色必须是六位十六进制，例如 4f46e5。".to_string()),
    };

    let reset_accent = move |_| {
        theme.update(|prefs| prefs.accent = None);
        accent_input.set("4f46e5".to_string());
        notice.set("已恢复默认主色。".to_string());
    };

    let apply_background = move |_| {
        match validate_background(&background_input.get_untracked()) {
            Some(url) => {
                theme.update(|prefs| prefs.background = Some(url));
                notice.set("背景图已应用。".to_string());
            }
            None => notice.set(
                "背景图只能是 http(s) 链接或站内相对路径（以 / 开头），且不含引号等字符。"
                    .to_string(),
            ),
        }
    };

    let clear_background = move |_| {
        theme.update(|prefs| prefs.background = None);
        background_input.set(String::new());
        notice.set("已清除背景图。".to_string());
    };

    // --- 改密码 ---
    let old_password = RwSignal::new(String::new());
    let new_password = RwSignal::new(String::new());
    let password_message = RwSignal::new(String::new());
    let password_busy = RwSignal::new(false);

    let on_change_password = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let old = old_password.get_untracked();
        let new = new_password.get_untracked();

        if old.is_empty() || new.is_empty() {
            password_message.set("请填写当前密码与新密码。".to_string());
            return;
        }

        password_message.set(String::new());
        password_busy.set(true);
        leptos::task::spawn_local(async move {
            match crate::auth::change_password(old, new).await {
                Ok(Ok(())) => {
                    password_message
                        .set("密码已更新，其它设备上的登录已失效。".to_string());
                    old_password.set(String::new());
                    new_password.set(String::new());
                }
                Ok(Err(text)) => password_message.set(text),
                Err(error) => password_message.set(format!("请求失败：{error}")),
            }
            password_busy.set(false);
        });
    };

    // 下面两个块各要一个闭包，而 UserState 不是 Copy，各克隆一份。
    // 另外它们都必须包在 <Suspense> 里：资源读在 Suspense 之外不会被推迟到
    // 数据就绪，会变成竞态——同一份代码有时渲染、有时不渲染。
    let user_state_for_upload = user_state.clone();
    let user_state_for_password = user_state.clone();
    let user_state_for_profile = user_state.clone();
    let user_state_for_account = user_state.clone();

    view! {
        <Title text="个人设置 — Wraindrock" />
        <section class="wrap">
            <PageHeader
                title="个人设置"
                lede="偏好存在你自己的浏览器里；登录后还会同步到账号。".to_string()
            />

            {upload_notice.map(|text| view! { <p class="notice" role="status">{text}</p> })}

            <div class="settings">
                <section class="setting">
                    <h2>"主题"</h2>
                    <p class="muted">"跟随系统会随操作系统的亮暗设置自动切换。"</p>
                    <ThemeToggle />
                </section>

                <section class="setting">
                    <h2>"主色"</h2>
                    <p class="muted">
                        "用于链接、按钮与强调元素。按钮上的文字颜色会按你选的主色自动配深浅。"
                    </p>
                    <div class="field-row">
                        <input
                            type="color"
                            class="color-input"
                            value=format!("#{initial_accent}")
                            on:input=move |ev| {
                                accent_input.set(event_target_value(&ev).trim_start_matches('#').to_string());
                            }
                        />
                        <input
                            type="text"
                            class="text-input"
                            spellcheck="false"
                            value=initial_accent
                            on:input=move |ev| accent_input.set(event_target_value(&ev))
                        />
                        <button class="btn btn-primary" on:click=apply_accent>"应用"</button>
                        <button class="btn" on:click=reset_accent>"恢复默认"</button>
                    </div>
                </section>

                <section class="setting">
                    <h2>"背景图"</h2>
                    <p class="muted">"可以用外链，也可以上传一张本地图片（登录后可用）。"</p>

                    <div class="field-row">
                        <input
                            type="text"
                            class="text-input"
                            spellcheck="false"
                            placeholder="https://example.com/bg.jpg"
                            value=initial_background
                            on:input=move |ev| background_input.set(event_target_value(&ev))
                        />
                        <button class="btn btn-primary" on:click=apply_background>"应用外链"</button>
                        <button class="btn" on:click=clear_background>"清除"</button>
                    </div>

                    // 上传走普通表单：multipart 用原生提交最直接，也不依赖 JS。
                    // 这里的 POST 不需要额外的 CSRF 令牌——会话 cookie 是 SameSite=Lax，
                    // 跨站表单提交根本带不上它。
                    <Suspense fallback=|| ()>
                    {move || matches!(user_state_for_upload.get(), Some(Ok(Some(_)))).then(|| view! {
                        <form
                            class="field-row upload-row"
                            method="post"
                            action="/api/me/background"
                            enctype="multipart/form-data"
                        >
                            <input
                                type="file"
                                name="file"
                                accept="image/png,image/jpeg,image/gif,image/webp"
                            />
                            <button class="btn btn-primary" type="submit">"上传图片"</button>
                        </form>
                    })}
                    </Suspense>
                </section>

                // 账号入口与登录状态相关的动作（后台管理 / 退出登录）
                <Suspense fallback=|| ()>
                    {move || match user_state_for_account.get() {
                        Some(Ok(Some(user))) => {
                            let profile_href = format!("/user/{}", user.username);
                            let is_admin = user.is_admin();
                            Some(view! {
                                <section class="setting">
                                    <h2>"账号"</h2>
                                    <p class="muted">"公开主页、后台管理与会话操作。"</p>
                                    <div class="field-row">
                                        <a class="btn" href=profile_href>"我的公开主页"</a>
                                        {is_admin
                                            .then(|| view! {
                                                <a class="btn" href="/admin">"后台管理"</a>
                                            })}
                                        <button
                                            type="button"
                                            class="btn btn-danger"
                                            on:click=move |_| {
                                                leptos::task::spawn_local(async move {
                                                    let _ = crate::auth::logout().await;
                                                    crate::auth::reload_page();
                                                });
                                            }
                                        >
                                            "退出登录"
                                        </button>
                                    </div>
                                </section>
                            })
                        }
                        _ => None,
                    }}
                </Suspense>

                // 没登录就没有密码可改
                <Suspense fallback=|| ()>
                    {move || match user_state_for_profile.get() {
                        Some(Ok(Some(user))) => {
                            Some(view! {
                                <section class="setting">
                                    <h2>"昵称与简介"</h2>
                                    <p class="muted">
                                        "昵称显示在顶栏与评论上；简介显示在你的公开资料页。"
                                    </p>
                                    <ProfileForm
                                        display_name=user.display_name.clone()
                                        bio=user.bio.clone().unwrap_or_default()
                                        user_state=user_state_for_profile.clone()
                                    />
                                </section>
                            })
                        }
                        _ => None,
                    }}
                </Suspense>

                <Suspense fallback=|| ()>
                {move || matches!(user_state_for_password.get(), Some(Ok(Some(_)))).then(|| view! {
                    <section class="setting">
                        <h2>"修改密码"</h2>
                        <p class="muted">"改完后其它设备上的登录会立即失效，当前这台保持登录。"</p>
                        <form class="auth-form" on:submit=on_change_password>
                            <label class="field">
                                <span>"当前密码"</span>
                                <input
                                    type="password"
                                    autocomplete="current-password"
                                    prop:value=move || old_password.get()
                                    on:input=move |ev| old_password.set(event_target_value(&ev))
                                />
                            </label>
                            <label class="field">
                                <span>"新密码"</span>
                                <input
                                    type="password"
                                    autocomplete="new-password"
                                    prop:value=move || new_password.get()
                                    on:input=move |ev| new_password.set(event_target_value(&ev))
                                />
                                <small class="muted">"至少 8 个字符。"</small>
                            </label>
                            {move || {
                                let text = password_message.get();
                                (!text.is_empty()).then(|| view! { <p class="notice" role="status">{text}</p> })
                            }}
                            <button class="btn btn-primary" type="submit" disabled=move || password_busy.get()>
                                {move || if password_busy.get() { "提交中…" } else { "修改密码" }}
                            </button>
                        </form>
                    </section>
                })}
                </Suspense>

                <p class="notice" role="status">{move || notice.get()}</p>
            </div>

            <p class="back"><a href="/">"← 回首页"</a></p>
        </section>
    }
}

/// 登录页。
///
/// 提交走 server function；成功后**整页重载**，因为顶栏的登录状态是服务端渲染的。
#[component]
pub fn LoginPage() -> impl IntoView {
    let username = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let message = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    // 登录成功后跳到「我的账号」，避免停在登录页让人误以为没登上。
    let navigate = use_navigate();

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let name = username.get_untracked();
        let pwd = password.get_untracked();

        if name.trim().is_empty() || pwd.is_empty() {
            message.set("请填写用户名和密码。".to_string());
            return;
        }

        message.set(String::new());
        busy.set(true);
        let navigate = navigate.clone();
        leptos::task::spawn_local(async move {
            match crate::auth::login(name, pwd).await {
                Ok(Ok(())) => navigate("/me", Default::default()),
                Ok(Err(text)) => {
                    message.set(text);
                    busy.set(false);
                }
                Err(e) => {
                    message.set(format!("请求失败：{e}"));
                    busy.set(false);
                }
            }
        });
    };

    view! {
        <Title text="登录 — Wraindrock" />
        <section class="wrap">
            <PageHeader title="登录" />
            <form class="auth-form" on:submit=on_submit>
                <label class="field">
                    <span>"用户名"</span>
                    <input
                        type="text"
                        autocomplete="username"
                        prop:value=move || username.get()
                        on:input=move |ev| username.set(event_target_value(&ev))
                    />
                </label>
                <label class="field">
                    <span>"密码"</span>
                    <input
                        type="password"
                        autocomplete="current-password"
                        prop:value=move || password.get()
                        on:input=move |ev| password.set(event_target_value(&ev))
                    />
                </label>
                {move || {
                    let text = message.get();
                    (!text.is_empty()).then(|| view! { <p class="error" role="alert">{text}</p> })
                }}
                <button class="btn btn-primary" type="submit" disabled=move || busy.get()>
                    {move || if busy.get() { "登录中…" } else { "登录" }}
                </button>
            </form>
            // 用 form GET 而不是 <a>：Leptos 客户端路由会拦截站内 <a> 点击
            // （SPA 跳转），而 /auth/oidc/start 是 Axum 后端路由，必须整页请求。
            <form method="get" action="/auth/oidc/start" class="auth-alt">
                <button class="btn" type="submit">"使用 Kanidm 登录"</button>
            </form>
            <p class="muted auth-alt">"还没有账号？" <a href="/register">"注册一个"</a></p>
        </section>
    }
}

/// 注册页。注册成功后直接登录，省掉再输一次。
#[component]
pub fn RegisterPage() -> impl IntoView {
    let username = RwSignal::new(String::new());
    let display_name = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let message = RwSignal::new(String::new());
    let busy = RwSignal::new(false);

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let name = username.get_untracked();
        let shown = display_name.get_untracked();
        let pwd = password.get_untracked();

        // 客户端先校验一次只为即时反馈；服务端仍会独立校验一遍
        if let Err(text) = crate::auth::validate_credentials(name.trim(), &pwd) {
            message.set(text);
            return;
        }

        message.set(String::new());
        busy.set(true);
        leptos::task::spawn_local(async move {
            match crate::auth::register(name, pwd, shown).await {
                Ok(Ok(())) => crate::auth::reload_page(),
                Ok(Err(text)) => {
                    message.set(text);
                    busy.set(false);
                }
                Err(e) => {
                    message.set(format!("请求失败：{e}"));
                    busy.set(false);
                }
            }
        });
    };

    view! {
        <Title text="注册 — Wraindrock" />
        <section class="wrap">
            <PageHeader
                title="注册"
                lede="注册后可以评论与提交评价。本站不发邮件，所以不需要邮箱。".to_string()
            />
            <form class="auth-form" on:submit=on_submit>
                <label class="field">
                    <span>"用户名"</span>
                    <input
                        type="text"
                        autocomplete="username"
                        prop:value=move || username.get()
                        on:input=move |ev| username.set(event_target_value(&ev))
                    />
                    <small class="muted">"3–20 个字符，只能用字母、数字、下划线和连字符。"</small>
                </label>
                <label class="field">
                    <span>"昵称（可留空）"</span>
                    <input
                        type="text"
                        prop:value=move || display_name.get()
                        on:input=move |ev| display_name.set(event_target_value(&ev))
                    />
                    <small class="muted">"留空就用用户名显示。"</small>
                </label>
                <label class="field">
                    <span>"密码"</span>
                    <input
                        type="password"
                        autocomplete="new-password"
                        prop:value=move || password.get()
                        on:input=move |ev| password.set(event_target_value(&ev))
                    />
                    <small class="muted">"至少 8 个字符。"</small>
                </label>
                {move || {
                    let text = message.get();
                    (!text.is_empty()).then(|| view! { <p class="error" role="alert">{text}</p> })
                }}
                <button class="btn btn-primary" type="submit" disabled=move || busy.get()>
                    {move || if busy.get() { "注册中…" } else { "注册" }}
                </button>
            </form>
            <p class="muted auth-alt">"已经有账号？" <a href="/login">"去登录"</a></p>
        </section>
    }
}

/// 公开资料页。
#[component]
pub fn UserProfilePage() -> impl IntoView {
    #[cfg(feature = "ssr")]
    let response_options = use_context::<leptos_axum::ResponseOptions>();

    let username = route_param("username");
    let profile = Resource::new(username, |name| crate::auth::user_profile(name));

    view! {
        <section class="wrap">
            <Suspense fallback=loading>
                {move || profile.get().map(|res| match res {
                    Ok(Some(user)) => {
                        // 简介可为空——那就整段不渲染，别留一个空段落
                        let bio = user
                            .bio
                            .clone()
                            .filter(|text| !text.trim().is_empty())
                            .map(|text| view! { <p>{text}</p> });
                        view! {
                            <Title text=format!("{} — Wraindrock", user.display_name) />
                            <PageHeader title=user.display_name.clone() />
                            <div class="prose">
                                {bio}
                                <p class="muted">"用户名：" <code>{user.username.clone()}</code></p>
                            </div>
                        }
                        .into_any()
                    }
                    Ok(None) => {
                        #[cfg(feature = "ssr")]
                        if let Some(options) = &response_options {
                            options.set_status(axum::http::StatusCode::NOT_FOUND);
                        }
                        view! {
                            <Title text="找不到这个用户 — Wraindrock" />
                            <PageHeader title="找不到这个用户" lede="它可能被改名或删掉了。".to_string() />
                            <p><a href="/">"← 回首页"</a></p>
                        }
                        .into_any()
                    }
                    Err(e) => load_error(e.to_string()).into_any(),
                })}
            </Suspense>
        </section>
    }
}

/// 兜底 404 页。
#[component]
pub fn NotFound() -> impl IntoView {
    #[cfg(feature = "ssr")]
    if let Some(options) = use_context::<leptos_axum::ResponseOptions>() {
        options.set_status(axum::http::StatusCode::NOT_FOUND);
    }

    view! {
        <Title text="页面不存在 — Wraindrock" />
        <section class="wrap">
            <PageHeader title="页面不存在" lede="这个地址没有对应的页面。".to_string() />
            <p><a href="/">"← 回首页"</a></p>
        </section>
    }
}

/// 全站共用的外壳内容：导航 + 内容 + 页脚。
#[component]
pub fn Layout(children: Children) -> impl IntoView {
    view! {
        <SiteHeader />
        <main class="site-main">{children()}</main>
        <SiteFooter />
    }
}

/// 光遇分类页：`/sky/gameplay`（攻略）与 `/sky/gallery`（画廊）。
///
/// 分类是 URL 传来的用户输入，先与白名单比对；不在名单里直接 404，而不是渲染一个
/// 空列表——「这里还没有内容」和「没有这个页面」是两回事。
#[component]
pub fn SkyCategoryPage() -> impl IntoView {
    let category = route_param("category");
    let kind = category();

    if !SKY_CATEGORIES.contains(&kind.as_str()) {
        set_status(404);
        return view! {
            <Title text="页面不存在 — Wraindrock" />
            <section class="wrap">
                <PageHeader title="页面不存在" lede="这个光遇分类没有对应的页面。".to_string() />
                <p><a href="/sky">"← 回光遇"</a></p>
            </section>
        }
        .into_any();
    }

    let label = if kind == "gallery" { "画廊" } else { "攻略" };
    // 站内可编辑的官方内容（DB）优先；仓库 Markdown 作为兼容来源一并展示。
    let items = {
        let kind = kind.clone();
        Resource::new_blocking(move || kind.clone(), |kind| async move {
            let officials = list_sky_official(kind.clone()).await;
            let markdown = list_sky(kind).await;
            (officials, markdown)
        })
    };

    view! {
        <Title text=format!("{label} — 光遇 — Wraindrock") />
        <section class="wrap">
            <PageHeader title=label lede=format!("光遇的{label}。") />
            <p class="back"><a href="/sky">"← 回光遇"</a></p>

            <Suspense fallback=loading>
                {move || match items.get() {
                    None => loading().into_any(),
                    Some((Err(error), _)) => load_error(error.to_string()).into_any(),
                    Some((Ok(officials), markdown)) => {
                        let markdown = markdown.unwrap_or_default();
                        if officials.is_empty() && markdown.is_empty() {
                            return view! {
                                <p class="muted">{format!("{label}还在整理，之后会放上来。")}</p>
                            }
                                .into_any();
                        }
                        let official_views = officials
                            .into_iter()
                            .map(|item| {
                                let href = format!("/sky/{}/{}", item.category, item.slug);
                                let title = item.title.clone();
                                view! {
                                    <article class="sky-item">
                                        <h2><a href=href.clone()>{item.title.clone()}</a></h2>
                                        <p class="card-meta">{item.updated_at.clone()}</p>
                                        {(!item.summary.is_empty())
                                            .then(|| {
                                                view! {
                                                    <p class="card-summary">{item.summary.clone()}</p>
                                                }
                                            })}
                                        {(!item.cover.is_empty())
                                            .then(|| {
                                                view! {
                                                    <img class="sky-cover" src=item.cover.clone() alt=title.clone() />
                                                }
                                            })}
                                        <a href=href>"阅读全文 →"</a>
                                    </article>
                                }
                            })
                            .collect_view();
                        let markdown_views = markdown
                            .into_iter()
                            .map(|item| {
                                let title = item.title.clone();
                                view! {
                                    <article class="sky-item">
                                        <h2>{item.title.clone()}</h2>
                                        <p class="card-meta">
                                            {item.date.clone()}
                                            " · "
                                            <AuthorLine
                                                author=item.author.clone()
                                                account=item.author_account.clone()
                                            />
                                        </p>
                                        {item
                                            .cover
                                            .map(|cover| {
                                                view! {
                                                    <img class="sky-cover" src=cover alt=title.clone() />
                                                }
                                            })}
                                        <div class="prose" inner_html=item.html></div>
                                    </article>
                                }
                            })
                            .collect_view();
                        view! {
                            <div class="sky-list">{official_views}{markdown_views}</div>
                        }
                            .into_any()
                    }
                }}
            </Suspense>
        </section>
    }
    .into_any()
}
