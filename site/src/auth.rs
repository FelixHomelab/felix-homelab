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

    /// 从本次请求的 `Cookie` 头里取会话令牌（可能多个同名）。
    pub fn session_tokens() -> Vec<String> {
        let Some(parts) = use_context::<axum::http::request::Parts>() else {
            return Vec::new();
        };
        super::session_tokens_from_headers(&parts.headers)
    }

    /// 从本次请求的 `Cookie` 头里取会话令牌（第一个）。
    pub fn session_token() -> Option<String> {
        session_tokens().into_iter().next()
    }

    /// 无 Domain 的主机内 cookie（本地开发使用；清理旧值时也用它）。
    fn host_cookie(value: &str, max_age: i64) -> String {
        format!("{COOKIE_SESSION}={value}; Path=/; Max-Age={max_age}; HttpOnly; SameSite=Lax")
    }

    /// 当前请求 Host 是否属于配置的共享父域（决定要不要刷新父域 cookie）。
    pub fn has_shared_domain() -> bool {
        !domain_flag().is_empty()
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

    /// 本次请求的 Host（去掉端口、小写）。
    fn request_host() -> Option<String> {
        let parts = use_context::<axum::http::request::Parts>()?;
        let host = parts
            .headers
            .get(axum::http::header::HOST)?
            .to_str()
            .ok()?
            .split(':')
            .next()?
            .trim()
            .to_ascii_lowercase();
        (!host.is_empty()).then_some(host)
    }

    /// 请求是否来自本机回环（本地开发/直连）。
    fn is_loopback_host() -> bool {
        matches!(
            request_host().as_deref(),
            Some("localhost") | Some("127.0.0.1") | Some("::1") | Some("[::1]")
        )
    }

    /// 会话 cookie 的共享父域。
    ///
    /// 只有请求 Host 属于 `COOKIE_DOMAIN`（本身或其子域）时才加 `Domain=`：
    /// - `https://www.wraindrock.com` / `*.wraindrock.com` → `Domain=wraindrock.com`，
    ///   多租户 Agent 子域才能带上会话；
    /// - `http://localhost:5729` → 不加 Domain，本地开发照常登录
    ///   （否则浏览器会直接拒绝该 cookie）。
    fn domain_flag() -> String {
        let Some(domain) = std::env::var("COOKIE_DOMAIN")
            .ok()
            .map(|value| value.trim().trim_start_matches('.').to_ascii_lowercase())
            .filter(|value| !value.is_empty())
        else {
            return String::new();
        };
        let Some(host) = request_host() else {
            return String::new();
        };
        if host == domain || host.ends_with(&format!(".{domain}")) {
            format!("; Domain={domain}")
        } else {
            String::new()
        }
    }

    /// HTTPS 下带 `Secure`。
    ///
    /// 非回环 Host 只可能是云侧 HTTPS 入口（本机端口只绑 127.0.0.1）；
    /// 回环始终不加，保证 `http://localhost:5729` 本地登录不被浏览器丢弃。
    /// `COOKIE_SECURE` 保留为兼容开关（设置与否结果一致）。
    fn secure_flag() -> &'static str {
        if is_loopback_host() {
            ""
        } else {
            "; Secure"
        }
    }

    /// 下发会话 cookie。
    ///
    /// 请求 Host 属于 `COOKIE_DOMAIN` 时下发父域 cookie（Agent 子域 SSO 用），
    /// 并顺手让「主机内旧 cookie」过期：浏览器同名 cookie 会同时发送，旧值排
    /// 前面时会把鉴权带偏（`session_tokens_from_headers` 也会逐个尝试兜底）。
    pub fn set_session(token: &str) {
        let domain = domain_flag();
        if domain.is_empty() {
            append_cookie(host_cookie(token, SESSION_DAYS * 24 * 60 * 60));
        } else {
            append_cookie(format!(
                "{COOKIE_SESSION}={token}; Path=/; Max-Age={}; HttpOnly; SameSite=Lax{}{}",
                SESSION_DAYS * 24 * 60 * 60,
                domain,
                secure_flag()
            ));
            append_cookie(host_cookie("", 0));
        }
    }

    /// 清除会话 cookie（主机内与父域两种变体都清）。
    pub fn clear_session() {
        append_cookie(host_cookie("", 0));
        let domain = domain_flag();
        if !domain.is_empty() {
            append_cookie(format!(
                "{COOKIE_SESSION}=; Path=/; Max-Age=0; HttpOnly; SameSite=Lax{}{}",
                domain,
                secure_flag()
            ));
        }
    }
}

