//! 多租户 AI Agent（P1 试点）。
//!
//! 职责划分：**站点是控制面，宿主脚本是执行面**。
//!
//! - 站点：订阅表（`agent_subscriptions`，一个用户多个实例 slot）、实例域名
//!   （`subdomain`，形如 `r4nd0m.<用户名>.<agent名>.agent.<域名>`）、网关鉴权
//!   `/api/agent/auth`（含**按需唤醒**）、后台开通页与用户首屏入口；
//! - 宿主：`scripts/agent-ctl.sh` 由 systemd `.path`/`.timer` 触发，负责容器
//!   生命周期（含**空闲睡眠**与**撤销 30 天后回收**）与 Caddy 路由；
//!   运行态回写 `/agents/status.json`（按 subdomain 索引）。
//!
//! 唤醒：网关收到请求时若实例在睡眠，站点落一个 `start` 请求并短轮询状态，
//! 用户侧只表现为「首次打开稍慢几秒」，无需知道背后的停机/启动。

use leptos::prelude::*;
use serde::{Deserialize, Serialize};

/// 支持的模板类型（仅服务端校验用）。
#[cfg(feature = "ssr")]
const AGENT_KINDS: [&str; 2] = ["opencode", "dsh"];

/// 单个用户可开通的实例数上限（P1 保护宿主资源）。
#[cfg(feature = "ssr")]
const AGENT_MAX_SLOTS: i64 = 9;

/// 撤销后保留（数据 + 域名）的宽限期：期间续期可原样复活。
#[cfg(feature = "ssr")]
const AGENT_GRACE_DAYS: i64 = 30;

/// 唤醒时等待宿主启动容器的上限（毫秒）。
#[cfg(feature = "ssr")]
const WAKE_TIMEOUT_MS: u64 = 40_000;

/// 模板的中文名（页面展示）。
pub fn agent_kind_label(kind: &str) -> &'static str {
    match kind {
        "opencode" => "OpenCode",
        "dsh" => "DeepSeek Harness",
        _ => "Agent",
    }
}

/// 用户名 → 域名标签：小写、`_`→`-`（DNS 标签/证书不接受下划线；仅服务端使用）。
#[cfg(feature = "ssr")]
fn username_slug(username: &str) -> String {
    username.trim().to_ascii_lowercase().replace('_', "-")
}

/// 生成随机子域码：18 位小写字母 + 数字（DNS 一级标签，避免被枚举）。
#[cfg(feature = "ssr")]
fn random_code() -> String {
    const CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut bytes = [0u8; 18];
    rand::fill(&mut bytes);
    bytes
        .iter()
        .map(|b| CHARS[(*b as usize) % CHARS.len()] as char)
        .collect()
}

/// 宿主回写的运行态（来自 `/agents/status.json`，按 subdomain 索引）。
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct AgentRuntime {
    /// running / exited / created / …
    pub state: String,
    /// 宿主意愿：started / sleeping / stopped / removed
    pub desired: String,
    /// 宿主探测：发布端口已在宿主回环上响应 HTTP（唤醒放行条件）
    pub ready: bool,
    pub health: Option<String>,
    pub kind: String,
    pub port: i64,
    /// DSH 的启动令牌（仅 dsh 且运行中时有值；由宿主从容器日志提取）
    pub token: Option<String>,
    /// 已配置的凭据引用名（只读名字，值永远不回传）
    pub keys: Vec<String>,
}

/// Agent 一行的完整视图（后台/用户首屏共用）。
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct AgentRow {
    pub id: i64,
    pub username: String,
    pub display_name: String,
    pub slot: i64,
    pub kind: String,
    pub status: String,
    pub subdomain: String,
    pub expires_at: Option<String>,
    /// 撤销时间（宽限期内可续期复活）
    pub revoked_at: Option<String>,
    /// 彻底删除时间（非空即「彻底删除记录」，行保留 30 天供回溯）
    pub purged_at: Option<String>,
    pub note: String,
    pub created_at: String,
    pub updated_at: String,
    /// 用户入口（https://<subdomain>.<域名>）
    pub url: String,
    /// 首次进入用的带令牌链接（仅 DSH）
    pub login_url: Option<String>,
    pub runtime: Option<AgentRuntime>,
}

// ---------------------------------------------------------------------------
// 仅服务端：域名、鉴权、唤醒、请求文件、状态
// ---------------------------------------------------------------------------

#[cfg(feature = "ssr")]
const AGENTS_DIR: &str = "/agents";

/// 网关（Caddy forward_auth）查询参数。
#[derive(Deserialize)]
pub struct AgentAuthQuery {
    /// 站点用户名（原始写法，大小写不敏感匹配）
    pub user: String,
    /// 实例号；路由文件总是显式带上
    #[serde(default = "default_slot")]
    pub slot: i64,
    /// VNC 子请求标记（VNC 有自己的鉴权路径，不参与 DSH 令牌跳转）
    #[serde(default)]
    pub vnc: Option<String>,
    /// 原始请求 URI（forward_auth 用占位符转发过来；形如 `/?token=xyz`）
    #[serde(default)]
    pub orig: Option<String>,
}

