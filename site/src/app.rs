//! 应用根组件与文档外壳。
//!
//! `shell` 决定服务端吐出的整份 HTML 骨架（SEO 与首屏都靠它），
//! `App` 是路由与全局 Provider 的挂载点。

use crate::auth::{current_user, UserState};
use crate::pages::admin::{
    AdminAgentPage, AdminBackupPage, AdminCommentsPage, AdminCommunityPage,
    AdminDashboardPage, AdminPodPage, AdminReviewsPage, AdminUsersPage,
};
use crate::pages::community::{CommunityDetailPage, CommunityIndex, CommunitySubmitPage};
use crate::pages::{
    AboutPage, AppearancePage, BlogIndex, BlogPost, BlogTag, ContactPage, HomePage, Layout,
    LoginPage, NotFound, ProjectIndex, ProjectShow, RegisterPage, SkyBoostingPage,
    SkyCategoryPage, SkyIndex, UserProfilePage,
};
use crate::theme::{ThemePrefs, ThemeState};
use leptos::prelude::*;
use leptos_meta::{provide_meta_context, Html, MetaTags, Stylesheet};
use leptos_router::components::{Route, Router, Routes};
use leptos_router::{path, SsrMode, StaticSegment};

/// 文档外壳。整站只有这一处 `<html>`。
pub fn shell(options: LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="zh-CN">
            <head>
                <meta charset="utf-8" />
                <meta name="viewport" content="width=device-width, initial-scale=1" />
                // 开发期热重载；生产构建下 AutoReload 不会输出任何东西
                <AutoReload options=options.clone() />
                <HydrationScripts options />
                <MetaTags />
            </head>
            <body>
                <App />
            </body>
        </html>
    }
}

