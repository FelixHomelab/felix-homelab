//! 服务端入口：Axum 承载 Leptos 的 SSR 路由，并持有 SQLite 连接池。
//!
//! 只在 `ssr` feature 下编译；浏览器端由 `lib.rs::hydrate` 接管。
//!
//! 启动顺序刻意固定为：连库 → 跑迁移 → 确保管理员存在 → 载入内容 → 起服务。
//! 任何一步失败都直接让进程起不来：宁可部署时立刻报错，也不要等访客点开某个页面
//! 才发现库没建好或某篇文章的 front matter 写坏了。

// release 构建会把 Leptos 的深层嵌套视图类型展开到默认递归上限之外，报
// 「queries overflow the depth limit!」。debug 构建不会触发，所以只有发布构建
// 才暴露——这正是部署时会用到的那一种。256 是编译器建议值。
#![recursion_limit = "256"]


#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use axum::extract::DefaultBodyLimit;
    use axum::routing::{get, post};
    use axum::{Json, Router};
    use felix_homelab_site::app::{shell, App};
    use felix_homelab_site::state::AppState;
    use felix_homelab_site::{
        agents, auth, auth_oidc, content, db, register_server_fns, seo, uploads,
    };
    use leptos::prelude::*;
    use leptos_axum::{generate_route_list, LeptosRoutes};

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,sqlx=warn".into()),
        )
        .init();

    // server function 注册：自动注册在本环境未生效（/api/* 会 404），显式注册。
    register_server_fns();

    // --- 数据库 ---
    // 默认落在 data/site.db；换路径只需改 DATABASE_URL，不必重新编译。
    let database_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://data/site.db".to_string());
    let pool = db::connect(&database_url).await?;
    db::migrate(&pool).await?;
    tracing::info!("数据库就绪：{database_url}");

    // 站长账号由环境变量初始化，不走开放注册（见 auth::ensure_admin）
    auth::ensure_admin(&pool).await?;

    // Agent 子域回填：老数据生成新样式域名，并让宿主刷新路由
    agents::ensure_subdomains(&pool).await?;

    // --- 内容 ---
    let content_dir = std::env::var("CONTENT_DIR").unwrap_or_else(|_| "content".to_string());
    content::store::load(&content_dir)
        .map_err(|e| anyhow::anyhow!("载入内容失败（目录 {content_dir}）: {e}"))?;
    tracing::info!("内容已载入：目录 {content_dir}");

    // --- Leptos ---
    let conf = get_configuration(None)?;
    let leptos_options = conf.leptos_options;
    let addr = leptos_options.site_addr;
    let routes = generate_route_list(App);

    let state = AppState {
        leptos_options: leptos_options.clone(),
        pool: pool.clone(),
    };

    let app = Router::new()
        // 健康检查：顺带证明数据库真的连上了，而不只是进程活着。
        // 注意错误类型必须是 IntoResponse——sqlx::Error 不是，所以显式映射成 503。
        .route(
            "/healthz",
            get(|axum::extract::State(state): axum::extract::State<AppState>| async move {
                let version: String = sqlx::query_scalar("SELECT sqlite_version()")
                    .fetch_one(&state.pool)
                    .await
                    .map_err(|e| {
                        tracing::error!("健康检查读取数据库失败: {e}");
                        (
                            axum::http::StatusCode::SERVICE_UNAVAILABLE,
                            "database unavailable",
                        )
                    })?;
                Ok::<_, (axum::http::StatusCode, &'static str)>(Json(serde_json::json!({
                    "status": "ok",
                    "sqlite": version,
                })))
            }),
        )
        // 背景图上传与清除：普通 Axum 路由而非 server function，
        // 这样普通 multipart 表单提交就能用，不依赖 JS。
        // 上传路由单独收紧请求体上限，避免有人拿大文件把内存打满。
        .route(
            "/api/me/background",
            post(uploads::upload_background).layer(DefaultBodyLimit::max(uploads::BODY_LIMIT)),
        )
        .route(
            "/api/me/background/clear",
            post(uploads::clear_background),
        )
        // 已上传图片的对外出口
        .route("/uploads/{*path}", get(uploads::serve_upload))
        // 多租户 Agent 网关鉴权：Caddy forward_auth 调用（GET），
        // 必须在下面的 server function 兜底路由之前注册。
        .route("/api/agent/auth", get(agents::agent_auth))
        // 云侧 Caddy on-demand TLS 的授权回调（见 README「AI Agent」）
        .route("/api/agent/tls-ask", get(agents::agent_tls_ask))
        // 主站 Kanidm OIDC 登录（与密码登录并联，见 auth_oidc.rs）
        .route("/auth/oidc/start", get(auth_oidc::start))
        .route("/auth/oidc/callback", get(auth_oidc::callback))
        // server function 兜底挂载：本环境 leptos_routes 的自动挂载不生效，
        // 改为「显式注册（register_server_fns）+ 这里统一切到 handle_server_fns」。
        .route("/api/{*fn_name}", post(leptos_axum::handle_server_fns))
        // 联系信息已并入「关于」：旧入口 301 过去，避免两页重复维护。
        .route(
            "/contact",
            get(|| async { axum::response::Redirect::permanent("/about") }),
        )
        .route(
            "/contact/",
            get(|| async { axum::response::Redirect::permanent("/about") }),
        )
        // 光遇是独立板块：旧的社区光遇入口在 Axum 层直接 301 到光遇，
        // 不经过模板层（对浏览器与爬虫都是真正的跳转）。
        .route(
            "/community/sky",
            get(|| async { axum::response::Redirect::permanent("/sky") }),
        )
        .route(
            "/community/sky/",
            get(|| async { axum::response::Redirect::permanent("/sky") }),
        )
        // SEO 三件套：产出 XML 与纯文本，直接由 Axum 提供，不过模板层
        .route("/rss.xml", get(seo::rss))
        .route("/sitemap.xml", get(seo::sitemap))
        .route("/robots.txt", get(seo::robots))
        // 用 with_context 版本：它会把 AppState 提供成 Leptos context，
        // server function 里 `use_context::<AppState>()` 就能拿到连接池。
        .leptos_routes_with_context(
            &state,
            routes,
            || {},
            {
                let leptos_options = leptos_options.clone();
                move || shell(leptos_options.clone())
            },
        )
        .fallback(leptos_axum::file_and_error_handler::<AppState, _>(shell))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("listening on http://{addr}");
    axum::serve(listener, app.into_make_service()).await?;

    Ok(())
}

#[cfg(not(feature = "ssr"))]
fn main() {
    // 浏览器端不需要 main：入口是 lib.rs 里的 hydrate()
}