/// 是否是浏览器地址栏/链接的页面导航（而非子资源请求）。
#[cfg(feature = "ssr")]
fn is_document_request(headers: &axum::http::HeaderMap) -> bool {
    let get = |name: &str| {
        headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("")
    };
    if get("sec-fetch-dest").eq_ignore_ascii_case("document")
        || get("sec-fetch-mode").eq_ignore_ascii_case("navigate")
    {
        return true;
    }
    get("accept").contains("text/html")
}

/// 是否已带 DSH 自己的登录 Cookie（`dsh-auth-*` → 已登录，勿再重定向）。
#[cfg(feature = "ssr")]
fn has_dsh_session(headers: &axum::http::HeaderMap) -> bool {
    headers
        .get(axum::http::header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|cookies| cookies.contains("dsh-auth-"))
}

/// 请求的 Referer 是否与 Agent 自身同域（站内二次导航 → 不重定向）。
#[cfg(feature = "ssr")]
fn referer_is_same_host(headers: &axum::http::HeaderMap) -> bool {
    let host = headers
        .get(axum::http::header::HOST)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    let referer = headers
        .get(axum::http::header::REFERER)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    if referer.is_empty() {
        return false;
    }
    let referer_host = referer
        .split("//")
        .nth(1)
        .unwrap_or("")
        .split('/')
        .next()
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    !referer_host.is_empty() && referer_host == host
}

/// Caddy on-demand TLS 的 ask 查询参数。
#[derive(Deserialize)]
pub struct AgentTlsAskQuery {
    /// 待签发证书的完整域名（云侧 Caddy 自动带上）
    pub domain: String,
}

/// serde 默认实例号：旧路由（不带 slot）视为实例 1。
///
/// 不能加 `#[cfg(feature = "ssr")]`：这个结构体在 hydrate 端也会编译，
/// 反序列化默认值函数必须两端存在。
pub fn default_slot() -> i64 {
    1
}

#[cfg(feature = "ssr")]
fn base_domain() -> String {
    std::env::var("AGENT_BASE_DOMAIN")
        .unwrap_or_else(|_| "agent.localhost".to_string())
        .trim()
        .trim_start_matches('.')
        .to_ascii_lowercase()
}

/// 实例入口 URL（本地开发用 http，公网用 https）。
#[cfg(feature = "ssr")]
fn agent_url(subdomain: &str) -> String {
    let domain = base_domain();
    let scheme = if domain.ends_with("localhost") {
        "http"
    } else {
        "https"
    };
    format!("{scheme}://{subdomain}.{domain}")
}

/// 是否已是新版子域格式（18 位小写字母/数字）。
#[cfg(feature = "ssr")]
fn is_new_subdomain(sub: &str) -> bool {
    sub.len() == 18 && sub.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
}

/// 生成一个全表唯一的子域：18 位随机码（一级标签，公开形态为 `<码>.wraindrock.com`；
/// 本地开发为 `<码>.agent.localhost`）。
///
/// 不再把用户名/Agent 类型编进域名：新增 Agent 类型无需改动域名方案，
/// 码本身足够随机、且按实例唯一记录在订阅表里。
#[cfg(feature = "ssr")]
async fn unique_subdomain(pool: &sqlx::SqlitePool) -> Result<String, String> {
    for _ in 0..8 {
        let candidate = random_code();
        let exists: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM agent_subscriptions WHERE subdomain = ?1")
                .bind(&candidate)
                .fetch_optional(pool)
                .await
                .map_err(|e| format!("检查域名唯一性失败: {e}"))?;
        if exists.is_none() {
            return Ok(candidate);
        }
    }
    Err("生成唯一域名失败，请重试。".to_string())
}

/// 启动迁移/回填：无子域或旧格式（旧的三段式 `<随机>.<用户名>.<类型>`）一律升级为
/// 18 位随机码；未撤销的实例补一个 grant 请求，让宿主更新状态、重建容器
/// （DSH 的 --trusted-host 在启动参数里）并刷新 Caddy 路由。
#[cfg(feature = "ssr")]
pub async fn ensure_subdomains(pool: &sqlx::SqlitePool) -> anyhow::Result<()> {
    use sqlx::Row;

    let rows = sqlx::query(
        "SELECT s.id, s.slot, s.subdomain, s.status, s.kind, u.username \
         FROM agent_subscriptions s JOIN users u ON u.id = s.user_id ORDER BY s.id ASC",
    )
    .fetch_all(pool)
    .await?;

    for row in rows {
        let old: Option<String> = row.get("subdomain");
        if old.as_deref().is_some_and(is_new_subdomain) {
            continue;
        }
        let id: i64 = row.get("id");
        let slot: i64 = row.get("slot");
        let status: String = row.get("status");
        let kind: String = row.get("kind");
        let username: String = row.get("username");
        let subdomain = unique_subdomain(pool)
            .await
            .map_err(anyhow::Error::msg)?;
        sqlx::query("UPDATE agent_subscriptions SET subdomain = ?1 WHERE id = ?2")
            .bind(&subdomain)
            .bind(id)
            .execute(pool)
            .await?;
        tracing::info!(
            "Agent 子域迁移：{username} #{slot} {} -> {subdomain}",
            old.unwrap_or_default()
        );
        if status != "revoked" {
            // 让宿主重建容器并刷新路由（失败不阻塞启动；下次唤醒/续期会再补）
            if let Err(error) = write_agent_request(
                "grant",
                &username,
                slot,
                &kind,
                Some(&subdomain),
                None,
            ) {
                tracing::warn!("Agent 子域迁移请求失败：{error}");
            }
        }
    }
    Ok(())
}