/// 为指定用户建会话并返回明文令牌。
///
/// 供 server function（密码登录）与普通 Axum 处理器（OIDC 回调）共用；
/// 调用方决定怎么把令牌写进 cookie（Leptos 上下文里用 `cookies::set_session`，
/// 普通处理器自己拼 `Set-Cookie`）。
#[cfg(feature = "ssr")]
pub(crate) async fn create_session(
    pool: &sqlx::SqlitePool,
    user_id: i64,
) -> Result<String, String> {
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

    // 把账号里的外观偏好镜像进 cookie。主题的读取路径是同步的，cookie 就是账号偏好的
    // 缓存；镜像失败只影响「这次登录后的外观」，不该让登录本身失败。
    if let Err(error) = crate::theme::mirror_account_prefs(pool, user_id).await {
        leptos::logging::warn!("镜像账号外观偏好失败: {error}");
    }

    Ok(token)
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
    totp: Option<&str>,
) -> Result<(), String> {
    use sqlx::Row;

    let row = sqlx::query(
        "SELECT id, username, password_hash, status,                 EXISTS(SELECT 1 FROM oauth_identities oi                        WHERE oi.user_id = users.id AND oi.provider = 'kanidm') AS linked          FROM users WHERE username = ?1 COLLATE NOCASE",
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

    let status: String = row.get("status");
    if status != "active" {
        return Err("这个账号已被停用。".to_string());
    }

    let linked: bool = row.get("linked");
    if linked {
        // 统一账号：凭据以全站唯一账号为准（对用户无感，本站登录就是它的登录）。
        // 密码不再落本站库；动态验证码（如管理员启用）走第二个输入框。
        let ident: String = row.get("username");
        verify_with_kanidm(&ident, password, totp).await?;
    } else {
        // 过渡期：尚未接入统一账号的老用户仍用本站密码（后续随激活流程迁移）
        let stored: String = row.get("password_hash");
        if !crypto::verify_password(password, &stored) {
            // 与「用户不存在」返回同一句话，避免泄露某个用户名是否已注册
            return Err("用户名或密码不对。".to_string());
        }
    }

    let user_id: i64 = row.get("id");
    let token = create_session(pool, user_id).await?;
    cookies::set_session(&token);
    Ok(())
}

/// 向全站统一账号（内部实现，用户不可见）校验用户名/密码（+动态验证码）。
///
/// 走 HTTP 认证会话：init2 → begin → 逐项凭据（TOTP/密码）→ success。
/// 这条链路与站点自己的 CLI/官方客户端同源；密码只在本请求内转发，不落任何日志。
#[cfg(feature = "ssr")]
async fn verify_with_kanidm(ident: &str, password: &str, totp: Option<&str>) -> Result<(), String> {
    use serde_json::{json, Value};

    fn service_error(context: &str, error: impl std::fmt::Display) -> String {
        tracing::error!("{context}: {error}");
        "登录服务暂时不可用，请稍后再试。".to_string()
    }

    let base = std::env::var("SITE_KANIDM_URL")
        .unwrap_or_else(|_| "https://id.wraindrock.com".to_string());
    let auth_url = format!("{}/v1/auth", base.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .user_agent("WraindrockSite/0.1 (+https://www.wraindrock.com)")
        .timeout(std::time::Duration::from_secs(12))
        .build()
        .map_err(|error| service_error("初始化统一账号客户端失败", error))?;

    // 1) 初始化认证会话（会话 id 在响应头）
    let response = client
        .post(&auth_url)
        .json(&json!({
            "step": { "init2": { "username": ident, "issue": "token", "privileged": false } }
        }))
        .send()
        .await
        .map_err(|error| service_error("统一账号认证初始化失败", error))?;
    if !response.status().is_success() {
        let status = response.status();
        return Err(service_error("统一账号认证初始化返回异常", status));
    }
    let session = response
        .headers()
        .get("x-kanidm-auth-session-id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .ok_or_else(|| {
            tracing::error!("统一账号认证响应缺少会话头");
            "登录服务暂时不可用，请稍后再试。".to_string()
        })?;
    let mut state: Value = response
        .json::<Value>()
        .await
        .map_err(|error| service_error("解析统一账号认证响应失败", error))?
        .get("state")
        .cloned()
        .unwrap_or(Value::Null);
    let mech = pick_password_mech(&state).ok_or_else(|| {
        "这个账号不支持密码登录，请联系站长。".to_string()
    })?;

    // 2) 开始密码认证
    state = auth_post(&client, &auth_url, &session, json!({ "step": { "begin": mech } }))
        .await?
        .get("state")
        .cloned()
        .unwrap_or(Value::Null);

    // 3) 逐项提交凭据；密码只提交一次，避免异常循环
    let mut password_sent = false;
    let mut last_sent = "";
    for _ in 0..5 {
        if state.get("success").is_some() {
            return Ok(());
        }
        if let Some(reason) = state.get("denied").and_then(Value::as_str) {
            tracing::warn!("统一账号拒绝登录（{reason}）");
            return Err(match last_sent {
                "totp" => "动态验证码不对或已过期。",
                "password" => "用户名或密码不对。",
                _ => "登录失败，请重试。",
            }
            .to_string());
        }

        let allowed = state
            .get("continue")
            .or_else(|| state.get("choose"))
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        if allowed.iter().any(|item| item == "totp") {
            let Some(code) = totp.map(str::trim).filter(|value| !value.is_empty()) else {
                return Err("请输入动态验证码后重试。".to_string());
            };
            let code: u32 = code
                .parse()
                .map_err(|_| "动态验证码应为 6 位数字。".to_string())?;
            last_sent = "totp";
            state = auth_post(
                &client,
                &auth_url,
                &session,
                json!({ "step": { "cred": { "totp": code } } }),
            )
            .await?
            .get("state")
            .cloned()
            .unwrap_or(Value::Null);
        } else if allowed.iter().any(|item| item == "password") {
            if password_sent {
                return Err("登录流程异常，请稍后再试。".to_string());
            }
            password_sent = true;
            last_sent = "password";
            state = auth_post(
                &client,
                &auth_url,
                &session,
                json!({ "step": { "cred": { "password": password } } }),
            )
            .await?
            .get("state")
            .cloned()
            .unwrap_or(Value::Null);
        } else if allowed.iter().any(|item| item == "backupcode") {
            let Some(code) = totp.map(str::trim).filter(|value| !value.is_empty()) else {
                return Err("请输入动态验证码或备用码后重试。".to_string());
            };
            last_sent = "totp";
            state = auth_post(
                &client,
                &auth_url,
                &session,
                json!({ "step": { "cred": { "backupcode": code } } }),
            )
            .await?
            .get("state")
            .cloned()
            .unwrap_or(Value::Null);
        } else {
            return Err("这个账号需要其它验证方式，暂不支持在网页登录。".to_string());
        }
    }
    Err("登录流程异常，请稍后再试。".to_string())
}

/// 从 `choose` 列表里选可用机制：优先"密码+MFA"，其次"仅密码"。
#[cfg(feature = "ssr")]
fn pick_password_mech(state: &serde_json::Value) -> Option<&'static str> {
    let choose = state.get("choose")?.as_array()?;
    let has = |name: &str| choose.iter().any(|item| item.as_str() == Some(name));
    if has("passwordmfa") {
        Some("passwordmfa")
    } else if has("password") {
        Some("password")
    } else {
        None
    }
}

