//! 账号与会话。
//!
//! 三条安全线，都在这一层守住：
//!
//! 1. **密码只以 argon2id 的 PHC 串入库**，绝无明文；PHC 串自带随机盐与参数，
//!    将来升级参数也不影响老密码校验。
//! 2. **会话令牌只在浏览器里存在**，库里存的是它的 SHA-256。库文件泄露也无法
//!    直接拿来登录。
//! 3. **cookie 带 `HttpOnly`**，脚本读不到令牌；`SameSite=Lax` 足以挡住跨站表单
//!    提交，同时不影响站内正常跳转。
//!
//! server function 的返回统一用 [`ActionResult`]：外层是传输层错误，内层是可以
//! 直接展示给用户的业务错误。这样不必为业务错误去实现 `FromServerFnError`。

use leptos::prelude::*;
use serde::{Deserialize, Serialize};

/// 存会话令牌的 cookie 名。
pub const COOKIE_SESSION: &str = "gf_session";

/// 会话有效期（天）。
pub const SESSION_DAYS: i64 = 30;

/// 简介长度上限（按字符数）。
///
/// 昵称**刻意不设上限**：昵称是个人表达，卡在 30 字上只会逼人缩写。代价是它能
/// 拉长每个带昵称的页面，属于已知取舍。
pub const BIO_MAX: usize = 300;

/// 用户名长度限制（按字符数）。
const USERNAME_MIN: usize = 3;
const USERNAME_MAX: usize = 20;

/// 密码长度限制（按字符数）。
const PASSWORD_MIN: usize = 8;
const PASSWORD_MAX: usize = 128;

/// 给前端的用户视图。刻意只含有展示需要的字段——
/// `password_hash`、`email` 之类一概不出库到前端。
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct UserView {
    pub id: i64,
    pub username: String,
    pub display_name: String,
    /// 自我简介，可空。只显示在公开资料页上。
    pub bio: Option<String>,
    pub role: String,
}

impl UserView {
    /// 是否管理员。后台鉴权用它。
    pub fn is_admin(&self) -> bool {
        self.role == "admin"
    }
}

/// server function 的统一返回：外层传输层错误，内层业务错误（可直接展示）。
pub type ActionResult = Result<Result<(), String>, ServerFnError>;

/// 当前登录用户的资源。
///
/// 名字不能叫 `CurrentUser`：`#[server]` 宏会为 `current_user` 生成同名类型，
/// 撞上之后宏的 `impl` 会落到 `Resource` 这个外部类型上，报 E0117。
pub type CurrentUserResource = Resource<Result<Option<UserView>, ServerFnError>>;

/// 把「当前用户」放进 context，供顶栏与各页面共用同一个资源，避免每处各拉一次。
#[derive(Clone)]
pub struct UserState {
    resource: CurrentUserResource,
}

impl UserState {
    /// 包一个已建好的资源。
    pub fn new(resource: CurrentUserResource) -> Self {
        Self { resource }
    }

    /// 读取当前用户。`None` 表示还没加载完，`Ok(None)` 表示未登录，
    /// `Ok(Some(_))` 表示已登录，`Err(_)` 表示这次查询失败。
    pub fn get(&self) -> Option<Result<Option<UserView>, ServerFnError>> {
        self.resource.get()
    }

    /// 登录、登出后重新拉取。
    pub fn refetch(&self) {
        self.resource.refetch();
    }
}

/// 校验密码长度。注册与改密码共用，避免两处各写一遍提示语。
pub fn validate_password(password: &str) -> Result<(), String> {
    let length = password.chars().count();
    if !(PASSWORD_MIN..=PASSWORD_MAX).contains(&length) {
        return Err(format!("密码需 {PASSWORD_MIN}–{PASSWORD_MAX} 个字符。"));
    }
    Ok(())
}

