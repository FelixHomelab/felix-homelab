//! 富媒体存储：图片 / 文件 / 语音 / 视频。
//!
//! 存储策略（实测参数见 `site/TODO.md` 的「存储压缩策略」）：
//! - 内容寻址（sha256），天然去重；
//! - **上传一律原样存储**（保证 Range/秒开），压缩由后台按“年龄”执行：
//!   >7 天 `zstd-3`、>1 月 `zstd-7`、>3 月 `zstd-19`；
//! - 高访问量（近 7 天访问过 / 累计访问 ≥30 次）跳过压缩与升级，保访问体验；
//! - 压缩收益 <5%（≥95% 原大小）则放弃压缩；已压缩格式不参与；
//! - 配额按**原始大小**计（`original_size`），实际占用记 `stored_size`。
//!
//! 路由：
//! - `POST /api/media`（multipart `file`，可选 `kind`）→ `{id,url,...}`
//! - `GET /media/{id}`（未压缩支持 Range；压缩的按需解压后返回）

#![cfg(feature = "ssr")]

use std::path::PathBuf;

use axum::body::Body;
use axum::extract::{Multipart, Path, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;

use crate::state::AppState;

/// 存储根目录（默认 /app/data/media）。
fn media_dir() -> PathBuf {
    std::env::var("MEDIA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/app/data/media"))
}

/// 单文件上限（按类型，字节）。
fn max_size(kind: &str) -> u64 {
    match kind {
        "image" => 25 << 20,
        "audio" => 100 << 20,
        "video" => 1024 << 20,
        _ => 200 << 20,
    }
}

fn detect_kind(mime: &str) -> &'static str {
    if mime.starts_with("image/") {
        "image"
    } else if mime.starts_with("audio/") {
        "audio"
    } else if mime.starts_with("video/") {
        "video"
    } else {
        "file"
    }
}

/// 跳过压缩的类型：
/// - 图片/视频/音频：要么已压缩（增益 <5%），要么需要 Range 拖动播放（WAV）；
/// - 压缩包/PDF：混合内容，实测收益低。
fn skip_compress(mime: &str) -> bool {
    mime.starts_with("image/")
        || mime.starts_with("video/")
        || mime.starts_with("audio/")
        || matches!(
            mime,
            "application/zip"
                | "application/gzip"
                | "application/x-7z-compressed"
                | "application/x-rar-compressed"
                | "application/pdf"
        )
}

/// 压缩后 / 原大小 ≥ 此比例则不采用压缩（节省解码开销与空间意义不大）。
const COMPRESS_KEEP_RATIO: f64 = 0.95;

/// 供保管流程删除 blob 用。
#[cfg(feature = "ssr")]
pub fn blob_path_for(sha: &str, compression: &str) -> PathBuf {
    blob_path(sha, compression)
}

fn blob_path(sha: &str, compression: &str) -> PathBuf {
    let shard = &sha[..2.min(sha.len())];
    let name = if compression == "none" {
        sha.to_string()
    } else {
        format!("{sha}.{compression}")
    };
    media_dir().join(shard).join(name)
}

#[derive(Serialize)]
struct UploadResult {
    id: i64,
    url: String,
    kind: String,
    mime: String,
    original_size: i64,
    stored_size: i64,
    compression: String,
}

