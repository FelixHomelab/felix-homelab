//! 背景图上传与提供（仅服务端）。
//!
//! 两条硬约束：
//!
//! 1. **不信客户端报的 content-type，只信文件魔数**。扩展名与 MIME 都由魔数决定，
//!    客户端声称是什么一概不作数。
//! 2. **只放行位图**（PNG / JPEG / GIF / WebP）。SVG 是文本、能内嵌脚本，作为
//!    用户可上传的内容是明确的 XSS 入口，直接不接受。
//!
//! 落盘形如 `data/uploads/<user_id>/<随机>.<ext>`，对外由 `/uploads/...` 提供。

use axum::extract::{Multipart, Path, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};

use crate::state::AppState;

/// 上传体积上限（2 MiB）。背景图不需要更大。
const MAX_UPLOAD_BYTES: usize = 2 * 1024 * 1024;

/// 路由层的请求体上限，略大于单文件上限，留出 multipart 分隔符的余量。
pub const BODY_LIMIT: usize = MAX_UPLOAD_BYTES + 64 * 1024;

/// 对外 URL 前缀。`theme` 模块据此区分「上传的图」与「外链」。
pub const URL_PREFIX: &str = "/uploads/";

/// 上传目录，可用 `UPLOAD_DIR` 覆盖。
pub fn upload_root() -> std::path::PathBuf {
    std::env::var("UPLOAD_DIR")
        .unwrap_or_else(|_| "data/uploads".to_string())
        .into()
}

/// 按魔数判断图片类型，返回 `(扩展名, MIME)`；认不出来返回 `None`。
fn sniff_image(bytes: &[u8]) -> Option<(&'static str, &'static str)> {
    const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

    if bytes.starts_with(PNG) {
        return Some(("png", "image/png"));
    }
    // JPEG 以 FF D8 FF 开头
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some(("jpg", "image/jpeg"));
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some(("gif", "image/gif"));
    }
    // WebP 是 RIFF 容器：`RIFF????WEBP`
    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        return Some(("webp", "image/webp"));
    }
    None
}