/// 统一的认证步骤 POST：带会话头、解析 JSON、把异常折叠成用户可读错误。
#[cfg(feature = "ssr")]
async fn auth_post(
    client: &reqwest::Client,
    url: &str,
    session: &str,
    body: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let response = client
        .post(url)
        .header("x-kanidm-auth-session-id", session)
        .json(&body)
        .send()
        .await
        .map_err(|error| {
            tracing::error!("统一账号认证请求失败: {error}");
            "登录服务暂时不可用，请稍后再试。".to_string()
        })?;
    if !response.status().is_success() {
        let status = response.status();
        tracing::error!("统一账号认证步骤返回异常: {status}");
        return Err("登录服务暂时不可用，请稍后再试。".to_string());
    }
    response.json().await.map_err(|error| {
        tracing::error!("解析统一账号认证响应失败: {error}");
        "登录服务暂时不可用，请稍后再试。".to_string()
    })
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
    session_tokens_from_headers(headers).into_iter().next()
}

/// 从 `Cookie` 头里取**所有**同名会话令牌，按出现顺序。
///
/// 浏览器可能同时带着旧的主机内 cookie 与新的父域 cookie（同名两个）：
/// 只取第一个会在「第一个是旧值/已失效」时误判未登录，因此鉴权要逐个尝试。
#[cfg(feature = "ssr")]
pub fn session_tokens_from_headers(headers: &axum::http::HeaderMap) -> Vec<String> {
    let Some(header) = headers
        .get(axum::http::header::COOKIE)
        .and_then(|value| value.to_str().ok())
    else {
        return Vec::new();
    };

    header
        .split(';')
        .filter_map(|pair| {
            let (name, value) = pair.split_once('=')?;
            (name.trim() == COOKIE_SESSION).then(|| value.trim().to_string())
        })
        .filter(|value| !value.is_empty())
        .collect()
}