/// `POST /api/media`：登录用户上传一个文件。
pub async fn upload(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Response {
    let Some(user_id) = crate::auth::user_id_from_headers(&state.pool, &headers).await else {
        return (StatusCode::UNAUTHORIZED, "请先登录。").into_response();
    };

    let mut explicit_kind: Option<String> = None;
    let mut file_name = String::from("");
    let mut mime = String::from("application/octet-stream");
    let mut bytes: Option<Vec<u8>> = None;

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or_default().to_string();
        match name.as_str() {
            "kind" => explicit_kind = field.text().await.ok(),
            "file" => {
                if let Some(file) = field.file_name() {
                    file_name = file.to_string();
                }
                if let Some(content_type) = field.content_type() {
                    mime = content_type.to_string();
                }
                match field.bytes().await {
                    Ok(data) => bytes = Some(data.to_vec()),
                    Err(e) => {
                        tracing::error!("读取上传内容失败: {e}");
                        return (StatusCode::BAD_REQUEST, "读取上传内容失败。").into_response();
                    }
                }
            }
            _ => {}
        }
    }

    let Some(bytes) = bytes else {
        return (StatusCode::BAD_REQUEST, "缺少文件字段。").into_response();
    };

    let kind = explicit_kind
        .filter(|k| matches!(k.as_str(), "image" | "audio" | "video" | "file"))
        .unwrap_or_else(|| detect_kind(&mime).to_string());

    if bytes.is_empty() {
        return (StatusCode::BAD_REQUEST, "空文件。").into_response();
    }
    if bytes.len() as u64 > max_size(&kind) {
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            format!("文件超出上限（{}MB）。", max_size(&kind) >> 20),
        )
            .into_response();
    }

    let original_size = bytes.len() as i64;

    // 容量池校验：已开通（总额 > 0）才限制；过渡期（未开通）不拦
    let total = crate::storage::user_capacity_total(&state.pool, user_id).await;
    if total > 0 {
        let used = crate::storage::user_capacity_used(&state.pool, user_id).await;
        if used + original_size > total {
            return (
                StatusCode::PAYLOAD_TOO_LARGE,
                "容量池余额不足，请先购买容量或在「我的订阅」查看用量。",
            )
                .into_response();
        }
    }

    let sha = hex::encode(Sha256::digest(&bytes));
    let plain_path = blob_path(&sha, "none");

    // 原样存储（内容寻址；已存在则直接复用）。压缩由后台按年龄执行。
    let bytes_c = bytes.clone();
    let path_c = plain_path.clone();
    let result = tokio::task::spawn_blocking(move || -> std::io::Result<(String, i64)> {
        let dir = path_c.parent().unwrap_or(&path_c).to_path_buf();
        std::fs::create_dir_all(&dir)?;
        if !path_c.exists() {
            std::fs::write(&path_c, &bytes_c)?;
        }
        Ok(("none".to_string(), bytes_c.len() as i64))
    })
    .await;

    let (compression, stored_size) = match result {
        Ok(Ok(value)) => value,
        Ok(Err(e)) => {
            tracing::error!("媒体写盘失败: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "存储失败，稍后再试。").into_response();
        }
        Err(e) => {
            tracing::error!("媒体写盘任务失败: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "存储失败，稍后再试。").into_response();
        }
    };


    let insert = sqlx::query(
        "INSERT INTO media (owner_id, kind, mime, original_name, original_size, stored_size, compression, sha256) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )
    .bind(user_id)
    .bind(&kind)
    .bind(&mime)
    .bind(&file_name)
    .bind(original_size)
    .bind(stored_size)
    .bind(&compression)
    .bind(&sha)
    .execute(&state.pool)
    .await;

    let id = match insert {
        Ok(done) => done.last_insert_rowid(),
        Err(e) => {
            tracing::error!("媒体入库失败: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "存储失败，稍后再试。").into_response();
        }
    };

    // 路径带上安全的文件名：扩展名让 Cloudflare 默认缓存规则生效（静态扩展名才缓存）
    let safe_name = sanitize_name(&file_name);
    axum::Json(UploadResult {
        id,
        url: format!("/media/{id}/{safe_name}"),
        kind,
        mime,
        original_size,
        stored_size,
        compression,
    })
    .into_response()
}

/// `GET /media/{id}`：按需解压或按 Range 返回。
pub async fn serve(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
) -> Response {
    serve_media(state, id, headers).await
}

/// `GET /media/{id}/{name}`：带文件名（含扩展名）的入口，便于 CDN 按扩展名缓存。
pub async fn serve_named(
    State(state): State<AppState>,
    Path((id, _name)): Path<(i64, String)>,
    headers: HeaderMap,
) -> Response {
    serve_media(state, id, headers).await
}

async fn serve_media(state: AppState, id: i64, headers: HeaderMap) -> Response {
    use sqlx::Row;
    let row = sqlx::query(
        "SELECT mime, original_size, compression, sha256, original_name FROM media WHERE id = ?1",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await
    .ok()
    .flatten();

    let Some(row) = row else {
        return (StatusCode::NOT_FOUND, "文件不存在。").into_response();
    };

    let mime: String = row.get("mime");
    let original_size: i64 = row.get("original_size");
    let compression: String = row.get("compression");
    let sha: String = row.get("sha256");
    let original_name: String = row.get("original_name");
    let path = blob_path(&sha, &compression);

    let _ = sqlx::query(
        "UPDATE media SET last_accessed_at = datetime('now'), access_count = access_count + 1 \
         WHERE id = ?1",
    )
    .bind(id)
    .execute(&state.pool)
    .await;

    let disposition = if original_name.is_empty() {
        "inline".to_string()
    } else {
        format!(
            "inline; filename*=UTF-8''{}",
            utf8_percent_encode(&original_name, NON_ALPHANUMERIC)
        )
    };

    // 压缩内容：解压后整体返回（压缩仅用于文本类小文件；>32MB 不压缩）
    if compression != "none" {
        let data = tokio::task::spawn_blocking(move || -> std::io::Result<Vec<u8>> {
            let file = std::fs::File::open(&path)?;
            let mut decoder = zstd::stream::read::Decoder::new(file)?;
            let mut buf = Vec::new();
            std::io::Read::read_to_end(&mut decoder, &mut buf)?;
            Ok(buf)
        })
        .await;
        let data = match data {
            Ok(Ok(data)) => data,
            _ => return (StatusCode::INTERNAL_SERVER_ERROR, "读取失败。").into_response(),
        };
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, mime)
            .header(header::CONTENT_DISPOSITION, disposition)
            .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
            .body(Body::from(data))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
    }

    // 未压缩：支持单区间 Range（音视频播放必需）
    let total = original_size as u64;
    let range = parse_range(&headers, total);
    match range {
        Some((start, end)) => {
            let len = end - start + 1;
            let file = match tokio::fs::File::open(&path).await {
                Ok(file) => file,
                Err(e) => {
                    tracing::error!("打开媒体文件失败: {e}");
                    return (StatusCode::NOT_FOUND, "文件不存在。").into_response();
                }
            };
            let mut file = file;
            if file.seek(std::io::SeekFrom::Start(start)).await.is_err() {
                return (StatusCode::INTERNAL_SERVER_ERROR, "读取失败。").into_response();
            }
            let stream = ReaderStream::new(file.take(len));
            Response::builder()
                .status(StatusCode::PARTIAL_CONTENT)
                .header(header::CONTENT_TYPE, mime)
                .header(header::CONTENT_DISPOSITION, disposition)
                .header(header::ACCEPT_RANGES, "bytes")
                .header(
                    header::CONTENT_RANGE,
                    format!("bytes {start}-{end}/{total}"),
                )
                .header(header::CONTENT_LENGTH, len)
                .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
                .body(Body::from_stream(stream))
                .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
        }
        None => {
            let file = match tokio::fs::File::open(&path).await {
                Ok(file) => file,
                Err(e) => {
                    tracing::error!("打开媒体文件失败: {e}");
                    return (StatusCode::NOT_FOUND, "文件不存在。").into_response();
                }
            };
            let stream = ReaderStream::new(file);
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime)
                .header(header::CONTENT_DISPOSITION, disposition)
                .header(header::ACCEPT_RANGES, "bytes")
                .header(header::CONTENT_LENGTH, total)
                .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
                .body(Body::from_stream(stream))
                .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
        }
    }
}

/// 解析单区间 `Range: bytes=a-b`；不合法或未提供返回 None（整体返回）。
fn parse_range(headers: &HeaderMap, total: u64) -> Option<(u64, u64)> {
    let raw = headers
        .get(header::RANGE)?
        .to_str()
        .ok()?
        .strip_prefix("bytes=")?;
    if raw.contains(',') {
        return None; // 多区间不处理，退回整体
    }
    let (start, end) = raw.split_once('-')?;
    let start: u64 = start.trim().parse().ok()?;
    if start >= total {
        return None;
    }
    let end: u64 = if end.trim().is_empty() {
        total - 1
    } else {
        end.trim().parse::<u64>().ok()?.min(total - 1)
    };
    (start <= end).then_some((start, end))
}

/// 文件名清洗：只保留 ASCII 字母数字与 `. - _`，其余替换为 `_`；空名用 `file`。
fn sanitize_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "file".to_string()
    } else {
        cleaned
    }
}

