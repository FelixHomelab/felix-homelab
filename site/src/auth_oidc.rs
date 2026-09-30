//! 主站经 Kanidm 的 OIDC 登录（与密码登录**并联**）。
//!
//! - `/auth/oidc/start`：生成 state + PKCE(verifier)，存短效 cookie，303 到 Kanidm；
//! - `/auth/oidc/callback`：校验 state，用授权码 + PKCE 换 token，拉 userinfo，
//!   以 `sub` 关联（首次按用户名绑定）既有站点账号，下发会话 cookie。
//!
//! 这些是普通 Axum 路由而不是 Leptos 页面：登录入口/回调要能直接返回 303 与
//! `Set-Cookie`，不需要经过 SSR 渲染。配置从环境变量读取（`.env` 经 quadlet
//! 的 `EnvironmentFile` 注入）：`SITE_OIDC_CLIENT_ID` / `SITE_OIDC_SECRET`。

use axum::extract::{Query, State};
use axum::http::header::{HeaderMap, HeaderValue, COOKIE, LOCATION, SET_COOKIE};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use serde::Deserialize;

use crate::auth::{create_session, COOKIE_SESSION, SESSION_DAYS};
use crate::state::AppState;

/// 短效 cookie：只服务于一次 OIDC 往返。
const COOKIE_STATE: &str = "gf_oidc_state";
const COOKIE_VERIFIER: &str = "gf_oidc_verifier";

/// 关联表里的 provider 标识。
const PROVIDER: &str = "kanidm";

fn issuer() -> String {
    std::env::var("SITE_OIDC_ISSUER")
        .unwrap_or_else(|_| "https://id.wraindrock.com".to_string())
        .trim_end_matches('/')
        .to_string()
}

fn client_id() -> String {
    std::env::var("SITE_OIDC_CLIENT_ID").unwrap_or_else(|_| "site".to_string())
}

fn client_secret() -> Result<String, String> {
    std::env::var("SITE_OIDC_SECRET")
        .map_err(|_| "服务器未配置 OIDC 密钥（SITE_OIDC_SECRET）。".to_string())
}

/// 固定为已注册的回调地址；本地调试可通过环境变量覆盖。
fn redirect_uri() -> String {
    std::env::var("SITE_OIDC_REDIRECT_URI")
        .unwrap_or_else(|_| "https://www.wraindrock.com/auth/oidc/callback".to_string())
}

fn secure_cookies() -> bool {
    redirect_uri().starts_with("https://")
}

fn random_token() -> String {
    let mut bytes = [0u8; 32];
    rand::fill(&mut bytes);
    hex::encode(bytes)
}

