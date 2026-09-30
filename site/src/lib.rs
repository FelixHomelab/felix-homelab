//! Wraindrock 社区站 — 前端库入口。
//!
//! 同一个 crate 同时编出两种产物：服务端（`ssr` feature，宿主为 Axum）
//! 与浏览器端 wasm（`hydrate` feature）。因此本文件里凡涉及两者差异的部分
//! 都必须用 `#[cfg(feature = ...)]` 隔开，否则会把服务端依赖编进 wasm 包。
//!
//! 设计文档见仓库根目录 `DESIGN.md`。

// release 构建会把 Leptos 的深层嵌套视图类型展开到默认递归上限之外，报
// 「queries overflow the depth limit!」。debug 构建不会触发，所以只有发布构建
// 才暴露——这正是部署时会用到的那一种。256 是编译器建议值。
#![recursion_limit = "256"]


pub mod admin;
pub mod agents;
pub mod app;
pub mod auth;
pub mod comments;
pub mod community;
pub mod components;
pub mod content;
pub mod pages;
pub mod reviews;
pub mod roles;
pub mod sky;
pub mod theme;

// 只在服务端存在：客户端 wasm 包里既没有数据库也没有 Axum
#[cfg(feature = "ssr")]
pub mod db;
#[cfg(feature = "ssr")]
pub mod media;
#[cfg(feature = "ssr")]
pub mod seo;
#[cfg(feature = "ssr")]
pub mod state;
#[cfg(feature = "ssr")]
pub mod stt;
#[cfg(feature = "ssr")]
pub mod uploads;

/// 浏览器端水合入口：只在 `hydrate` 构建里存在。
/// cargo-leptos 会把生成的 wasm 包指向这个导出函数。
#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    // 把 panic 转成浏览器控制台里可读的报错；默认的 panic 信息在 wasm 里几乎不可读
    console_error_panic_hook::set_once();
    leptos::mount::hydrate_body(app::App);
}

/// 显式注册所有 server function。
///
/// 本构建环境下 server_fn 的自动注册（inventory）没有生效，表现为
/// `POST /api/<fn>` 全部 404 —— 登录、注册、评论、后台动作、Pod 管理都不可用。
/// 按 server_fn 的报错提示，在 `main` 启动时逐个调用 `register_explicit()`。
#[cfg(feature = "ssr")]
pub fn register_server_fns() {
    // admin
    server_fn::axum::register_explicit::<admin::AmIAdmin>();
    server_fn::axum::register_explicit::<admin::AdminLoadOverview>();
    server_fn::axum::register_explicit::<admin::AdminListComments>();
    server_fn::axum::register_explicit::<admin::AdminSetCommentStatus>();
    server_fn::axum::register_explicit::<admin::AdminDeleteComment>();
    server_fn::axum::register_explicit::<admin::AdminListReviews>();
    server_fn::axum::register_explicit::<admin::AdminSetReviewStatus>();
    server_fn::axum::register_explicit::<admin::AdminReplyReview>();
    server_fn::axum::register_explicit::<admin::AdminDeleteReview>();
    server_fn::axum::register_explicit::<admin::AdminListUsers>();
    server_fn::axum::register_explicit::<admin::AdminSetUserStatus>();
    server_fn::axum::register_explicit::<admin::AdminSetUserRole>();
    server_fn::axum::register_explicit::<admin::AdminSetUserScope>();
    server_fn::axum::register_explicit::<roles::AdminPermissions>();
    server_fn::axum::register_explicit::<admin::AdminListPod>();
    server_fn::axum::register_explicit::<admin::AdminRestartContainer>();
    // admin：社区投稿（直接发布 + 事后管理）
    server_fn::axum::register_explicit::<admin::AdminListCommunity>();
    server_fn::axum::register_explicit::<admin::AdminSetCommunityStatus>();
    server_fn::axum::register_explicit::<admin::AdminDeleteCommunity>();
    // admin：备份（分源 + 多渠道）
    server_fn::axum::register_explicit::<admin::AdminBackupConfig>();
    server_fn::axum::register_explicit::<admin::AdminBackupSaveConfig>();
    server_fn::axum::register_explicit::<admin::AdminBackupTriggerSync>();
    server_fn::axum::register_explicit::<admin::AdminBackupNow>();
    server_fn::axum::register_explicit::<admin::AdminBackupList>();

    // agents：多租户 AI Agent 后台
    server_fn::axum::register_explicit::<agents::AdminListAgents>();
    server_fn::axum::register_explicit::<agents::AdminGrantAgent>();
    server_fn::axum::register_explicit::<agents::AdminAgentAction>();
    server_fn::axum::register_explicit::<agents::AdminRenewAgent>();
    server_fn::axum::register_explicit::<agents::AdminPurgeAgent>();

    // auth
    server_fn::axum::register_explicit::<auth::CurrentUser>();
    server_fn::axum::register_explicit::<auth::Register>();
    server_fn::axum::register_explicit::<auth::Login>();
    server_fn::axum::register_explicit::<auth::Logout>();
    server_fn::axum::register_explicit::<auth::UserProfile>();
    server_fn::axum::register_explicit::<auth::ChangePassword>();
    server_fn::axum::register_explicit::<auth::UpdateProfile>();

    // comments
    server_fn::axum::register_explicit::<comments::LoadCommentThread>();
    server_fn::axum::register_explicit::<comments::SubmitComment>();

    // community
    server_fn::axum::register_explicit::<community::ListCommunity>();
    server_fn::axum::register_explicit::<community::GetCommunity>();
    server_fn::axum::register_explicit::<community::SubmitCommunity>();
    server_fn::axum::register_explicit::<community::UpdateCommunity>();
    server_fn::axum::register_explicit::<community::DeleteCommunity>();
    // 光遇：官方内容 / 代跑展示 / 投稿管理 / 精选
    server_fn::axum::register_explicit::<sky::ListSkyOfficial>();
    server_fn::axum::register_explicit::<sky::GetSkyOfficial>();
    server_fn::axum::register_explicit::<sky::SkyBoosting>();
    server_fn::axum::register_explicit::<sky::ListSkyCommunity>();
    server_fn::axum::register_explicit::<sky::AdminListSkyOfficial>();
    server_fn::axum::register_explicit::<sky::AdminSaveSkyOfficial>();
    server_fn::axum::register_explicit::<sky::AdminDeleteSkyOfficial>();
    server_fn::axum::register_explicit::<sky::AdminSaveSkyBoosting>();
    server_fn::axum::register_explicit::<sky::AdminListSkyPosts>();
    server_fn::axum::register_explicit::<sky::AdminFeatureSkyPost>();
    server_fn::axum::register_explicit::<sky::AdminPinReview>();

    // content
    server_fn::axum::register_explicit::<content::ListPosts>();
    server_fn::axum::register_explicit::<content::GetPost>();
    server_fn::axum::register_explicit::<content::ListPostsByTag>();
    server_fn::axum::register_explicit::<content::ListTags>();
    server_fn::axum::register_explicit::<content::ListProjects>();
    server_fn::axum::register_explicit::<content::GetProject>();
    server_fn::axum::register_explicit::<content::GetPage>();
    server_fn::axum::register_explicit::<content::ListSky>();

    // reviews
    server_fn::axum::register_explicit::<reviews::LoadReviewBoard>();
    server_fn::axum::register_explicit::<reviews::SubmitSkyReview>();

    // theme
    server_fn::axum::register_explicit::<theme::SaveThemePrefs>();

    tracing::info!(
        "server function 已注册：{} 个",
        server_fn::axum::server_fn_paths().count()
    );
}