/// 记录一次实例活动（供宿主判断空闲睡眠）；60 秒内不重复写。
#[cfg(feature = "ssr")]
fn touch_activity(subdomain: &str) {
    let dir = std::path::Path::new(AGENTS_DIR).join("activity");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let path = dir.join(subdomain);
    if let Ok(meta) = std::fs::metadata(&path) {
        if let Ok(modified) = meta.modified() {
            if modified.elapsed().map(|d| d.as_secs() < 60).unwrap_or(false) {
                return;
            }
        }
    }
    let _ = std::fs::write(&path, b"");
}

/// `/api/agent/auth`：Caddy 在每个 Agent 请求前调用。
///
/// 2xx 放行；401 未登录；402 到期；403 未开通/停用/非本人；
/// **睡眠中的实例会在这里被唤醒**（落 start 请求 + 短轮询），
/// 用户侧只表现为首次打开多等几秒。
#[cfg(feature = "ssr")]
pub async fn agent_auth(
    axum::extract::State(state): axum::extract::State<crate::state::AppState>,
    axum::extract::Query(query): axum::extract::Query<AgentAuthQuery>,
    headers: axum::http::HeaderMap,
) -> axum::response::Response {
    use axum::http::StatusCode;
    use axum::response::IntoResponse;
    use sqlx::Row;

    let site = std::env::var("SITE_URL").unwrap_or_else(|_| "/".to_string());
    let hint = |title: &str, body: &str| {
        format!(
            "<!doctype html><meta charset=\"utf-8\"><title>{title}</title>\
             <body style=\"font-family:system-ui;max-width:36rem;margin:15vh auto;padding:0 1rem\">\
             <h2>{title}</h2><p>{body}</p>\
             <p><a href=\"{site}\">{site}</a></p></body>"
        )
    };
    let waking = |title: &str, body: &str| {
        format!(
            "<!doctype html><meta charset=\"utf-8\"><meta http-equiv=\"refresh\" content=\"3\">\
             <title>{title}</title>\
             <body style=\"font-family:system-ui;max-width:36rem;margin:15vh auto;padding:0 1rem\">\
             <h2>{title}</h2><p>{body}</p><p class=\"muted\">页面会自动重试…</p></body>"
        )
    };
    let slot = query.slot.max(1);

    let Some((user_id, username, _role)) =
        crate::auth::user_from_headers(&state.pool, &headers).await
    else {
        return (
            StatusCode::UNAUTHORIZED,
            axum::response::Html(hint(
                "需要登录",
                "请先登录主站并确认账号状态正常，再回到当前地址。",
            )),
        )
            .into_response();
    };

    if !query.user.trim().eq_ignore_ascii_case(&username) {
        return (
            StatusCode::FORBIDDEN,
            axum::response::Html(hint("无权访问", "这个 Agent 属于其他账号。")),
        )
            .into_response();
    }

    let row = sqlx::query(
        "SELECT status, subdomain, kind, \
                CASE WHEN expires_at IS NOT NULL AND expires_at <= datetime('now') \
                     THEN 1 ELSE 0 END AS expired \
         FROM agent_subscriptions WHERE user_id = ?1 AND slot = ?2",
    )
    .bind(user_id)
    .bind(slot)
    .fetch_optional(&state.pool)
    .await
    .ok()
    .flatten();

    let Some(row) = row else {
        return (
            StatusCode::FORBIDDEN,
            axum::response::Html(hint("尚未开通", "这个账号还没有开通该 Agent，请联系管理员。")),
        )
            .into_response();
    };

    let status: String = row.get("status");
    let subdomain: String = row.get("subdomain");
    let kind: String = row.get("kind");
    let expired: i64 = row.get("expired");

    if expired == 1 {
        return (
            StatusCode::PAYMENT_REQUIRED,
            axum::response::Html(hint("使用期已到", "Agent 使用期已到，请联系管理员续费。")),
        )
            .into_response();
    }

    match status.as_str() {
        "active" => {}
        "stopped" => {
            return (
                StatusCode::FORBIDDEN,
                axum::response::Html(hint("已暂停", "Agent 已暂停，请联系管理员恢复。")),
            )
                .into_response();
        }
        _ => {
            return (
                StatusCode::FORBIDDEN,
                axum::response::Html(hint(
                    "尚未开通",
                    "这个账号还没有开通该 Agent，请联系管理员。",
                )),
            )
                .into_response();
        }
    }

    touch_activity(&subdomain);

    // 睡眠/未运行 → 唤醒并等待就绪
    //
    // 就绪由**宿主**判定：Agent 在独立 bridge 网络里，站点（Pod 内）探不到它的
    // 端口；宿主启动后会探测「发布在宿主回环的端口是否响应 HTTP」并写入
    // status.json 的 ready 字段，站点只轮询该字段。
    let ready = |runtime: Option<&AgentRuntime>| runtime.map(|r| r.ready).unwrap_or(false);
    let mut runtime = read_agent_status().get(&subdomain).cloned();
    if !ready(runtime.as_ref()) {
        if let Err(error) = write_agent_request("start", &username, slot, &kind, Some(&subdomain), None)
        {
            tracing::warn!("唤醒请求失败：{error}");
        }
        let mut waited = 0u64;
        while waited < WAKE_TIMEOUT_MS {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            waited += 500;
            runtime = read_agent_status().get(&subdomain).cloned();
            if ready(runtime.as_ref()) {
                break;
            }
        }
    }

    if ready(runtime.as_ref()) {
        // DSH 的令牌每次启动都会重新生成：睡眠期间卡片只能给出无令牌链接。
        // 对「页面导航」且不是站内二次跳转的请求：302 到最新令牌地址，让首次
        // 进入自动完成登录；子资源与站内导航直接放行（DSH 自己用 Cookie）。
        // DSH 令牌每次启动都会重新生成。判定顺序（页面导航场景）：
        //   · 带 DSH 登录 Cookie（登录完成后）→ 放行；
        //   · 原始 URL 里的 token 与当前令牌一致 → 放行（DSH 会消费它并种 Cookie）；
        //   · 否则 302 到最新令牌地址（`/?token=…`），让首次进入自动完成登录。
        // 原始 URI 由 agent-ctl 在 forward_auth 里用 `orig={http.request.uri}` 带过来。
        let provided = query
            .orig
            .as_deref()
            .and_then(|uri| uri.split("token=").nth(1))
            .map(|value| value.split(['&', '#']).next().unwrap_or(""))
            .unwrap_or("");
        let current = runtime.as_ref().and_then(|r| r.token.clone());
        let needs_token_redirect = kind == "dsh"
            && query.vnc.is_none()
            && is_document_request(&headers)
            && !referer_is_same_host(&headers)
            && !has_dsh_session(&headers)
            && current.as_deref() != Some(provided);
        if needs_token_redirect {
            // 令牌可能比 ready 晚几秒落盘：短轮询等待
            let mut token = current.clone().filter(|t| !t.is_empty());
            let mut waited = 0u64;
            while token.as_deref().unwrap_or("").is_empty() && waited < 20_000 {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                waited += 500;
                token = read_agent_status()
                    .get(&subdomain)
                    .and_then(|r| r.token.clone());
            }
            match token.filter(|t| !t.is_empty()) {
                Some(token) => {
                    let target = format!("{}?token={}", agent_url(&subdomain), token);
                    return axum::response::Redirect::to(&target).into_response();
                }
                None => {
                    return (
                        StatusCode::SERVICE_UNAVAILABLE,
                        axum::response::Html(waking(
                            "正在准备登录",
                            "实例已就绪，正在生成登录链接，请稍候。",
                        )),
                    )
                        .into_response();
                }
            }
        }
        StatusCode::NO_CONTENT.into_response()
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            axum::response::Html(waking("正在唤醒", "实例正在启动，请稍候几秒。")),
        )
            .into_response()
    }
}