/// 校验用户名与密码的基本格式。返回给用户看的中文说明。
///
/// 客户端与服务器共用同一份规则：客户端提前拦下是为了即时反馈，
/// 服务端仍然必须再校验一次——客户端的校验永远只是体验，不是防线。
///
/// 用户名**大小写不敏感**：`Felix` 与 `felix` 是同一个人，不能各占一个账号。
/// 实现方式是存储照旧保留用户挑的写法，查找时统一加 `COLLATE NOCASE`，数据库
/// 侧再靠 `idx_users_username_nocase` 兜住并发注册。迁移见 `0002`。
pub fn validate_credentials(username: &str, password: &str) -> Result<(), String> {
    let name_len = username.chars().count();
    if !(USERNAME_MIN..=USERNAME_MAX).contains(&name_len) {
        return Err(format!("用户名需 {USERNAME_MIN}–{USERNAME_MAX} 个字符。"));
    }
    // 限定 ASCII：用户名会出现在 /user/:username 这类路径里，收紧字符集省掉转义麻烦
    if !username
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err("用户名只能用字母、数字、下划线和连字符。".to_string());
    }

    validate_password(password)
}

/// 登录或登出之后重新加载页面。
///
/// 顶栏的登录状态与主题都是**服务端渲染**的，局部刷新拿不到新状态，
/// 必须让浏览器重新请求一次。
pub fn reload_page() {
    #[cfg(not(feature = "ssr"))]
    if let Some(window) = web_sys::window() {
        let _ = window.location().reload();
    }
}

// ---------------------------------------------------------------------------
// 仅服务端：密码、令牌、cookie
// ---------------------------------------------------------------------------

#[cfg(feature = "ssr")]
mod crypto {
    use argon2::password_hash::phc::PasswordHash;
    use argon2::{Argon2, PasswordHasher, PasswordVerifier};
    use sha2::{Digest, Sha256};

    /// 生成密码哈希。`Argon2::default()` 即 Argon2id + v19 + 推荐参数，
    /// 返回的 PHC 串已含随机盐与参数，直接入库即可。
    pub fn hash_password(password: &str) -> anyhow::Result<String> {
        Argon2::default()
            .hash_password(password.as_bytes())
            .map(|hash| hash.to_string())
            .map_err(|e| anyhow::anyhow!("密码哈希失败: {e}"))
    }

    /// 校验密码。解析失败与比对失败一律返回 `false`，不向调用方区分原因。
    pub fn verify_password(password: &str, stored: &str) -> bool {
        let Ok(parsed) = PasswordHash::new(stored) else {
            return false;
        };
        Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok()
    }

    /// 生成会话令牌：返回 (写进 cookie 的明文, 入库的哈希)。
    pub fn new_session_token() -> (String, String) {
        let mut bytes = [0u8; 32];
        rand::fill(&mut bytes);
        let plain = hex::encode(bytes);
        let hash = hash_token(&plain);
        (plain, hash)
    }

    /// 令牌哈希。
    ///
    /// 这里用 SHA-256 而不是 argon2：令牌本身是 256 位随机数，不存在被爆破或撞库的
    /// 可能，只需要保证「库泄露后不能直接拿来登录」；而 argon2 每个请求都算一遍太贵。
    pub fn hash_token(token: &str) -> String {
        hex::encode(Sha256::digest(token.as_bytes()))
    }
}

#[cfg(feature = "ssr")]
mod cookies {
    use super::{COOKIE_SESSION, SESSION_DAYS};
    use axum::http::header::{HeaderValue, SET_COOKIE};
    use leptos::prelude::*;

    /// 从本次请求的 `Cookie` 头里取会话令牌。
    pub fn session_token() -> Option<String> {
        let parts = use_context::<axum::http::request::Parts>()?;
        super::session_token_from_headers(&parts.headers)
    }

    /// 给响应追加 `Set-Cookie`。
    ///
    /// `ResponseOptions` 由 leptos_axum 在每个 server function 请求里现提供一份，
    /// 并在返回前合并进响应，所以这里设置就是最终结果。
    fn append_cookie(value: String) {
        let Some(options) = use_context::<leptos_axum::ResponseOptions>() else {
            return;
        };
        match HeaderValue::from_str(&value) {
            Ok(header) => options.append_header(SET_COOKIE, header),
            Err(_) => leptos::logging::error!("构造 Set-Cookie 失败：{value}"),
        }
    }

    /// 上线后设为 HTTPS 时，把 `COOKIE_SECURE=1` 打开，让浏览器只在加密连接上回传令牌。
    fn secure_flag() -> &'static str {
        if std::env::var("COOKIE_SECURE").is_ok() {
            "; Secure"
        } else {
            ""
        }
    }

    /// 下发会话 cookie。
    pub fn set_session(token: &str) {
        append_cookie(format!(
            "{COOKIE_SESSION}={token}; Path=/; Max-Age={}; HttpOnly; SameSite=Lax{}",
            SESSION_DAYS * 24 * 60 * 60,
            secure_flag()
        ));
    }

    /// 清除会话 cookie。
    pub fn clear_session() {
        append_cookie(format!(
            "{COOKIE_SESSION}=; Path=/; Max-Age=0; HttpOnly; SameSite=Lax{}",
            secure_flag()
        ));
    }
}

