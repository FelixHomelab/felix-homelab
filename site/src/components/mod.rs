//! 全站布局与可复用组件。
//!
//! 导航刻意用普通 `<a>` 而不是 Leptos 的 `<A>`：本站以内容为主，每次跳转都拿一份
//! 完整的服务端渲染文档，比客户端路由更简单也更不容易出错；浏览器端只水合确实
//! 需要交互的部分。

pub mod comments;
pub mod community;
pub mod reviews;

use crate::auth::UserState;
use crate::theme::{ThemeMode, ThemeState};
use leptos::prelude::*;

/// 水合完成后变为 `true` 的信号。
///
/// 用于「依赖会话 Cookie / 客户端状态」的区块：SSR 与客户端首帧都渲染空，
/// 水合结束后再出现，避免两端资源就绪时机不同造成的 hydration 失配
/// （失配会让整站 wasm 水合崩溃：站内跳转失灵、内容不显示）。
pub fn ready_after_hydration() -> Signal<bool> {
    let ready = RwSignal::new(false);
    // Effect 只在客户端运行，且在水合完成后执行
    Effect::new(move |_| ready.set(true));
    ready.into()
}

/// 顶部导航栏。
#[component]
pub fn SiteHeader() -> impl IntoView {
    view! {
        <header class="site-header">
            <div class="wrap header-inner">
                <a class="brand" href="/">"Felix Homelab"</a>
                <div class="header-right">
                    <nav class="site-nav">
                        <a href="/blog">"博客"</a>
                        <a href="/community">"社区"</a>
                        <a href="/sky">"光遇"</a>
                        <a
                            class="nav-service"
                            href="https://cloud.grantfelix.top/"
                            target="_blank"
                            rel="noreferrer"
                        >
                            "OpenCloud"
                        </a>
                        <a
                            class="nav-service"
                            href="https://forgejo.grantfelix.top/"
                            target="_blank"
                            rel="noreferrer"
                        >
                            "Forgejo"
                        </a>
                        <a href="/about">"关于"</a>
                    </nav>
                    <ThemeToggle />
                    <UserMenu />
                </div>
            </div>
        </header>
    }
}

/// 顶栏的账号区域：未登录给登录/注册入口，已登录显示用户名与登出。
///
/// 必须包在 `<Suspense>` 里：Leptos 只把 Suspense 边界内的资源纳入「渲染前先等数据」
/// 的范围，裸读 `.get()` 只会拿到 `None`。`SsrMode::Async` 会等所有 Suspense 边界，
/// 因此首屏就能显示正确的登录状态，而不会先闪一下「未登录」。
#[component]
pub fn UserMenu() -> impl IntoView {
    let state = use_context::<UserState>().expect("UserState 应由 App 提供");

    view! {
        <div class="user-menu">
            <Suspense fallback=|| view! { <span class="muted">"…"</span> }>
                {move || match state.get() {
                    // 还没加载完：留一个占位，避免顶栏在加载前后跳动
                    None => view! { <span class="muted">"…"</span> }.into_any(),
                    // 查询失败时退回「未登录」的入口，不要把用户卡在错误状态
                    Some(Err(_)) => view! { <a href="/login">"登录"</a> }.into_any(),
                    Some(Ok(None)) => view! {
                        <a href="/login">"登录"</a>
                        <a href="/register">"注册"</a>
                    }
                    .into_any(),
                    Some(Ok(Some(user))) => {
                        let href = format!("/user/{}", user.username);
                        let label = if user.display_name.is_empty() {
                            user.username.clone()
                        } else {
                            user.display_name.clone()
                        };
                        view! {
                            <a class="user-name" href=href>{label}</a>
                            // 后台入口只对管理员显示。藏起来只是少一个入口，
                            // 真正的授权在 /admin 各 server function 里。
                            {user.is_admin().then(|| view! { <a href="/admin">"后台"</a> })}
                            <button
                                type="button"
                                class="link-button"
                                on:click=move |_| {
                                    leptos::task::spawn_local(async move {
                                        // 登出必须在服务端删掉会话行，仅清 cookie 不够
                                        let _ = crate::auth::logout().await;
                                        crate::auth::reload_page();
                                    });
                                }
                            >
                                "登出"
                            </button>
                        }
                        .into_any()
                    }
                }}
            </Suspense>
        </div>
    }
}