/// `/api/agent/tls-ask`：云侧 Caddy on-demand TLS 的授权回调。
///
/// 只对「`<subdomain>.<AGENT_BASE_DOMAIN>` 且该 subdomain 在订阅表中存在」
/// 的域名返回 2xx。新增用户/实例无需改云侧配置，首次访问自动签发。
#[cfg(feature = "ssr")]
pub async fn agent_tls_ask(
    axum::extract::State(state): axum::extract::State<crate::state::AppState>,
    axum::extract::Query(query): axum::extract::Query<AgentTlsAskQuery>,
    headers: axum::http::HeaderMap,
) -> axum::http::StatusCode {
    use axum::http::StatusCode;

    // 只接受经本机回环来的调用（云侧 Caddy 的 ask 经 frp 隧道访问本机 Caddy）
    let host = headers
        .get(axum::http::header::HOST)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    if !matches!(host.as_str(), "127.0.0.1" | "localhost" | "::1" | "[::1]") {
        return StatusCode::FORBIDDEN;
    }

    let base = base_domain();
    let domain = query.domain.trim().to_ascii_lowercase();
    let suffix = format!(".{base}");
    let Some(subdomain) = domain.strip_suffix(&suffix) else {
        return StatusCode::FORBIDDEN;
    };
    if subdomain.is_empty() || subdomain.contains("..") {
        return StatusCode::FORBIDDEN;
    }

    let found: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM agent_subscriptions WHERE lower(subdomain) = ?1")
            .bind(subdomain)
            .fetch_optional(&state.pool)
            .await
            .ok()
            .flatten();
    if found.is_some() {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::FORBIDDEN
    }
}

