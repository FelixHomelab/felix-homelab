//! 语音转文字代理：/api/stt。
//!
//! 浏览器录音（webm/opus 等）→ 本站鉴权 → 转发给站内 STT 服务
//! （faster-whisper，pod 内 `http://localhost:8080/v1/audio/transcriptions`）。
//! 这样 STT 服务不直接对外，且转写接口统一走登录校验与体积上限。

#![cfg(feature = "ssr")]

use axum::extract::{Multipart, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};

use crate::state::AppState;

/// 单次录音上限（25MB，约 10 分钟 opus）。
const STT_BODY_LIMIT: usize = 25 << 20;

pub const STT_ROUTE_LIMIT: usize = 25 << 20;

fn stt_endpoint() -> String {
    std::env::var("STT_URL")
        .unwrap_or_else(|_| "http://localhost:8095/v1/audio/transcriptions".to_string())
}

/// `POST /api/stt`（multipart `file`，可选 `language`）→ `{"text": "..."}`。
pub async fn transcribe(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Response {
    let Some(_user_id) = crate::auth::user_id_from_headers(&state.pool, &headers).await else {
        return (StatusCode::UNAUTHORIZED, "请先登录。").into_response();
    };

    let mut audio: Option<(Vec<u8>, String, String)> = None; // (bytes, filename, mime)
    let mut language: Option<String> = None;

    while let Ok(Some(field)) = multipart.next_field().await {
        match field.name().unwrap_or_default() {
            "language" => language = field.text().await.ok(),
            "file" => {
                let name = field.file_name().unwrap_or("voice.webm").to_string();
                let mime = field
                    .content_type()
                    .unwrap_or("audio/webm")
                    .to_string();
                match field.bytes().await {
                    Ok(data) => {
                        if data.len() > STT_BODY_LIMIT {
                            return (
                                StatusCode::PAYLOAD_TOO_LARGE,
                                "录音太长（上限 25MB）。",
                            )
                                .into_response();
                        }
                        audio = Some((data.to_vec(), name, mime));
                    }
                    Err(e) => {
                        tracing::error!("读取录音失败: {e}");
                        return (StatusCode::BAD_REQUEST, "读取录音失败。").into_response();
                    }
                }
            }
            _ => {}
        }
    }

    let Some((bytes, filename, mime)) = audio else {
        return (StatusCode::BAD_REQUEST, "缺少录音文件。").into_response();
    };

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
    {
        Ok(client) => client,
        Err(e) => {
            tracing::error!("构造 STT 客户端失败: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "服务器出了点问题。").into_response();
        }
    };

    let part = reqwest::multipart::Part::bytes(bytes)
        .file_name(filename)
        .mime_str(&mime)
        .unwrap_or_else(|_| reqwest::multipart::Part::bytes(Vec::new()));
    let mut form = reqwest::multipart::Form::new().part("file", part);
    if let Some(language) = language.filter(|value| !value.trim().is_empty()) {
        form = form.text("language", language);
    }

    let response = client.post(stt_endpoint()).multipart(form).send().await;
    match response {
        Ok(response) if response.status().is_success() => match response.json::<serde_json::Value>().await {
            Ok(json) => axum::Json(json).into_response(),
            Err(e) => {
                tracing::error!("解析 STT 响应失败: {e}");
                (StatusCode::BAD_GATEWAY, "转写服务返回异常。").into_response()
            }
        },
        Ok(response) => {
            let status = response.status();
            tracing::error!("STT 服务返回异常: {status}");
            (StatusCode::BAD_GATEWAY, "转写服务暂时不可用。").into_response()
        }
        Err(e) => {
            tracing::error!("请求 STT 服务失败: {e}");
            (StatusCode::BAD_GATEWAY, "转写服务暂时不可用。").into_response()
        }
    }
}