/// 认证与建会话的内部实现。
///
/// 抽成普通异步函数而不是 server function：注册成功后要直接登录，
/// server function 之间无法互相调用。
#[cfg(feature = "ssr")]
async fn authenticate_and_start_session(
    pool: &sqlx::SqlitePool,
    username: &str,
    password: &str,
) -> Result<(), String> {
    use sqlx::Row;

    let row = sqlx::query(
        "SELECT id, password_hash, status FROM users WHERE username = ?1 COLLATE NOCASE",
    )
    .bind(username)
    .fetch_optional(pool)
    .await
        .map_err(|e| {
            leptos::logging::error!("查询用户失败: {e}");
            "服务器出了点问题，稍后再试。".to_string()
        })?;

    let Some(row) = row else {
        // 用户不存在时也做一次等价的哈希校验：否则「存在」要花几十毫秒而
        // 「不存在」立即返回，可以用响应时间把已注册的用户名枚举出来。
        static DUMMY: std::sync::OnceLock<String> = std::sync::OnceLock::new();
        let dummy = DUMMY.get_or_init(|| {
            crypto::hash_password("timing-equalizer").unwrap_or_else(|_| String::new())
        });
        crypto::verify_password(password, dummy);
        return Err("用户名或密码不对。".to_string());
    };

    let stored: String = row.get("password_hash");
    if !crypto::verify_password(password, &stored) {
        // 与「用户不存在」返回同一句话，避免泄露某个用户名是否已注册
        return Err("用户名或密码不对。".to_string());
    }

    let status: String = row.get("status");
    if status != "active" {
        return Err("这个账号已被停用。".to_string());
    }

    let user_id: i64 = row.get("id");
    let (token, token_hash) = crypto::new_session_token();

    // 过期时间交给 SQLite 自己算，避免 Rust 侧与数据库侧的日期格式对不上
    sqlx::query(
        "INSERT INTO sessions (user_id, token_hash, expires_at) \
         VALUES (?1, ?2, datetime('now', ?3))",
    )
    .bind(user_id)
    .bind(&token_hash)
    .bind(format!("+{SESSION_DAYS} days"))
    .execute(pool)
    .await
    .map_err(|e| {
        leptos::logging::error!("创建会话失败: {e}");
        "服务器出了点问题，稍后再试。".to_string()
    })?;

    // 顺手清掉该用户的过期会话，不必另设定时任务
    let _ = sqlx::query("DELETE FROM sessions WHERE user_id = ?1 AND expires_at <= datetime('now')")
        .bind(user_id)
        .execute(pool)
        .await;

    let _ = sqlx::query("UPDATE users SET last_login_at = datetime('now') WHERE id = ?1")
        .bind(user_id)
        .execute(pool)
        .await;

    cookies::set_session(&token);

    // 把账号里的外观偏好镜像进 cookie。主题的读取路径是同步的，cookie 就是账号偏好的
    // 缓存；镜像失败只影响「这次登录后的外观」，不该让登录本身失败。
    if let Err(error) = crate::theme::mirror_account_prefs(pool, user_id).await {
        leptos::logging::warn!("镜像账号外观偏好失败: {error}");
    }

    Ok(())
}

/// 按用户名取用户视图（仅服务端内部用）。
#[cfg(feature = "ssr")]
async fn fetch_user_view(
    pool: &sqlx::SqlitePool,
    username: &str,
) -> Option<UserView> {
    use sqlx::Row;
    let row = sqlx::query(
        "SELECT id, username, display_name, bio, role FROM users \
         WHERE username = ?1 COLLATE NOCASE",
    )
    .bind(username)
    .fetch_optional(pool)
    .await
    .ok()??;
    Some(UserView {
        id: row.get("id"),
        username: row.get("username"),
        display_name: row.get("display_name"),
        bio: row.get("bio"),
        role: row.get("role"),
    })
}