/// 由扩展名反推 MIME，供提供文件时设置响应头。
fn mime_for_extension(extension: &str) -> Option<&'static str> {
    match extension {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

/// 生成一个不可预测的文件名。
fn random_file_name(extension: &str) -> String {
    let mut bytes = [0u8; 16];
    rand::fill(&mut bytes);
    format!("{}.{}", hex::encode(bytes), extension)
}

/// 把 `/uploads/<user>/<file>` 映回磁盘路径。不是本用户的路径一律拒绝。
fn disk_path_for(user_id: i64, url: &str) -> Option<std::path::PathBuf> {
    let rest = url.strip_prefix(URL_PREFIX)?;
    let (owner, file) = rest.split_once('/')?;

    // 只认自己的目录，且文件名必须只是一个文件名
    if owner != user_id.to_string() {
        return None;
    }
    if file.is_empty() || file.contains('/') || file.contains('\\') || file.contains("..") {
        return None;
    }

    Some(upload_root().join(owner).join(file))
}

/// 读出该用户当前的背景图 URL。
async fn current_background(pool: &sqlx::SqlitePool, user_id: i64) -> Option<String> {
    sqlx::query_scalar::<_, Option<String>>("SELECT bg_value FROM user_prefs WHERE user_id = ?1")
        .bind(user_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .flatten()
}

/// 写回背景图设置。`upload` 表示本地上传，`none` 表示清空。
async fn set_background(
    pool: &sqlx::SqlitePool,
    user_id: i64,
    kind: &str,
    value: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO user_prefs (user_id, bg_kind, bg_value, updated_at) \
         VALUES (?1, ?2, ?3, datetime('now')) \
         ON CONFLICT(user_id) DO UPDATE SET \
           bg_kind = excluded.bg_kind, \
           bg_value = excluded.bg_value, \
           updated_at = datetime('now')",
    )
    .bind(user_id)
    .bind(kind)
    .bind(value)
    .execute(pool)
    .await
    .map(|_| ())
}

/// 删除该用户上一张上传的图（只删本用户目录下的，且不删刚写的那张）。
async fn remove_previous_upload(user_id: i64, previous: Option<&str>, keep: Option<&str>) {
    let Some(previous) = previous else { return };
    if Some(previous) == keep {
        return;
    }
    if let Some(path) = disk_path_for(user_id, previous) {
        let _ = tokio::fs::remove_file(path).await;
    }
}

/// `POST /api/me/background`：接收一张背景图。
///
/// 故意做成普通 Axum 路由而不是 server function：multipart 用原生提取器更直接，
/// 而且普通 `<form enctype="multipart/form-data">` 提交即可使用，不依赖 JS。
///
/// 提取器顺序有讲究：`Multipart` 会消费请求体，必须排在最后。
pub async fn upload_background(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Response {
    let Some(user_id) = crate::auth::user_id_from_headers(&state.pool, &headers).await else {
        return Redirect::to("/me?e=auth").into_response();
    };

    let mut uploaded: Option<Vec<u8>> = None;
    loop {
        let Ok(Some(field)) = multipart.next_field().await else {
            break;
        };
        if field.name() != Some("file") {
            continue;
        }
        match field.bytes().await {
            Ok(data) if data.len() <= MAX_UPLOAD_BYTES => uploaded = Some(data.to_vec()),
            // 超限与读取出错都归到同一类：对用户来说就是「这张图太大了」
            _ => return Redirect::to("/me?e=size").into_response(),
        }
        break;
    }

    let Some(data) = uploaded else {
        return Redirect::to("/me?e=empty").into_response();
    };
    if data.is_empty() {
        return Redirect::to("/me?e=empty").into_response();
    }

    // 类型完全由魔数决定，客户端报的 content-type 不参与判断
    let Some((extension, _mime)) = sniff_image(&data) else {
        return Redirect::to("/me?e=type").into_response();
    };

    let directory = upload_root().join(user_id.to_string());
    if let Err(error) = tokio::fs::create_dir_all(&directory).await {
        tracing::error!("创建上传目录失败: {error}");
        return Redirect::to("/me?e=io").into_response();
    }

    let file_name = random_file_name(extension);
    if let Err(error) = tokio::fs::write(directory.join(&file_name), &data).await {
        tracing::error!("写入上传文件失败: {error}");
        return Redirect::to("/me?e=io").into_response();
    }

    let url = format!("{URL_PREFIX}{user_id}/{file_name}");
    let previous = current_background(&state.pool, user_id).await;

    if let Err(error) = set_background(&state.pool, user_id, "upload", Some(&url)).await {
        tracing::error!("保存背景图记录失败: {error}");
        // 记录没写成功就把刚落的文件删掉，别留下没人引用的孤儿文件
        let _ = tokio::fs::remove_file(directory.join(&file_name)).await;
        return Redirect::to("/me?e=io").into_response();
    }

    // 换图成功后再删旧图，顺序反了会在写库失败时把用户原来的图弄丢
    remove_previous_upload(user_id, previous.as_deref(), Some(&url)).await;

    // 303：让浏览器改用 GET 重新请求 /me，避免刷新时重复提交
    Redirect::to("/me?e=ok").into_response()
}

/// `POST /api/me/background/clear`：清空背景图。
pub async fn clear_background(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    let Some(user_id) = crate::auth::user_id_from_headers(&state.pool, &headers).await else {
        return Redirect::to("/me?e=auth").into_response();
    };

    let previous = current_background(&state.pool, user_id).await;
    if let Err(error) = set_background(&state.pool, user_id, "none", None).await {
        tracing::error!("清空背景图记录失败: {error}");
        return Redirect::to("/me?e=io").into_response();
    }

    remove_previous_upload(user_id, previous.as_deref(), None).await;
    Redirect::to("/me?e=cleared").into_response()
}

/// `GET /uploads/{*path}`：提供已上传的图片。
///
/// 路径走**严格白名单**：必须是 `<数字>/<文件名>.<位图扩展名>`，且文件名只含
/// 字母数字。任何不合形式的输入直接 404，从根上排除 `..` 与绝对路径之类的穿越。
pub async fn serve_upload(Path(path): Path<String>) -> Response {
    let Some((owner, file)) = path.split_once('/') else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if owner.is_empty() || !owner.chars().all(|c| c.is_ascii_digit()) {
        return StatusCode::NOT_FOUND.into_response();
    }

    let Some((stem, extension)) = file.rsplit_once('.') else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if stem.is_empty() || !stem.chars().all(|c| c.is_ascii_alphanumeric()) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let Some(mime) = mime_for_extension(&extension.to_ascii_lowercase()) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let Ok(bytes) = tokio::fs::read(upload_root().join(owner).join(file)).await else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let mut response = bytes.into_response();
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(mime));
    // 不让浏览器猜类型：上传内容一律按声明的图片类型解释
    response.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    // 文件名含随机数、内容不会变，可以长缓存
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=31536000, immutable"),
    );
    response
}
