//! 媒体工具条：图片上传 / 语音录音转写。
//!
//! 社区发布页与评论框共用。交互：
//! - 「图片」：选择图片 → POST /api/media → 正文追加 `![图片](/media/...)`；
//! - 「语音」：点击开始录音（MediaRecorder）→ 再点停止 → 音频上传 /api/media，
//!   并调用 /api/stt 转写 → 正文追加「识别文字 + 🎤 语音播放条（Markdown 链接）」，
//!   发送前可自由编辑。
//!
//! 仅在浏览器端（hydrate）有副作用；SSR 只渲染按钮骨架（与整站水合策略一致）。

use leptos::prelude::*;

/// 把一段 Markdown 追加到正文（自动补换行）。
fn append_line(text: &RwSignal<String>, line: &str) {
    let current = text.get_untracked();
    let mut next = current;
    if !next.is_empty() && !next.ends_with('\n') {
        next.push('\n');
    }
    next.push_str(line);
    next.push('\n');
    text.set(next);
}

#[component]
pub fn MediaTools(#[prop(into)] text: RwSignal<String>) -> impl IntoView {
    let file_input = NodeRef::<leptos::html::Input>::new();
    let busy = RwSignal::new(false);
    let recording = RwSignal::new(false);
    let message = RwSignal::new(String::new());

    let pick_image = move |_| {
        #[cfg(feature = "hydrate")]
        if let Some(input) = file_input.get() {
            let _ = input.click();
        }
    };

    let on_file_change = move |ev: leptos::ev::Event| {
        #[cfg(feature = "hydrate")]
        {
            use wasm_bindgen::JsCast;
            let Some(target) = ev.target() else { return };
            let Ok(input) = target.dyn_into::<web_sys::HtmlInputElement>() else {
                return;
            };
            let Some(files) = input.files() else { return };
            let Some(file) = files.get(0) else { return };
            input.set_value("");
            busy.set(true);
            message.set(String::new());
            leptos::task::spawn_local(async move {
                match imp::upload_file(&file, "/api/media").await {
                    Ok(url) => append_line(&text, &format!("![图片]({url})")),
                    Err(error) => message.set(error),
                }
                busy.set(false);
            });
        }
        #[cfg(not(feature = "hydrate"))]
        let _ = ev;
    };

    let toggle_record = move |_| {
        #[cfg(feature = "hydrate")]
        imp::toggle_recording(recording, busy, message, text);
    };

    view! {
        <div class="media-tools">
            <input
                node_ref=file_input
                type="file"
                accept="image/*"
                class="hidden-file"
                on:change=on_file_change
            />
            <button
                type="button"
                class="btn btn-small"
                on:click=pick_image
                disabled=move || busy.get()
            >
                {move || if busy.get() { "上传中…" } else { "图片" }}
            </button>
            <button
                type="button"
                class="btn btn-small"
                class:recording=move || recording.get()
                on:click=toggle_record
                disabled=move || busy.get()
            >
                {move || if recording.get() { "停止录音" } else { "语音" }}
            </button>
            {move || {
                recording
                    .get()
                    .then(|| view! { <span class="recording-dot" title="录音中"></span> })
            }}
            {move || {
                let text = message.get();
                (!text.is_empty()).then(|| view! { <span class="muted media-tools-msg">{text}</span> })
            }}
        </div>
    }
}

/// 浏览器端实现：录音状态、上传与转写。
#[cfg(feature = "hydrate")]
mod imp {
    use std::cell::RefCell;
    use std::rc::Rc;

    use leptos::prelude::*;
    use wasm_bindgen::prelude::*;

    use super::append_line;

    /// 录音中的全局状态（同一时刻只允许一路录音）。
    struct RecState {
        recorder: web_sys::MediaRecorder,
        stream: web_sys::MediaStream,
        chunks: Rc<RefCell<Vec<web_sys::Blob>>>,
        handlers: Vec<Closure<dyn FnMut(web_sys::Event)>>,
    }