/// 向宿主请求目录落一个动作文件（先写临时文件再改名，避免脚本读到半截 JSON）。
///
/// 返回最终文件路径：唤醒/凭据类请求由调用方轮询它是否被宿主处理（删除）。
/// 文件权限 0600（可能包含用户密钥）。
#[cfg(feature = "ssr")]
fn write_agent_request(
    action: &str,
    username: &str,
    slot: i64,
    kind: &str,
    subdomain: Option<&str>,
    secret: Option<(&str, &str)>,
) -> Result<std::path::PathBuf, String> {
    use std::time::{SystemTime, UNIX_EPOCH};

    let dir = std::path::Path::new(AGENTS_DIR).join("requests");
    std::fs::create_dir_all(&dir).map_err(|e| {
        format!("创建 {AGENTS_DIR}/requests 失败：{e}（请先执行 make install 挂载 agents 目录）")
    })?;

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let name = format!(
        "{}-s{slot}-{action}-{ts}.json",
        username_slug(username).replace('-', "_")
    );
    let mut payload = serde_json::json!({
        "action": action,
        "username": username,
        "slot": slot,
        "kind": kind,
        "ts": ts,
    });
    if let Some(subdomain) = subdomain {
        payload["subdomain"] = serde_json::Value::String(subdomain.to_string());
    }
    if let Some((key_name, key_value)) = secret {
        payload["key_name"] = serde_json::Value::String(key_name.to_string());
        payload["key_value"] = serde_json::Value::String(key_value.to_string());
    }

    let tmp = dir.join(format!(".{name}.tmp"));
    let final_path = dir.join(&name);
    std::fs::write(
        &tmp,
        serde_json::to_vec_pretty(&payload).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("写 Agent 请求文件失败：{e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
    }
    std::fs::rename(&tmp, &final_path).map_err(|e| format!("提交 Agent 请求文件失败：{e}"))?;
    Ok(final_path)
}

/// 读取宿主回写的运行态（按 subdomain 索引）。
#[cfg(feature = "ssr")]
fn read_agent_status() -> std::collections::HashMap<String, AgentRuntime> {
    let mut map = std::collections::HashMap::new();
    let path = std::path::Path::new(AGENTS_DIR).join("status.json");
    let Ok(text) = std::fs::read_to_string(path) else {
        return map;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return map;
    };
    let Some(agents) = value.get("agents").and_then(|v| v.as_object()) else {
        return map;
    };
    for (key, entry) in agents {
        map.insert(
            key.to_ascii_lowercase(),
            AgentRuntime {
                state: entry
                    .get("state")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string(),
                desired: entry
                    .get("desired")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                ready: entry
                    .get("ready")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                health: entry
                    .get("health")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
                kind: entry
                    .get("kind")
                    .and_then(|v| v.as_str())
                    .unwrap_or("opencode")
                    .to_string(),
                port: entry.get("port").and_then(|v| v.as_i64()).unwrap_or(0),
                token: entry
                    .get("token")
                    .and_then(|v| v.as_str())
                    .filter(|v| !v.is_empty())
                    .map(str::to_string),
                keys: entry
                    .get("keys")
                    .and_then(|v| v.as_array())
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(|item| item.as_str())
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default(),
            },
        );
    }
    map
}

/// 把一行订阅拼成视图（含运行态与入口）。
#[cfg(feature = "ssr")]
fn row_to_agent(row: &sqlx::sqlite::SqliteRow) -> AgentRow {
    use sqlx::Row;
    let subdomain: String = row.get("subdomain");
    let url = agent_url(&subdomain);
    let runtime = read_agent_status().get(&subdomain).cloned();
    let login_url = runtime
        .as_ref()
        .filter(|r| r.kind == "dsh")
        .and_then(|r| r.token.as_ref())
        .map(|token| format!("{url}?token={token}"));
    AgentRow {
        id: row.get("id"),
        url,
        login_url,
        subdomain,
        display_name: row.get("display_name"),
        username: row.get("username"),
        slot: row.get("slot"),
        kind: row.get("kind"),
        status: row.get("status"),
        expires_at: row.get("expires_at"),
        revoked_at: row.get("revoked_at"),
        purged_at: row.get("purged_at"),
        note: row.get("note"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        runtime,
    }
}

/// 订阅行查询的公共 SELECT 段。
#[cfg(feature = "ssr")]
const AGENT_SELECT: &str = "SELECT s.id, s.slot, s.subdomain, u.username, u.display_name, s.kind, s.status, \
     s.expires_at, s.revoked_at, s.purged_at, s.note, s.created_at, s.updated_at \
     FROM agent_subscriptions s JOIN users u ON u.id = s.user_id";

// ---------------------------------------------------------------------------
// 后台 server functions
// ---------------------------------------------------------------------------

/// 后台：列出 Agent 实例 + 宿主运行态。
///
/// `include_deleted=false`（默认）只显示仍有效的实例；已删除（撤销）与彻底删除
/// 记录默认隐藏，由后台开关切换显示。
///
/// 顺带做记录维护：
/// - 撤销超过宽限期的行转为「彻底删除记录」（宿主已回收资源，这里补记时间）；
/// - 彻底删除记录超过 30 天即清理。
#[server]
pub async fn admin_list_agents(include_deleted: bool) -> Result<Vec<AgentRow>, ServerFnError> {
    use crate::state::AppState;

    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");
    crate::roles::require_permission("agent")
        .await
        .map_err(ServerFnError::new)?;

    // 撤销宽限期已过 → 资源已被宿主回收，转为彻底删除记录（供回溯 30 天）
    let _ = sqlx::query(
        "UPDATE agent_subscriptions SET purged_at = datetime('now') \
         WHERE status = 'revoked' AND purged_at IS NULL \
           AND revoked_at IS NOT NULL AND revoked_at <= datetime('now', ?1)",
    )
    .bind(format!("-{AGENT_GRACE_DAYS} days"))
    .execute(&app.pool)
    .await;

    // 彻底删除记录保留 30 天
    let _ = sqlx::query(
        "DELETE FROM agent_subscriptions WHERE purged_at IS NOT NULL \
         AND purged_at <= datetime('now', ?1)",
    )
    .bind(format!("-{AGENT_GRACE_DAYS} days"))
    .execute(&app.pool)
    .await;

    let sql = if include_deleted {
        format!("{AGENT_SELECT} ORDER BY s.created_at DESC, s.id DESC LIMIT 500")
    } else {
        format!(
            "{AGENT_SELECT} WHERE s.status != 'revoked' AND s.purged_at IS NULL \
             ORDER BY s.created_at DESC, s.id DESC LIMIT 500"
        )
    };
    let rows = sqlx::query(&sql)
        .fetch_all(&app.pool)
        .await
        .map_err(|e| ServerFnError::new(format!("查询 Agent 订阅失败: {e}")))?;

    Ok(rows.iter().map(row_to_agent).collect())
}

/// 用户首屏：当前登录账号自己的 Agent 入口（未登录返回空表）。
/// 时长池视图：某统计窗口内的总时长与剩余时长（秒）。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TimePool {
    /// 统计窗口：all / month / week / day
    pub window: String,
    pub total_seconds: i64,
    pub remaining_seconds: i64,
}

/// 查询账号时长池（充值入账与剩余）。
///
/// 计量/充值系统尚未落地：当前统一返回 0，前端显示空态进度条；
/// 落地后按 `window`（全部/本月/本周/今天）从时间池流水聚合。
#[server]
pub async fn agent_time_pool(window: String) -> Result<TimePool, ServerFnError> {
    Ok(TimePool {
        window,
        total_seconds: 0,
        remaining_seconds: 0,
    })
}

#[server]
pub async fn my_agents() -> Result<Vec<AgentRow>, ServerFnError> {
    use crate::state::AppState;

    let Some(identity) = crate::auth::current_identity().await else {
        return Ok(Vec::new());
    };
    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");

    let sql = format!(
        "{AGENT_SELECT} WHERE s.user_id = ?1 AND s.status != 'revoked' AND s.purged_at IS NULL \
         ORDER BY s.slot ASC LIMIT 50"
    );
    let rows = sqlx::query(&sql)
        .bind(identity.id)
        .fetch_all(&app.pool)
        .await
        .map_err(|e| ServerFnError::new(format!("查询我的 Agent 失败: {e}")))?;

    Ok(rows.iter().map(row_to_agent).collect())
}

/// 后台：给一个账号开通/续费指定类型的 Agent 实例。
///
/// 复用顺序：**同类型未撤销实例 → 宽限期内已撤销实例（域名/数据原样复活）→
/// 新 slot**；`count` 为期望的实例数，其他类型不受影响。
#[server]
pub async fn admin_grant_agent(
    username: String,
    kind: String,
    count: i64,
    days: i64,
    note: String,
) -> Result<Result<(), String>, ServerFnError> {
    use crate::state::AppState;
    use sqlx::Row;

    if let Err(message) = crate::roles::require_permission("agent").await {
        return Ok(Err(message));
    }

    let kind = kind.trim().to_ascii_lowercase();
    if !AGENT_KINDS.contains(&kind.as_str()) {
        return Ok(Err(format!("暂不支持的模板：{kind}")));
    }
    let count = count.clamp(1, AGENT_MAX_SLOTS);
    let days = days.clamp(0, 3650);
    let note = note.trim().chars().take(200).collect::<String>();

    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");
    let user = sqlx::query("SELECT id, username FROM users WHERE username = ?1 COLLATE NOCASE")
        .bind(username.trim())
        .fetch_optional(&app.pool)
        .await
        .map_err(|e| ServerFnError::new(format!("查询用户失败: {e}")))?;

    let Some(user) = user else {
        return Ok(Err("找不到这个用户。".to_string()));
    };
    let user_id: i64 = user.get("id");
    let actual_name: String = user.get("username");

    // 清理该用户超过宽限期的撤销行（域名/数据由宿主回收）
    let _ = sqlx::query(
        "DELETE FROM agent_subscriptions WHERE user_id = ?1 AND status = 'revoked' \
         AND revoked_at IS NOT NULL AND revoked_at <= datetime('now', ?2)",
    )
    .bind(user_id)
    .bind(format!("-{AGENT_GRACE_DAYS} days"))
    .execute(&app.pool)
    .await;

    // 只新建：占用 max_slot 之后的新 slot（续费/复活走行内「续费」）
    let max_slot: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(slot), 0) FROM agent_subscriptions WHERE user_id = ?1",
    )
    .bind(user_id)
    .fetch_one(&app.pool)
    .await
    .map_err(|e| ServerFnError::new(format!("查询现有实例失败: {e}")))?;

    if max_slot + count > AGENT_MAX_SLOTS {
        return Ok(Err(format!(
            "已达实例上限（{AGENT_MAX_SLOTS} 个），请先删除不用的实例。"
        )));
    }

    for offset in 1..=count {
        let slot = max_slot + offset;
        let subdomain = unique_subdomain(&app.pool)
            .await
            .map_err(ServerFnError::new)?;

        sqlx::query(
            "INSERT INTO agent_subscriptions \
                 (user_id, slot, subdomain, kind, status, expires_at, note, revoked_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, 'active', \
                     CASE WHEN ?5 = 1 THEN datetime('now', ?6) ELSE NULL END, ?7, NULL, datetime('now'))",
        )
        .bind(user_id)
        .bind(slot)
        .bind(&subdomain)
        .bind(&kind)
        .bind(if days > 0 { 1 } else { 0 })
        .bind(format!("+{days} days"))
        .bind(&note)
        .execute(&app.pool)
        .await
        .map_err(|e| ServerFnError::new(format!("写入订阅失败: {e}")))?;

        write_agent_request("grant", &actual_name, slot, &kind, Some(&subdomain), None)
            .map_err(ServerFnError::new)?;
    }

    Ok(Ok(()))
}

