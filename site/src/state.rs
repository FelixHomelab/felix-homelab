//! 全站共享状态（仅服务端）。
//!
//! 这个类型有两个身份，别混淆：
//!
//! 1. Axum 的 **state**——`leptos_axum` 的 `LeptosRoutes` 与
//!    `file_and_error_handler` 都要求 `LeptosOptions: FromRef<S>`，
//!    靠 `#[derive(FromRef)]` 满足，省掉把 `LeptosOptions` 从 state 里拆出去。
//! 2. Leptos 的 **context**——`leptos_routes_with_context` 会把 state 提供成
//!    请求内的 context，因此 `#[server]` 函数里 `use_context::<AppState>()`
//!    就能直接取到连接池，不必自己搭一套提取器。

use axum::extract::FromRef;
use leptos::prelude::LeptosOptions;
use sqlx::SqlitePool;

/// 全站共享状态。
#[derive(Clone, FromRef)]
pub struct AppState {
    pub leptos_options: LeptosOptions,
    pub pool: SqlitePool,
}