    thread_local! {
        static REC: RefCell<Option<RecState>> = const { RefCell::new(None) };
    }

    fn window() -> Result<web_sys::Window, String> {
        web_sys::window().ok_or_else(|| "浏览器环境异常".to_string())
    }

    /// fetch POST（same-origin，带会话 Cookie），返回 JSON。
    async fn post_form(path: &str, form: &web_sys::FormData) -> Result<JsValue, String> {
        let init = web_sys::RequestInit::new();
        init.set_method("POST");
        init.set_body(form.as_ref());
        init.set_credentials(web_sys::RequestCredentials::SameOrigin);
        let request = web_sys::Request::new_with_str_and_init(path, &init)
            .map_err(|_| "构造请求失败".to_string())?;
        let promise = window()?
            .fetch_with_request(&request);
        let response = wasm_bindgen_futures::JsFuture::from(promise)
            .await
            .map_err(|_| "网络请求失败".to_string())?;
        let response: web_sys::Response = response
            .dyn_into()
            .map_err(|_| "响应类型异常".to_string())?;
        if !response.ok() {
            return Err(format!("请求失败（HTTP {}）", response.status()));
        }
        wasm_bindgen_futures::JsFuture::from(
            response.json().map_err(|_| "响应解析失败".to_string())?,
        )
        .await
        .map_err(|_| "响应解析失败".to_string())
    }

    fn json_string(value: &JsValue, key: &str) -> Option<String> {
        js_sys::Reflect::get(value, &JsValue::from_str(key))
            .ok()
            .and_then(|value| value.as_string())
    }

    /// 上传文件到媒体接口，返回 URL。
    pub async fn upload_file(file: &web_sys::File, path: &str) -> Result<String, String> {
        let form = web_sys::FormData::new().map_err(|_| "构造上传表单失败".to_string())?;
        form.append_with_blob_and_filename("file", file, &file.name())
            .map_err(|_| "附加文件失败".to_string())?;
        let json = post_form(path, &form).await?;
        json_string(&json, "url").ok_or_else(|| "上传响应缺少 url".to_string())
    }

    /// 上传录音 Blob 并转写；返回（音频 URL, 识别文字）。
    async fn upload_voice(blob: &web_sys::Blob) -> Result<(String, String), String> {
        let media_form =
            web_sys::FormData::new().map_err(|_| "构造上传表单失败".to_string())?;
        media_form
            .append_with_blob_and_filename("file", blob, "voice.webm")
            .map_err(|_| "附加录音失败".to_string())?;
        let media_json = post_form("/api/media", &media_form).await?;
        let url = json_string(&media_json, "url").ok_or_else(|| "上传响应缺少 url".to_string())?;

        let stt_form =
            web_sys::FormData::new().map_err(|_| "构造转写表单失败".to_string())?;
        stt_form
            .append_with_blob_and_filename("file", blob, "voice.webm")
            .map_err(|_| "附加录音失败".to_string())?;
        let text = match post_form("/api/stt", &stt_form).await {
            Ok(json) => json_string(&json, "text").unwrap_or_default(),
            Err(_) => String::new(), // 转写失败不影响语音消息本身
        };
        Ok((url, text))
    }