/// 主题三档切换。放在顶栏，随手可及。
///
/// 点击后既更新 `<html>` 上的 class（响应式，立即生效），也把选择写进 cookie
/// ——不写 cookie 的话，下次刷新服务端就不知道你的偏好，会闪回默认外观。
#[component]
pub fn ThemeToggle() -> impl IntoView {
    let theme = use_context::<ThemeState>().expect("ThemeState 应由 App 提供");

    view! {
        <div class="segmented" role="group" aria-label="主题">
            {[ThemeMode::Auto, ThemeMode::Light, ThemeMode::Dark]
                .into_iter()
                .map(|mode| view! {
                    <button
                        type="button"
                        class="segmented-btn"
                        class:active=move || theme.get().same_mode(mode)
                        title=mode.label()
                        aria-pressed=move || theme.get().same_mode(mode).to_string()
                        on:click=move |_| theme.update(|prefs| prefs.mode = mode)
                    >
                        {mode.label()}
                    </button>
                })
                .collect_view()}
        </div>
    }
}

/// 页脚。
#[component]
pub fn SiteFooter() -> impl IntoView {
    view! {
        <footer class="site-footer">
            <div class="wrap">
                <p class="muted">"© 2026 Felix Homelab · 用 Rust 构建"</p>
            </div>
        </footer>
    }
}

/// 页面标题区块，统一各页的标题排布。
#[component]
pub fn PageHeader(
    /// 大标题。用 `into` 是为了既接受字面量，也接受运行期拼出来的 `String`。
    #[prop(into)]
    title: String,
    /// 标题下的一句话说明，可省略。
    #[prop(optional, into)]
    lede: Option<String>,
) -> impl IntoView {
    view! {
        <header class="page-header">
            <h1>{title}</h1>
            {lede.map(|text| view! { <p class="lede">{text}</p> })}
        </header>
    }
}

/// 文章卡片：列表页的基本单元。
#[component]
pub fn PostCard(post: crate::content::PostSummary) -> impl IntoView {
    let href = format!("/blog/{}", post.slug);
    let meta = format!("{} · {} 分钟", post.date, post.reading_minutes);

    view! {
        <article class="card">
            <a class="card-title" href=href>{post.title.clone()}</a>
            <p class="card-meta">{meta}</p>
            <p class="card-summary">{post.summary}</p>
            <ul class="tag-row">
                {post.tags.into_iter().map(|tag| {
                    let href = format!("/blog/tag/{tag}");
                    view! { <li><a class="tag" href=href>{tag}</a></li> }
                }).collect_view()}
            </ul>
        </article>
    }
}

/// 项目卡片。
#[component]
pub fn ProjectCard(project: crate::content::ProjectSummary) -> impl IntoView {
    let href = format!("/projects/{}", project.slug);
    // 先转成 String：下面 `project.stack.into_iter()` 会部分移出 project，
    // 若这里还持着 project.kind 的借用就会冲突。
    let kind = kind_label(&project.kind).to_string();

    view! {
        <article class="card">
            <div class="card-head">
                <a class="card-title" href=href>{project.name.clone()}</a>
                <span class="badge">{kind}</span>
            </div>
            <p class="card-summary">{project.summary}</p>
            <ul class="tag-row">
                {project.stack.into_iter().map(|item| {
                    view! { <li><span class="tag tag-plain">{item}</span></li> }
                }).collect_view()}
            </ul>
        </article>
    }
}

/// 把项目的 `kind` 翻成中文标签。未知取值原样显示，方便发现写错的 front matter。
pub fn kind_label(kind: &str) -> &str {
    match kind {
        "open" => "开源",
        "private" => "私有",
        "team" => "团队",
        other => other,
    }
}