/// 应用根组件。
#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();

    // 初始偏好：服务端从请求头的 Cookie 读，浏览器端从 document.cookie 读。
    // 两端读的是同一只 cookie，所以水合时取值一致，不会出现不匹配。
    let theme = ThemeState::new(ThemePrefs::current());
    provide_context(theme);

    // 当前登录用户需要查库，所以只能是异步资源。用 `new_blocking` 而不是 `new`：
    // 阻塞型资源会**阻止 HTTP 响应发出直到数据就绪**，这正是「首屏就要正确的登录状态」
    // 需要的语义。普通资源只是「读到就先给，读不到给 None」，位置不对会变成竞态——
    // 同一份代码有时渲染、有时不渲染。
    let user = Resource::new_blocking(|| (), |_| current_user());
    provide_context(UserState::new(user));

    view! {
        <Stylesheet id="leptos" href="/pkg/felix-homelab-site.css" />

        // 主题在第一帧就写进 <html>：class 决定亮暗，style 给出自定义的主色与背景图。
        // 这是「不闪烁」的全部秘密——服务端已经知道答案，不必等水合后再改样式。
        <Html {..} class=move || theme.html_class() style=move || theme.style_attr() />

        <Router>
            <Layout>
                // 全站统一用 SsrMode::Async：服务端等所有资源就绪后一次性渲染出完整
                // HTML。默认的 OutOfOrder 会把内容塞进 <template> 再靠内联 JS 搬位，
                // 禁用 JS 的访客与爬虫只会看到 fallback；而且 <head> 先于数据发出，
                // 写在 Suspense 里的 <Title> 会丢失。内容站不值得为这点 TTFB 让步。
                <Routes fallback=NotFound>
                    <Route path=StaticSegment("") view=HomePage ssr=SsrMode::Async />
                    <Route path=path!("/blog") view=BlogIndex ssr=SsrMode::Async />
                    // 标签路由必须排在 :slug 前面，否则 tag 会被当成文章 slug
                    <Route path=path!("/blog/tag/:tag") view=BlogTag ssr=SsrMode::Async />
                    <Route path=path!("/blog/:slug") view=BlogPost ssr=SsrMode::Async />
                    // 社区投稿：静态子路径必须排在 :username/:slug 之前
                    <Route path=path!("/community") view=CommunityIndex ssr=SsrMode::Async />
                    <Route path=path!("/community/posts") view=CommunityIndex ssr=SsrMode::Async />
                    <Route
                        path=path!("/community/projects")
                        view=CommunityIndex
                        ssr=SsrMode::Async
                    />
                    <Route path=path!("/community/sky") view=CommunityIndex ssr=SsrMode::Async />
                    <Route
                        path=path!("/community/new")
                        view=CommunitySubmitPage
                        ssr=SsrMode::Async
                    />
                    <Route
                        path=path!("/community/:username/:slug/edit")
                        view=CommunitySubmitPage
                        ssr=SsrMode::Async
                    />
                    <Route
                        path=path!("/community/:username/:slug")
                        view=CommunityDetailPage
                        ssr=SsrMode::Async
                    />
                    <Route path=path!("/projects") view=ProjectIndex ssr=SsrMode::Async />
                    <Route path=path!("/projects/:slug") view=ProjectShow ssr=SsrMode::Async />
                    <Route path=path!("/about") view=AboutPage ssr=SsrMode::Async />
                    <Route path=path!("/contact") view=ContactPage ssr=SsrMode::Async />
                    <Route path=path!("/sky") view=SkyIndex ssr=SsrMode::Async />
                    // boosting 必须排在 :category 之前，否则它会被当成一个分类
                    <Route path=path!("/sky/boosting") view=SkyBoostingPage ssr=SsrMode::Async />
                    <Route path=path!("/sky/:category") view=SkyCategoryPage ssr=SsrMode::Async />
                    // /me 会读「当前用户」这个资源，必须是 Async——否则登录后才该出现的
                    // 上传表单与改密码区块根本不会渲染（资源在首屏还是 None）。
                    <Route path=path!("/me") view=AppearancePage ssr=SsrMode::Async />
                    <Route path=path!("/login") view=LoginPage ssr=SsrMode::Async />
                    <Route path=path!("/register") view=RegisterPage ssr=SsrMode::Async />
                    <Route path=path!("/user/:username") view=UserProfilePage ssr=SsrMode::Async />
                    // 后台四条。页面本身会判一次身份只是为了给出像样的 403，
                    // 真正的授权在每个 server function 里。
                    <Route path=path!("/admin") view=AdminDashboardPage ssr=SsrMode::Async />
                    <Route path=path!("/admin/comments") view=AdminCommentsPage ssr=SsrMode::Async />
                    <Route
                        path=path!("/admin/community")
                        view=AdminCommunityPage
                        ssr=SsrMode::Async
                    />
                    <Route
                        path=path!("/admin/sky-reviews")
                        view=AdminReviewsPage
                        ssr=SsrMode::Async
                    />
                    <Route path=path!("/admin/users") view=AdminUsersPage ssr=SsrMode::Async />
                    <Route path=path!("/admin/pod") view=AdminPodPage ssr=SsrMode::Async />
                    <Route path=path!("/admin/agents") view=AdminAgentPage ssr=SsrMode::Async />
                    <Route path=path!("/admin/backup") view=AdminBackupPage ssr=SsrMode::Async />

                    // 兼容尾斜杠：leptos_router 0.8 不做尾斜杠归一，`/admin/` 会落到
                    // NotFound。给每个页面补一条带尾斜杠的别名，避免手输/复制多一个
                    // 斜杠就打不开。（顺序要求：带参数的路由仍在更具体之后）
                    <Route path=path!("/blog/") view=BlogIndex ssr=SsrMode::Async />
                    <Route path=path!("/blog/tag/:tag/") view=BlogTag ssr=SsrMode::Async />
                    <Route path=path!("/blog/:slug/") view=BlogPost ssr=SsrMode::Async />
                    <Route path=path!("/community/") view=CommunityIndex ssr=SsrMode::Async />
                    <Route
                        path=path!("/community/posts/")
                        view=CommunityIndex
                        ssr=SsrMode::Async
                    />
                    <Route
                        path=path!("/community/projects/")
                        view=CommunityIndex
                        ssr=SsrMode::Async
                    />
                    <Route path=path!("/community/sky/") view=CommunityIndex ssr=SsrMode::Async />
                    <Route
                        path=path!("/community/new/")
                        view=CommunitySubmitPage
                        ssr=SsrMode::Async
                    />
                    <Route
                        path=path!("/community/:username/:slug/edit/")
                        view=CommunitySubmitPage
                        ssr=SsrMode::Async
                    />
                    <Route
                        path=path!("/community/:username/:slug/")
                        view=CommunityDetailPage
                        ssr=SsrMode::Async
                    />
                    <Route path=path!("/projects/") view=ProjectIndex ssr=SsrMode::Async />
                    <Route path=path!("/projects/:slug/") view=ProjectShow ssr=SsrMode::Async />
                    <Route path=path!("/about/") view=AboutPage ssr=SsrMode::Async />
                    <Route path=path!("/contact/") view=ContactPage ssr=SsrMode::Async />
                    <Route path=path!("/sky/") view=SkyIndex ssr=SsrMode::Async />
                    <Route path=path!("/sky/boosting/") view=SkyBoostingPage ssr=SsrMode::Async />
                    <Route path=path!("/sky/:category/") view=SkyCategoryPage ssr=SsrMode::Async />
                    <Route path=path!("/me/") view=AppearancePage ssr=SsrMode::Async />
                    <Route path=path!("/login/") view=LoginPage ssr=SsrMode::Async />
                    <Route path=path!("/register/") view=RegisterPage ssr=SsrMode::Async />
                    <Route path=path!("/user/:username/") view=UserProfilePage ssr=SsrMode::Async />
                    <Route path=path!("/admin/") view=AdminDashboardPage ssr=SsrMode::Async />
                    <Route
                        path=path!("/admin/comments/")
                        view=AdminCommentsPage
                        ssr=SsrMode::Async
                    />
                    <Route
                        path=path!("/admin/community/")
                        view=AdminCommunityPage
                        ssr=SsrMode::Async
                    />
                    <Route
                        path=path!("/admin/sky-reviews/")
                        view=AdminReviewsPage
                        ssr=SsrMode::Async
                    />
                    <Route path=path!("/admin/users/") view=AdminUsersPage ssr=SsrMode::Async />
                    <Route path=path!("/admin/pod/") view=AdminPodPage ssr=SsrMode::Async />
                    <Route path=path!("/admin/agents/") view=AdminAgentPage ssr=SsrMode::Async />
                    <Route path=path!("/admin/backup/") view=AdminBackupPage ssr=SsrMode::Async />
                </Routes>
            </Layout>
        </Router>
    }
}