    /// 开始/停止录音。
    pub fn toggle_recording(
        recording: RwSignal<bool>,
        busy: RwSignal<bool>,
        message: RwSignal<String>,
        text: RwSignal<String>,
    ) {
        let active = REC.with(|state| state.borrow().is_some());
        if active {
            // 停止：触发 onstop（在 onstop 里做上传与转写）
            REC.with(|state| {
                if let Some(rec) = state.borrow().as_ref() {
                    let _ = rec.recorder.stop();
                }
            });
            return;
        }

        busy.set(true);
        message.set(String::new());
        leptos::task::spawn_local(async move {
            let start = async {
                let constraints = web_sys::MediaStreamConstraints::new();
                constraints.set_audio(&JsValue::TRUE);
                let media_devices = window()?.navigator().media_devices().map_err(|_| {
                    "当前浏览器不支持录音".to_string()
                })?;
                let stream_value = wasm_bindgen_futures::JsFuture::from(
                    media_devices
                        .get_user_media_with_constraints(&constraints)
                        .map_err(|_| "无法访问麦克风".to_string())?,
                )
                .await
                .map_err(|_| "无法访问麦克风（请允许权限）".to_string())?;
                let stream: web_sys::MediaStream = stream_value
                    .dyn_into()
                    .map_err(|_| "音频流类型异常".to_string())?;

                let recorder = web_sys::MediaRecorder::new_with_media_stream(&stream)
                    .map_err(|_| "初始化录音失败".to_string())?;
                let chunks: Rc<RefCell<Vec<web_sys::Blob>>> = Rc::new(RefCell::new(Vec::new()));

                // ondataavailable：收集音频块
                let data_chunks = chunks.clone();
                let on_data = Closure::<dyn FnMut(web_sys::Event)>::new(move |event: web_sys::Event| {
                    if let Ok(blob_event) = event.dyn_into::<web_sys::BlobEvent>() {
                        if let Some(data) = blob_event.data() {
                            data_chunks.borrow_mut().push(data);
                        }
                    }
                });
                recorder.set_ondataavailable(Some(on_data.as_ref().unchecked_ref()));

                // onstop：拼接、上传、转写、落正文（然后清状态）
                let stop_chunks = chunks.clone();
                let on_stop = Closure::<dyn FnMut(web_sys::Event)>::new(move |_event: web_sys::Event| {
                    recording.set(false);
                    let parts = stop_chunks.borrow().clone();
                    REC.with(|state| {
                        if let Some(rec) = state.borrow().as_ref() {
                            for track in rec.stream.get_tracks().iter() {
                                if let Ok(track) = track.dyn_into::<web_sys::MediaStreamTrack>() {
                                    track.stop();
                                }
                            }
                        }
                        *state.borrow_mut() = None;
                    });
                    if parts.is_empty() {
                        message.set("没有录到声音，请重试。".to_string());
                        return;
                    }
                    busy.set(true);
                    leptos::task::spawn_local(async move {
                        let array = js_sys::Array::new();
                        for part in &parts {
                            array.push(part.as_ref());
                        }
                        let bag = web_sys::BlobPropertyBag::new();
                        bag.set_type("audio/webm");
                        let blob = web_sys::Blob::new_with_blob_sequence_and_options(
                            array.as_ref(),
                            &bag,
                        )
                        .ok();
                        match blob {
                            Some(blob) => match upload_voice(&blob).await {
                                Ok((url, recognized)) => {
                                    if !recognized.trim().is_empty() {
                                        append_line(&text, recognized.trim());
                                    }
                                    append_line(&text, &format!("[🎤 语音]({url})"));
                                }
                                Err(error) => message.set(error),
                            },
                            None => message.set("拼接录音失败。".to_string()),
                        }
                        busy.set(false);
                    });
                });
                recorder.set_onstop(Some(on_stop.as_ref().unchecked_ref()));

                recorder.start().map_err(|_| "开始录音失败".to_string())?;

                // 只保留需要的 handler（其余留空对象即可，Rust 侧需要保活）
                REC.with(|state| {
                    *state.borrow_mut() = Some(RecState {
                        recorder,
                        stream,
                        chunks,
                        handlers: vec![on_data, on_stop],
                    });
                });
                Ok::<(), String>(())
            }
            .await;
            match start {
                Ok(()) => {
                    recording.set(true);
                    message.set("录音中… 点击「停止录音」结束".to_string());
                }
                Err(error) => message.set(error),
            }
            busy.set(false);
        });
    }
}