/// 热度阈值：累计访问次数达到该值视为“热”，不再压缩/升级。
const HOT_ACCESS_COUNT: i64 = 30;

/// 启动媒体重压缩后台任务（站点进程内单实例；每 24h 一轮，启动后 5 分钟先跑一轮）。
#[cfg(feature = "ssr")]
pub fn spawn_recompression_task(pool: sqlx::SqlitePool) {
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(300)).await;
        loop {
            match run_recompression_once(&pool).await {
                Ok(0) => {}
                Ok(count) => tracing::info!("媒体重压缩完成：{count} 个文件"),
                Err(e) => tracing::warn!("媒体重压缩任务失败: {e}"),
            }
            tokio::time::sleep(std::time::Duration::from_secs(24 * 3600)).await;
        }
    });
}

/// 一轮重压缩：按“年龄”升档（none→-3→-7→-19），热文件跳过。
#[cfg(feature = "ssr")]
async fn run_recompression_once(pool: &sqlx::SqlitePool) -> anyhow::Result<usize> {
    use sqlx::Row;

    // 阶段：源级别 → 目标级别 → 文件年龄门槛（天）
    let stages = [
        ("none", "zstd-3", 7_i64),
        ("zstd-3", "zstd-7", 30_i64),
        ("zstd-7", "zstd-19", 90_i64),
    ];
    let mut done = 0usize;

    for (from, to, age_days) in stages {
        let rows = sqlx::query(
            "SELECT id, sha256, mime FROM media \
             WHERE compression = ?1 \
               AND created_at <= datetime('now', ?2) \
               AND last_accessed_at <= datetime('now','-7 days') \
               AND access_count < ?3 \
             ORDER BY created_at ASC LIMIT 50",
        )
        .bind(from)
        .bind(format!("-{age_days} days"))
        .bind(HOT_ACCESS_COUNT)
        .fetch_all(pool)
        .await?;

        for row in rows {
            let id: i64 = row.get("id");
            let sha: String = row.get("sha256");
            let mime: String = row.get("mime");
            if skip_compress(&mime) {
                continue;
            }
            let level = match to {
                "zstd-3" => 3,
                "zstd-7" => 7,
                _ => 19,
            };
            match recompress_blob(&sha, from, to, level).await {
                Ok(Some(stored)) => {
                    sqlx::query(
                        "UPDATE media SET compression = ?1, stored_size = ?2 WHERE id = ?3",
                    )
                    .bind(to)
                    .bind(stored)
                    .bind(id)
                    .execute(pool)
                    .await?;
                    // 旧 blob 无其它行引用则删除（内容寻址可能被多行共享）
                    let refs: i64 = sqlx::query_scalar(
                        "SELECT COUNT(*) FROM media WHERE sha256 = ?1 AND compression = ?2",
                    )
                    .bind(&sha)
                    .bind(from)
                    .fetch_one(pool)
                    .await
                    .unwrap_or(0);
                    if refs == 0 {
                        let old = blob_path(&sha, from);
                        let _ = std::fs::remove_file(old);
                    }
                    done += 1;
                }
                Ok(None) => {
                    // 压缩收益不足：保持原级别（下次仍会尝试；如需避免可调阈值）
                }
                Err(e) => tracing::warn!("重压缩 media#{id} 失败: {e}"),
            }
        }
    }
    Ok(done)
}