/// 后台：给某个实例续费（延长有效期）。
///
/// - 有效期内续费：在原到期时间上累加；
/// - 已过期或已撤销：从当前时间起算；**已撤销的会连同原域名与数据复活**
///   （30 天宽限期内，宿主接到 grant 请求后重建容器并恢复路由）；
/// - `days == 0` 表示改为长期有效；`note` 非空时更新备注。
#[server]
pub async fn admin_renew_agent(
    username: String,
    slot: i64,
    days: i64,
    note: String,
) -> Result<Result<(), String>, ServerFnError> {
    use crate::state::AppState;
    use sqlx::Row;

    if let Err(message) = crate::roles::require_permission("agent").await {
        return Ok(Err(message));
    }

    let slot = slot.max(1);
    let days = days.clamp(0, 3650);
    let note = note.trim().chars().take(200).collect::<String>();

    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");
    let row = sqlx::query(
        "SELECT u.username, s.kind, s.subdomain, s.status FROM agent_subscriptions s \
         JOIN users u ON u.id = s.user_id \
         WHERE u.username = ?1 COLLATE NOCASE AND s.slot = ?2",
    )
    .bind(username.trim())
    .bind(slot)
    .fetch_optional(&app.pool)
    .await
    .map_err(|e| ServerFnError::new(format!("查询实例失败: {e}")))?;

    let Some(row) = row else {
        return Ok(Err("该用户还没有这个 Agent 实例。".to_string()));
    };
    let actual_name: String = row.get("username");
    let kind: String = row.get("kind");
    let subdomain: String = row.get("subdomain");
    let was_revoked = row.get::<String, _>("status") == "revoked";

    sqlx::query(
        "UPDATE agent_subscriptions SET \
             status = CASE WHEN status = 'revoked' THEN 'active' ELSE status END, \
             expires_at = CASE WHEN ?1 = 0 THEN NULL \
                 ELSE datetime(CASE WHEN expires_at IS NOT NULL AND expires_at > datetime('now') \
                                    THEN expires_at ELSE datetime('now') END, ?2) END, \
             note = CASE WHEN ?3 = '' THEN note ELSE ?3 END, \
             revoked_at = NULL, \
             updated_at = datetime('now') \
         WHERE user_id = (SELECT id FROM users WHERE username = ?4 COLLATE NOCASE) AND slot = ?5",
    )
    .bind(if days > 0 { 1 } else { 0 })
    .bind(format!("+{days} days"))
    .bind(&note)
    .bind(&actual_name)
    .bind(slot)
    .execute(&app.pool)
    .await
    .map_err(|e| ServerFnError::new(format!("续费失败: {e}")))?;

    // 已撤销的实例需要宿主把容器/路由按原域名、原数据卷复活
    if was_revoked {
        write_agent_request("grant", &actual_name, slot, &kind, Some(&subdomain), None)
            .map_err(ServerFnError::new)?;
    }

    Ok(Ok(()))
}