/// 从 `Cookie` 头里取会话令牌。
///
/// 抽成不依赖 Leptos context 的纯函数：普通 Axum 处理器（如背景图上传）也要用。
#[cfg(feature = "ssr")]
pub fn session_token_from_headers(headers: &axum::http::HeaderMap) -> Option<String> {
    let header = headers.get(axum::http::header::COOKIE)?.to_str().ok()?;

    header.split(';').find_map(|pair| {
        let (name, value) = pair.split_once('=')?;
        (name.trim() == COOKIE_SESSION).then(|| value.trim().to_string())
    })
}

/// 解析当前登录用户 id 的**无上下文**版本。
#[cfg(feature = "ssr")]
pub async fn user_id_from_headers(
    pool: &sqlx::SqlitePool,
    headers: &axum::http::HeaderMap,
) -> Option<i64> {
    let token = session_token_from_headers(headers)?;
    let token_hash = crypto::hash_token(&token);

    sqlx::query_scalar(
        "SELECT u.id FROM sessions s JOIN users u ON u.id = s.user_id \
         WHERE s.token_hash = ?1 \
           AND s.expires_at > datetime('now') \
           AND u.status = 'active'",
    )
    .bind(&token_hash)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
}

/// 登录身份（仅服务端内部用）。
///
/// 与 [`UserView`] 的区别：这是给服务端自己判权限用的，只取判断所需的最小字段，
/// 不经过序列化、也不会流到前端。
#[cfg(feature = "ssr")]
#[derive(Clone, Debug)]
pub struct Identity {
    pub id: i64,
    pub role: String,
}

#[cfg(feature = "ssr")]
impl Identity {
    /// 是否管理员。
    pub fn is_admin(&self) -> bool {
        self.role == "admin"
    }
}

/// 取当前登录用户的身份。未登录、会话过期或账号被停用都返回 `None`。
#[cfg(feature = "ssr")]
pub async fn current_identity() -> Option<Identity> {
    use crate::state::AppState;
    use sqlx::Row;

    let app = use_context::<AppState>()?;
    let parts = use_context::<axum::http::request::Parts>()?;
    let token = session_token_from_headers(&parts.headers)?;
    let token_hash = crypto::hash_token(&token);

    let row = sqlx::query(
        "SELECT u.id, u.role FROM sessions s JOIN users u ON u.id = s.user_id \
         WHERE s.token_hash = ?1 \
           AND s.expires_at > datetime('now') \
           AND u.status = 'active'",
    )
    .bind(&token_hash)
    .fetch_optional(&app.pool)
    .await
    .ok()??;

    Some(Identity {
        id: row.get("id"),
        role: row.get("role"),
    })
}

/// 当前请求的登录用户 id。未登录、会话过期或账号被停用都返回 `None`。
#[cfg(feature = "ssr")]
pub async fn current_user_id() -> Option<i64> {
    current_identity().await.map(|identity| identity.id)
}

/// 当前用户是否管理员。
///
/// 这只用于决定**显示什么**。真正的防线在每个后台 server function 自己的权限校验里
/// ——接口是公开可达的，把按钮藏起来挡不住直接构造请求。
pub fn current_user_is_admin(user_state: &UserState) -> bool {
    matches!(user_state.get(), Some(Ok(Some(user))) if user.is_admin())
}