/// PKCE：code_challenge = base64url(SHA-256(verifier))，不带 padding。
fn pkce_challenge(verifier: &str) -> String {
    use base64::Engine;
    use sha2::{Digest, Sha256};
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// 短效 cookie（state / verifier）；回调时要清掉，所以把 Max-Age 也参数化。
fn short_cookie(name: &str, value: &str, max_age: i64) -> String {
    let secure = if secure_cookies() { "; Secure" } else { "" };
    format!("{name}={value}; Path=/; Max-Age={max_age}; HttpOnly; SameSite=Lax{secure}")
}

/// 会话 cookie：与 `auth::cookies` 同样的策略——配了 `COOKIE_DOMAIN` 时下发父域
/// cookie（Agent 子域 SSO 用），同时让主机内旧 cookie 过期，避免同名旧值带偏鉴权。
fn session_cookie(value: &str, max_age: i64, host: &str) -> Vec<String> {
    let secure = if secure_cookies() { "; Secure" } else { "" };
    let domain = std::env::var("COOKIE_DOMAIN").unwrap_or_default();
    if !domain.is_empty() && host.ends_with(&domain) {
        vec![
            format!(
                "{COOKIE_SESSION}={value}; Path=/; Max-Age={max_age}; HttpOnly; SameSite=Lax; Domain=.{domain}{secure}"
            ),
            format!("{COOKIE_SESSION}=; Path=/; Max-Age=0; HttpOnly; SameSite=Lax{secure}"),
        ]
    } else {
        vec![format!(
            "{COOKIE_SESSION}={value}; Path=/; Max-Age={max_age}; HttpOnly; SameSite=Lax{secure}"
        )]
    }
}

fn cookie_pairs(headers: &HeaderMap) -> Vec<(String, String)> {
    let Some(raw) = headers.get(COOKIE).and_then(|v| v.to_str().ok()) else {
        return Vec::new();
    };
    raw.split(';')
        .filter_map(|pair| {
            let (name, value) = pair.split_once('=')?;
            Some((name.trim().to_string(), value.trim().to_string()))
        })
        .collect()
}

fn html_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// 给用户看的失败页（登录链路不适合直接吐纯文本）。
fn error_page(status: StatusCode, message: &str) -> Response {
    let html = format!(
        "<!doctype html><html lang=\"zh-CN\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
         <title>登录失败 — Wraindrock</title></head>\
         <body style=\"font-family:system-ui,sans-serif;max-width:40rem;margin:4rem auto;padding:0 1rem;line-height:1.7\">\
         <h1 style=\"font-size:1.4rem\">登录失败</h1><p>{}</p>\
         <p><a href=\"/login\">返回登录页</a></p></body></html>",
        html_escape(message)
    );
    (status, Html(html)).into_response()
}

/// 生成带 `Set-Cookie` 的 303 响应。
fn redirect_with_cookies(location: &str, cookies: &[String]) -> Response {
    let mut response = Response::builder()
        .status(StatusCode::SEE_OTHER)
        .header(LOCATION, location)
        .header(axum::http::header::CACHE_CONTROL, "no-store")
        .body(axum::body::Body::empty())
        .expect("构造重定向响应");
    for cookie in cookies {
        if let Ok(value) = HeaderValue::from_str(cookie) {
            response.headers_mut().append(SET_COOKIE, value);
        }
    }
    response
}

/// `GET /auth/oidc/start`：种下 state/verifier 并跳转到 Kanidm 授权页。
pub async fn start(State(_state): State<AppState>, _headers: HeaderMap) -> Response {
    if let Err(message) = client_secret() {
        return error_page(StatusCode::SERVICE_UNAVAILABLE, &message);
    }

    let state = random_token();
    let verifier = random_token();
    let challenge = pkce_challenge(&verifier);

    let params = [
        ("client_id", client_id()),
        ("redirect_uri", redirect_uri()),
        ("response_type", "code".to_string()),
        ("scope", "openid profile email groups".to_string()),
        ("state", state.clone()),
        ("code_challenge", challenge),
        ("code_challenge_method", "S256".to_string()),
    ];
    let query = params
        .iter()
        .map(|(key, value)| format!("{key}={}", utf8_percent_encode(value, NON_ALPHANUMERIC)))
        .collect::<Vec<_>>()
        .join("&");
    let location = format!("{}/ui/oauth2?{query}", issuer());

    let cookies = vec![
        short_cookie(COOKIE_STATE, &state, 600),
        short_cookie(COOKIE_VERIFIER, &verifier, 600),
    ];
    redirect_with_cookies(&location, &cookies)
}

#[derive(Deserialize)]
pub struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

/// `GET /auth/oidc/callback`：校验 → 换 token → userinfo → 关联 → 建会话。
pub async fn callback(
    State(state): State<AppState>,
    Query(query): Query<CallbackQuery>,
    headers: HeaderMap,
) -> Response {
    if let Some(error) = query.error {
        let detail = query.error_description.unwrap_or_default();
        return error_page(
            StatusCode::BAD_REQUEST,
            &format!("Kanidm 返回了错误：{error} {detail}"),
        );
    }
    let Some(code) = query.code else {
        return error_page(StatusCode::BAD_REQUEST, "回调缺少授权码，请重试。");
    };

    let cookies = cookie_pairs(&headers);
    let cookie = |name: &str| {
        cookies
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
    };
    let expected_state = cookie(COOKIE_STATE).unwrap_or_default();
    if expected_state.is_empty() || Some(&expected_state) != query.state.as_ref() {
        return error_page(StatusCode::BAD_REQUEST, "登录状态校验失败，请重新发起登录。");
    }
    let verifier = cookie(COOKIE_VERIFIER).unwrap_or_default();
    if verifier.is_empty() {
        return error_page(StatusCode::BAD_REQUEST, "登录会话已过期，请重新发起登录。");
    }

    let secret = match client_secret() {
        Ok(secret) => secret,
        Err(message) => return error_page(StatusCode::SERVICE_UNAVAILABLE, &message),
    };

    let http = match reqwest::Client::builder()
        .user_agent("WraindrockSite/0.1 (+https://www.wraindrock.com)")
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            tracing::error!("构造 HTTP 客户端失败: {error}");
            return error_page(StatusCode::INTERNAL_SERVER_ERROR, "服务器出了点问题，稍后再试。");
        }
    };

    let token_response = http
        .post(format!("{}/oauth2/token", issuer()))
        .basic_auth(client_id(), Some(secret))
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code.as_str()),
            ("redirect_uri", redirect_uri().as_str()),
            ("code_verifier", verifier.as_str()),
        ])
        .send()
        .await;

    let token_json: serde_json::Value = match token_response {
        Ok(response) if response.status().is_success() => match response.json().await {
            Ok(json) => json,
            Err(error) => {
                tracing::error!("解析 token 响应失败: {error}");
                return error_page(StatusCode::BAD_GATEWAY, "登录服务返回异常，请重试。");
            }
        },
        Ok(response) => {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            tracing::error!("token 交换失败: {status} {body}");
            return error_page(StatusCode::BAD_GATEWAY, "登录凭据交换失败，请重试。");
        }
        Err(error) => {
            tracing::error!("请求 token 端点失败: {error}");
            return error_page(StatusCode::BAD_GATEWAY, "无法连接登录服务，请稍后再试。");
        }
    };
    let Some(access_token) = token_json["access_token"].as_str() else {
        tracing::error!("token 响应缺少 access_token: {token_json}");
        return error_page(StatusCode::BAD_GATEWAY, "登录服务返回异常，请重试。");
    };

    let userinfo = http
        .get(format!(
            "{}/oauth2/openid/{}/userinfo",
            issuer(),
            client_id()
        ))
        .bearer_auth(access_token)
        .send()
        .await;
    let info: serde_json::Value = match userinfo {
        Ok(response) if response.status().is_success() => match response.json().await {
            Ok(json) => json,
            Err(error) => {
                tracing::error!("解析 userinfo 失败: {error}");
                return error_page(StatusCode::BAD_GATEWAY, "登录服务返回异常，请重试。");
            }
        },
        Ok(response) => {
            let status = response.status();
            tracing::error!("userinfo 请求失败: {status}");
            return error_page(StatusCode::BAD_GATEWAY, "登录服务返回异常，请重试。");
        }
        Err(error) => {
            tracing::error!("请求 userinfo 失败: {error}");
            return error_page(StatusCode::BAD_GATEWAY, "无法连接登录服务，请稍后再试。");
        }
    };

    let Some(sub) = info["sub"].as_str() else {
        tracing::error!("userinfo 缺少 sub: {info}");
        return error_page(StatusCode::BAD_GATEWAY, "登录服务返回异常，请重试。");
    };
    let username = info["preferred_username"]
        .as_str()
        .or_else(|| info["name"].as_str())
        .unwrap_or_default()
        .to_string();

    let user_id = match link_or_find(&state.pool, sub, &username).await {
        Ok(id) => id,
        Err(message) => return error_page(StatusCode::FORBIDDEN, &message),
    };

    let token = match create_session(&state.pool, user_id).await {
        Ok(token) => token,
        Err(message) => return error_page(StatusCode::INTERNAL_SERVER_ERROR, &message),
    };

    let host = headers
        .get(axum::http::header::HOST)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .split(':')
        .next()
        .unwrap_or_default()
        .to_string();

    let mut cookies = session_cookie(&token, SESSION_DAYS * 24 * 60 * 60, &host);
    cookies.push(short_cookie(COOKIE_STATE, "", 0));
    cookies.push(short_cookie(COOKIE_VERIFIER, "", 0));
    redirect_with_cookies("/me", &cookies)
}