/// 把 `sha` 的 blob 从 `from` 级别重压到 `to` 级别；收益不足返回 None。
#[cfg(feature = "ssr")]
async fn recompress_blob(
    sha: &str,
    from: &str,
    to: &str,
    level: i32,
) -> anyhow::Result<Option<i64>> {
    let sha = sha.to_string();
    let from = from.to_string();
    let to = to.to_string();
    let joined = tokio::task::spawn_blocking(move || -> anyhow::Result<Option<i64>> {
        let src = blob_path(&sha, &from);
        let dst = blob_path(&sha, &to);
        if !src.exists() {
            anyhow::bail!("源 blob 不存在: {}", src.display());
        }
        if dst.exists() {
            return Ok(Some(std::fs::metadata(&dst)?.len() as i64));
        }
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let data: Vec<u8> = if from == "none" {
            std::fs::read(&src)?
        } else {
            let file = std::fs::File::open(&src)?;
            let mut decoder = zstd::stream::read::Decoder::new(file)?;
            let mut buf = Vec::new();
            std::io::Read::read_to_end(&mut decoder, &mut buf)?;
            buf
        };
        let original = data.len() as i64;
        let mut encoder =
            zstd::stream::write::Encoder::new(std::fs::File::create(&dst)?, level)?;
        std::io::copy(&mut data.as_slice(), &mut encoder)?;
        let mut out = encoder.finish()?;
        use std::io::Write;
        out.flush()?;
        let size = std::fs::metadata(&dst)?.len() as i64;
        if size as f64 / (original as f64) >= COMPRESS_KEEP_RATIO {
            let _ = std::fs::remove_file(&dst);
            return Ok(None);
        }
        Ok(Some(size))
    })
    .await??;
    Ok(joined)
}