/// 按环境变量确保管理员账号存在。
///
/// 站长的账号不该由「开放注册」产生，所以用一次性的环境变量初始化：
/// `ADMIN_USERNAME` + `ADMIN_PASSWORD` 同时设置时建号并授管理员。
/// 已存在则只补角色，**不动密码**——否则每次启动都会把改过的密码重置回去。
#[cfg(feature = "ssr")]
pub async fn ensure_admin(pool: &sqlx::SqlitePool) -> anyhow::Result<()> {
    // 空字符串按「没设置」处理。容器里通常写成 `${ADMIN_USERNAME:-}`，没填时就是空串；
    // 若把空串当成有效值，下面 validate_credentials 会失败并让**进程直接起不来**——
    // 表现成「没配管理员反而服务启动失败」，很难往这个方向想。
    let non_empty = |key: &str| {
        std::env::var(key)
            .ok()
            .filter(|value| !value.trim().is_empty())
    };
    let (Some(username), Some(password)) = (non_empty("ADMIN_USERNAME"), non_empty("ADMIN_PASSWORD"))
    else {
        return Ok(());
    };

    let existing: Option<i64> =
        sqlx::query_scalar("SELECT id FROM users WHERE username = ?1 COLLATE NOCASE")
            .bind(&username)
            .fetch_optional(pool)
            .await?;

    match existing {
        Some(id) => {
            sqlx::query("UPDATE users SET role = 'admin' WHERE id = ?1")
                .bind(id)
                .execute(pool)
                .await?;
            tracing::info!("管理员账号已存在，仅确认角色：{username}");
        }
        None => {
            if let Err(msg) = validate_credentials(&username, &password) {
                anyhow::bail!("ADMIN_USERNAME / ADMIN_PASSWORD 不合法：{msg}");
            }
            let hash = crypto::hash_password(&password)?;
            sqlx::query(
                "INSERT INTO users (username, password_hash, display_name, role) \
                 VALUES (?1, ?2, ?3, 'admin')",
            )
            .bind(&username)
            .bind(&hash)
            .bind(&username)
            .execute(pool)
            .await?;
            tracing::info!("已创建管理员账号：{username}");
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// server function
// ---------------------------------------------------------------------------

/// 取当前登录用户。会话无效、已过期或账号被停用时都返回 `None`。
#[server]
pub async fn current_user() -> Result<Option<UserView>, ServerFnError> {
    use crate::state::AppState;
    use sqlx::Row;

    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");
    let Some(token) = cookies::session_token() else {
        return Ok(None);
    };
    let token_hash = crypto::hash_token(&token);

    let row = sqlx::query(
        "SELECT u.id, u.username, u.display_name, u.bio, u.role \
         FROM sessions s JOIN users u ON u.id = s.user_id \
         WHERE s.token_hash = ?1 \
           AND s.expires_at > datetime('now') \
           AND u.status = 'active'",
    )
    .bind(&token_hash)
    .fetch_optional(&app.pool)
    .await
    .map_err(|e| ServerFnError::new(format!("查询会话失败: {e}")))?;

    Ok(row.map(|row| UserView {
        id: row.get("id"),
        username: row.get("username"),
        display_name: row.get("display_name"),
        bio: row.get("bio"),
        role: row.get("role"),
    }))
}

/// 注册。成功后直接登录，省掉一次输入。
#[server]
pub async fn register(
    username: String,
    password: String,
    display_name: String,
) -> ActionResult {
    use crate::state::AppState;

    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");
    let username = username.trim().to_string();
    let display_name = match display_name.trim() {
        "" => username.clone(),
        name => name.to_string(),
    };

    if let Err(message) = validate_credentials(&username, &password) {
        return Ok(Err(message));
    }

    let taken: Option<i64> =
        sqlx::query_scalar("SELECT id FROM users WHERE username = ?1 COLLATE NOCASE")
            .bind(&username)
            .fetch_optional(&app.pool)
            .await
            .map_err(|e| ServerFnError::new(format!("查询用户名失败: {e}")))?;
    if taken.is_some() {
        return Ok(Err("这个用户名已经被用了。".to_string()));
    }

    let hash = crypto::hash_password(&password)
        .map_err(|e| ServerFnError::new(format!("生成密码哈希失败: {e}")))?;

    // 唯一约束仍然可能被并发注册撞上，这里把数据库的报错也翻译成同一句话
    let inserted = sqlx::query(
        "INSERT INTO users (username, password_hash, display_name) VALUES (?1, ?2, ?3)",
    )
    .bind(&username)
    .bind(&hash)
    .bind(&display_name)
    .execute(&app.pool)
    .await;
    if let Err(e) = inserted {
        leptos::logging::error!("插入用户失败: {e}");
        return Ok(Err("这个用户名已经被用了。".to_string()));
    }

    match authenticate_and_start_session(&app.pool, &username, &password).await {
        Ok(()) => Ok(Ok(())),
        Err(message) => Ok(Err(message)),
    }
}

/// 登录。
#[server]
pub async fn login(username: String, password: String) -> ActionResult {
    use crate::state::AppState;

    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");
    let username = username.trim().to_string();

    match authenticate_and_start_session(&app.pool, &username, &password).await {
        Ok(()) => Ok(Ok(())),
        Err(message) => Ok(Err(message)),
    }
}

/// 登出：删掉库里的会话行并清 cookie。
///
/// 只清 cookie 是不够的——令牌还能被别人的浏览器继续用；必须同时让它在库里失效。
#[server]
pub async fn logout() -> ActionResult {
    use crate::state::AppState;

    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");

    if let Some(token) = cookies::session_token() {
        let token_hash = crypto::hash_token(&token);
        if let Err(e) = sqlx::query("DELETE FROM sessions WHERE token_hash = ?1")
            .bind(&token_hash)
            .execute(&app.pool)
            .await
        {
            leptos::logging::error!("删除会话失败: {e}");
        }
    }

    cookies::clear_session();
    Ok(Ok(()))
}

/// 公开资料页需要的最小用户信息。
#[server]
pub async fn user_profile(username: String) -> Result<Option<UserView>, ServerFnError> {
    use crate::state::AppState;

    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");
    Ok(fetch_user_view(&app.pool, &username).await)
}

/// 修改密码。
///
/// 成功后**注销其它所有会话，但保留当前这个**：密码泄露的场景下，真正止住损失的
/// 动作是把别人已登录的会话踢掉；保留当前会话则免得用户改完还要重新登录。
#[server]
pub async fn change_password(old_password: String, new_password: String) -> ActionResult {
    use crate::state::AppState;
    use sqlx::Row;

    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");
    let Some(token) = cookies::session_token() else {
        return Ok(Err("请先登录。".to_string()));
    };
    let token_hash = crypto::hash_token(&token);

    let row = sqlx::query(
        "SELECT u.id, u.password_hash \
         FROM sessions s JOIN users u ON u.id = s.user_id \
         WHERE s.token_hash = ?1 \
           AND s.expires_at > datetime('now') \
           AND u.status = 'active'",
    )
    .bind(&token_hash)
    .fetch_optional(&app.pool)
    .await
    .map_err(|e| ServerFnError::new(format!("查询当前用户失败: {e}")))?;

    let Some(row) = row else {
        return Ok(Err("请先登录。".to_string()));
    };

    let user_id: i64 = row.get("id");
    let stored: String = row.get("password_hash");

    if !crypto::verify_password(&old_password, &stored) {
        return Ok(Err("当前密码不对。".to_string()));
    }
    if old_password == new_password {
        return Ok(Err("新密码与当前密码相同。".to_string()));
    }
    if let Err(message) = validate_password(&new_password) {
        return Ok(Err(message));
    }

    let hash = crypto::hash_password(&new_password)
        .map_err(|e| ServerFnError::new(format!("生成密码哈希失败: {e}")))?;

    sqlx::query("UPDATE users SET password_hash = ?1 WHERE id = ?2")
        .bind(&hash)
        .bind(user_id)
        .execute(&app.pool)
        .await
        .map_err(|e| ServerFnError::new(format!("更新密码失败: {e}")))?;

    sqlx::query("DELETE FROM sessions WHERE user_id = ?1 AND token_hash <> ?2")
        .bind(user_id)
        .bind(&token_hash)
        .execute(&app.pool)
        .await
        .map_err(|e| ServerFnError::new(format!("清理其它会话失败: {e}")))?;

    Ok(Ok(()))
}

/// 修改昵称与简介。
///
/// 昵称显示在顶栏与评论上，简介只显示在公开资料页；两者都由用户自己填，因此都要
/// 限长。空简介存成 NULL 而不是空串——「没写」和「写了个空字符串」是两回事。
#[server]
pub async fn update_profile(display_name: String, bio: String) -> ActionResult {
    use crate::state::AppState;

    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");
    let Some(user_id) = current_user_id().await else {
        return Ok(Err("请先登录。".to_string()));
    };

    let display_name = display_name.trim();
    if display_name.is_empty() {
        return Ok(Err("昵称不能为空。".to_string()));
    }

    let bio = bio.trim();
    if bio.chars().count() > BIO_MAX {
        return Ok(Err(format!("简介不能超过 {BIO_MAX} 个字符。")));
    }
    let bio_value = (!bio.is_empty()).then_some(bio);

    sqlx::query("UPDATE users SET display_name = ?1, bio = ?2 WHERE id = ?3")
        .bind(display_name)
        .bind(bio_value)
        .bind(user_id)
        .execute(&app.pool)
        .await
        .map_err(|e| ServerFnError::new(format!("更新资料失败: {e}")))?;

    Ok(Ok(()))
}