/// 找到（或首次绑定）站点账号：先按 (provider, sub)，再按用户名兜底。
async fn link_or_find(
    pool: &sqlx::SqlitePool,
    sub: &str,
    username: &str,
) -> Result<i64, String> {
    if let Some(user_id) = sqlx::query_scalar::<_, i64>(
        "SELECT user_id FROM oauth_identities WHERE provider = ?1 AND sub = ?2",
    )
    .bind(PROVIDER)
    .bind(sub)
    .fetch_optional(pool)
    .await
    .map_err(|error| {
        tracing::error!("查询 OIDC 关联失败: {error}");
        "服务器出了点问题，稍后再试。".to_string()
    })?
    {
        return Ok(user_id);
    }

    if username.is_empty() {
        return Err("登录服务没有返回用户名，请联系站长。".to_string());
    }

    use sqlx::Row;
    let row = sqlx::query("SELECT id, status FROM users WHERE username = ?1 COLLATE NOCASE")
        .bind(username)
        .fetch_optional(pool)
        .await
        .map_err(|error| {
            tracing::error!("查询站点账号失败: {error}");
            "服务器出了点问题，稍后再试。".to_string()
        })?;

    let Some(row) = row else {
        return Err(format!(
            "Kanidm 账号「{username}」还没有对应的站点账号。邀请码注册即将开放，或先联系站长开通。"
        ));
    };

    let status: String = row.get("status");
    if status != "active" {
        return Err("这个账号已被停用。".to_string());
    }
    let user_id: i64 = row.get("id");

    let _ = sqlx::query(
        "INSERT OR IGNORE INTO oauth_identities (user_id, provider, sub) VALUES (?1, ?2, ?3)",
    )
    .bind(user_id)
    .bind(PROVIDER)
    .bind(sub)
    .execute(pool)
    .await;

    Ok(user_id)
}