/// 后台：启动 / 暂停 / 删除某账号的某个实例。
///
/// - `start`：订阅置回 active，请求宿主唤醒；
/// - `stop`：订阅置 stopped（网关拒绝，不自动唤醒）；
/// - `remove`：订阅置 revoked，请求宿主停容器、删路由；**数据与域名保留
///   30 天**（宿主的 state 里带 `removed_at`，到期回收）。
#[server]
pub async fn admin_agent_action(
    username: String,
    slot: i64,
    action: String,
) -> Result<Result<(), String>, ServerFnError> {
    use crate::state::AppState;
    use sqlx::Row;

    if let Err(message) = crate::roles::require_permission("agent").await {
        return Ok(Err(message));
    }

    let slot = slot.max(1);
    let (status, request_action) = match action.trim() {
        "start" => ("active", "start"),
        "stop" => ("stopped", "stop"),
        "remove" => ("revoked", "remove"),
        other => return Ok(Err(format!("未知操作：{other}"))),
    };

    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");
    let row = sqlx::query(
        "SELECT u.username, s.kind, s.subdomain FROM agent_subscriptions s \
         JOIN users u ON u.id = s.user_id \
         WHERE u.username = ?1 COLLATE NOCASE AND s.slot = ?2",
    )
    .bind(username.trim())
    .bind(slot)
    .fetch_optional(&app.pool)
    .await
    .map_err(|e| ServerFnError::new(format!("查询订阅失败: {e}")))?;

    let Some(row) = row else {
        return Ok(Err("该用户还没有这个 Agent 实例。".to_string()));
    };
    let actual_name: String = row.get("username");
    let kind: String = row.get("kind");
    let subdomain: String = row.get("subdomain");

    if request_action == "remove" {
        sqlx::query(
            "UPDATE agent_subscriptions SET status = 'revoked', revoked_at = datetime('now'), \
             updated_at = datetime('now') \
             WHERE user_id = (SELECT id FROM users WHERE username = ?1 COLLATE NOCASE) AND slot = ?2",
        )
        .bind(&actual_name)
        .bind(slot)
        .execute(&app.pool)
        .await
        .map_err(|e| ServerFnError::new(format!("更新订阅失败: {e}")))?;
    } else {
        sqlx::query(
            "UPDATE agent_subscriptions SET status = ?1, revoked_at = NULL, updated_at = datetime('now') \
             WHERE user_id = (SELECT id FROM users WHERE username = ?2 COLLATE NOCASE) AND slot = ?3",
        )
        .bind(status)
        .bind(&actual_name)
        .bind(slot)
        .execute(&app.pool)
        .await
        .map_err(|e| ServerFnError::new(format!("更新订阅失败: {e}")))?;
    }

    write_agent_request(
        request_action,
        &actual_name,
        slot,
        &kind,
        Some(&subdomain),
        None,
    )
    .map_err(ServerFnError::new)?;
    Ok(Ok(()))
}