/// 解析当前登录用户 id 的**无上下文**版本。
#[cfg(feature = "ssr")]
pub async fn user_id_from_headers(
    pool: &sqlx::SqlitePool,
    headers: &axum::http::HeaderMap,
) -> Option<i64> {
    for token in session_tokens_from_headers(headers) {
        let token_hash = crypto::hash_token(&token);
        let id: Option<i64> = sqlx::query_scalar(
            "SELECT u.id FROM sessions s JOIN users u ON u.id = s.user_id \
             WHERE s.token_hash = ?1 \
               AND s.expires_at > datetime('now') \
               AND u.status = 'active'",
        )
        .bind(&token_hash)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();
        if id.is_some() {
            return id;
        }
    }
    None
}

/// 解析当前登录用户 `(id, 用户名, 角色)` 的**无上下文**版本。
///
/// 多租户 Agent 的网关鉴权（`/api/agent/auth`）是普通 Axum 处理器，拿不到
/// Leptos context，所以这里返回用户名本身用于与子域对应。
#[cfg(feature = "ssr")]
pub async fn user_from_headers(
    pool: &sqlx::SqlitePool,
    headers: &axum::http::HeaderMap,
) -> Option<(i64, String, String)> {
    use sqlx::Row;

    for token in session_tokens_from_headers(headers) {
        let token_hash = crypto::hash_token(&token);
        let row = sqlx::query(
            "SELECT u.id, u.username, u.role FROM sessions s JOIN users u ON u.id = s.user_id \
             WHERE s.token_hash = ?1 \
               AND s.expires_at > datetime('now') \
               AND u.status = 'active'",
        )
        .bind(&token_hash)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();
        if let Some(row) = row {
            return Some((row.get("id"), row.get("username"), row.get("role")));
        }
    }
    None
}

/// 自愈：请求 Host 属于共享父域时，把会话 cookie 刷新为父域版
/// （并清掉旧的主机内 cookie）。老用户访问一次主站即可把会话带进
/// Agent 子域，无需手动重登或清缓存。
#[cfg(feature = "ssr")]
fn refresh_session_cookie_if_shared(token: &str) {
    if cookies::has_shared_domain() && use_context::<leptos_axum::ResponseOptions>().is_some() {
        cookies::set_session(token);
    }
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

    for token in session_tokens_from_headers(&parts.headers) {
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
        .ok()
        .flatten();
        if let Some(row) = row {
            refresh_session_cookie_if_shared(&token);
            return Some(Identity {
                id: row.get("id"),
                role: row.get("role"),
            });
        }
    }
    None
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

    for token in cookies::session_tokens() {
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

        if let Some(row) = row {
            refresh_session_cookie_if_shared(&token);
            return Ok(Some(UserView {
                id: row.get("id"),
                username: row.get("username"),
                display_name: row.get("display_name"),
                bio: row.get("bio"),
                role: row.get("role"),
            }));
        }
    }

    Ok(None)
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

    match authenticate_and_start_session(&app.pool, &username, &password, None).await {
        Ok(()) => Ok(Ok(())),
        Err(message) => Ok(Err(message)),
    }
}

/// 登录。
#[server]
pub async fn login(username: String, password: String, totp: Option<String>) -> ActionResult {
    use crate::state::AppState;

    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");
    let username = username.trim().to_string();

    match authenticate_and_start_session(&app.pool, &username, &password, totp.as_deref()).await {
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

    // 浏览器可能同时带着旧的主机内 cookie 与父域 cookie：逐个注销，
    // 否则「登出」后另一个 cookie 仍然有效，用户会以为没登出成功。
    for token in cookies::session_tokens() {
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