/// 后台：**永久删除**一个已撤销的实例（容器/数据卷/工作区/路由立即回收）。
///
/// 用于版本测试清理、注销用户的数据清理等特殊情况。为防误触，必须原样手抄
/// `我确认永久删除<随机码>`（随机码 = 域名的第一段），服务端逐字校验。
///
/// 删除后在订阅表里留下「彻底删除记录」（`purged_at`），保留 30 天供回溯。
#[server]
pub async fn admin_purge_agent(
    username: String,
    slot: i64,
    confirmation: String,
) -> Result<Result<(), String>, ServerFnError> {
    use crate::state::AppState;
    use sqlx::Row;

    if let Err(message) = crate::roles::require_permission("agent").await {
        return Ok(Err(message));
    }

    let slot = slot.max(1);
    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");
    let row = sqlx::query(
        "SELECT u.username, s.kind, s.subdomain, s.status, s.purged_at \
         FROM agent_subscriptions s JOIN users u ON u.id = s.user_id \
         WHERE u.username = ?1 COLLATE NOCASE AND s.slot = ?2",
    )
    .bind(username.trim())
    .bind(slot)
    .fetch_optional(&app.pool)
    .await
    .map_err(|e| ServerFnError::new(format!("查询实例失败: {e}")))?;

    let Some(row) = row else {
        return Ok(Err("该用户还没有这个 Agent 实例。".to_string()));
    };
    let actual_name: String = row.get("username");
    let kind: String = row.get("kind");
    let subdomain: String = row.get("subdomain");
    let status: String = row.get("status");
    let purged_at: Option<String> = row.get("purged_at");

    if purged_at.is_some() {
        return Ok(Err("这个实例已经彻底删除过了。".to_string()));
    }
    if status != "revoked" {
        return Ok(Err("只能永久删除已删除（撤销）的实例；请先「删除」。".to_string()));
    }

    let code = subdomain.split('.').next().unwrap_or("").to_string();
    let expected = format!("我确认永久删除{code}");
    if confirmation.trim() != expected {
        return Ok(Err(format!(
            "确认文字不匹配：请原样输入「{expected}」。"
        )));
    }

    sqlx::query(
        "UPDATE agent_subscriptions SET purged_at = datetime('now'), updated_at = datetime('now') \
         WHERE user_id = (SELECT id FROM users WHERE username = ?1 COLLATE NOCASE) AND slot = ?2",
    )
    .bind(&actual_name)
    .bind(slot)
    .execute(&app.pool)
    .await
    .map_err(|e| ServerFnError::new(format!("记录彻底删除失败: {e}")))?;

    // 宿主立即回收容器/数据卷/工作区/路由/状态条目
    write_agent_request("purge", &actual_name, slot, &kind, Some(&subdomain), None)
        .map_err(ServerFnError::new)?;

    Ok(Ok(()))
}
