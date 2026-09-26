//! 外观偏好：主题模式、主色、背景图。
//!
//! 目前存在三只 cookie（`gf_theme` / `gf_accent` / `gf_bg`）里；阶段 4 起，已登录
//! 用户的偏好改存账号，cookie 退化为未登录访客的载体。
//!
//! **消除首屏闪烁**的办法：服务端渲染时就从请求头读到 cookie，把结果直接写进
//! `<html>` 的 class 与 style，因此第一帧就是最终外观。浏览器端水合时读的是同一个
//! cookie，两端取值一致，不会产生水合不匹配。
//!
//! **这三个值全部来自客户端，并且会被拼进 HTML 属性**，因此一律当作不可信输入：
//! 主色限死六位十六进制；背景图限死 `http(s)://` 或站内相对路径，且字符集走白名单。
//! 放行任意字符串等于把 XSS 送到每个访客面前。

use leptos::prelude::*;
use serde::{Deserialize, Serialize};

/// 存主题模式的 cookie 名。
pub const COOKIE_MODE: &str = "gf_theme";
/// 存主色的 cookie 名。
pub const COOKIE_ACCENT: &str = "gf_accent";
/// 存背景图 URL 的 cookie 名（percent 编码后）。
pub const COOKIE_BG: &str = "gf_bg";

/// cookie 有效期：一年。
const COOKIE_MAX_AGE: i64 = 60 * 60 * 24 * 365;

/// 背景图 URL 允许出现的字符。
///
/// 刻意不含 `"` `'` `(` `)` `<` `>` `` ` `` 与空白：这些字符能闭合 `url("...")`
/// 或直接跳出属性，是注入的常见入口。
const SAFE_URL_CHARS: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~:/?#[]@!$&*+,;=%";

/// 主题三档。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum ThemeMode {
    /// 跟随系统（默认）。
    #[default]
    Auto,
    Light,
    Dark,
}

impl ThemeMode {
    /// 存进 cookie 的取值。
    pub const fn as_str(self) -> &'static str {
        match self {
            ThemeMode::Auto => "auto",
            ThemeMode::Light => "light",
            ThemeMode::Dark => "dark",
        }
    }

    /// 从 cookie 取值解析；无法识别时回落到 `Auto`，不报错。
    pub fn parse(raw: &str) -> Self {
        match raw {
            "light" => ThemeMode::Light,
            "dark" => ThemeMode::Dark,
            _ => ThemeMode::Auto,
        }
    }

    /// 写到 `<html class="...">` 上的类名。
    ///
    /// `auto` 也给出一个类名 `auto`：Leptos 的 `class` 属性无论空否都会输出，
    /// 与其留下难看的 `class=""`，不如给个自解释的值。CSS 里 `html.auto` 不匹配
    /// `html.light` / `html.dark`，因此系统偏好照常生效。
    pub const fn html_class(self) -> &'static str {
        match self {
            ThemeMode::Auto => "auto",
            ThemeMode::Light => "light",
            ThemeMode::Dark => "dark",
        }
    }

    /// 给界面显示的中文名。
    pub const fn label(self) -> &'static str {
        match self {
            ThemeMode::Auto => "跟随系统",
            ThemeMode::Light => "亮色",
            ThemeMode::Dark => "暗色",
        }
    }
}

/// 一份外观偏好。
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct ThemePrefs {
    pub mode: ThemeMode,
    /// 六位小写十六进制，不含 `#`。
    pub accent: Option<String>,
    /// 背景图 URL，已通过白名单校验。
    pub background: Option<String>,
}

impl ThemePrefs {
    /// 取当前生效的偏好。
    ///
    /// 两端读的是同一只 cookie，所以服务端渲染与水合的结果一致。
    pub fn current() -> Self {
        let raw = read_cookie_string();
        Self::from_cookie_str(raw.as_deref())
    }

    /// 从 `name=value; name2=value2` 形式的串解析。所有取值都经过校验。
    pub fn from_cookie_str(raw: Option<&str>) -> Self {
        let Some(raw) = raw else {
            return Self::default();
        };

        let mut prefs = Self::default();
        for pair in raw.split(';') {
            let pair = pair.trim();
            let Some((name, value)) = pair.split_once('=') else {
                continue;
            };
            match name.trim() {
                COOKIE_MODE => prefs.mode = ThemeMode::parse(value.trim()),
                COOKIE_ACCENT => prefs.accent = validate_accent(value.trim()),
                COOKIE_BG => prefs.background = decode_background(value.trim()),
                _ => {}
            }
        }
        prefs
    }

    /// 写到 `<html class="...">` 上的类名。
    pub fn html_class(&self) -> &'static str {
        self.mode.html_class()
    }

    /// 写到 `<html style="...">` 上的 CSS 变量声明，`None` 表示不写这条属性。
    pub fn style_attr(&self) -> Option<String> {
        let mut declarations: Vec<String> = Vec::new();

        if let Some(accent) = &self.accent {
            declarations.push(format!("--accent:#{accent}"));
            // 主色由用户自选，深浅都可能；这里按亮度选前景色，
            // 否则选了浅色主色时按钮上的白字会看不清。
            declarations.push(format!("--accent-contrast:{}", contrast_for(accent)));
        }

        if let Some(url) = &self.background {
            declarations.push(format!("--bg-image:url(\"{url}\")"));
        }

        if declarations.is_empty() {
            None
        } else {
            Some(declarations.join(";"))
        }
    }

    /// 是否与另一份偏好完全相同——用于判断某个模式按钮是否处于选中态。
    pub fn same_mode(&self, mode: ThemeMode) -> bool {
        self.mode == mode
    }
}

/// 校验主色：只接受六位十六进制。
///
/// 这个值会被拼进 `style` 属性，因此不能放过任何非十六进制字符。
/// 表单输入与 cookie 取值共用这一处。
pub fn validate_accent(raw: &str) -> Option<String> {
    let hex = raw.strip_prefix('#').unwrap_or(raw);
    if hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(hex.to_ascii_lowercase())
    } else {
        None
    }
}

/// 解码并校验背景图 URL。
///
/// 写入 cookie 时做了 percent 编码（避免 `;` `,` `=` 破坏 cookie 语法），这里还原，
/// 再走 [`validate_background`]。任何一步不通过就当作没设。
fn decode_background(raw: &str) -> Option<String> {
    let decoded = percent_encoding::percent_decode_str(raw)
        .decode_utf8()
        .ok()?;
    validate_background(&decoded)
}

/// 校验一个**未编码**的背景图 URL。
///
/// 表单里用户直接输入的地址走这里；从 cookie 读到的先 percent 解码再走这里。
pub fn validate_background(url: &str) -> Option<String> {
    let url = url.trim();

    if url.is_empty() || url.len() > 500 {
        return None;
    }

    // 只放行外链 http(s) 与站内相对路径；javascript: / data: 等一律拒绝。
    // 另外必须排掉 `//host/x.jpg`：它以 `/` 开头，看着像站内路径，
    // 实际是协议相对地址，会去加载第三方资源。
    let is_relative_path = url.starts_with('/') && !url.starts_with("//");
    let allowed_scheme = url.starts_with("https://") || url.starts_with("http://") || is_relative_path;
    if !allowed_scheme {
        return None;
    }

    if !url.chars().all(|c| SAFE_URL_CHARS.contains(c)) {
        return None;
    }

    Some(url.to_string())
}

/// 依主色亮度选前景色。
///
/// 做法是按 WCAG 的相对亮度分别算出「配白字」与「配黑字」的对比度，取更高的那个——
/// 比拍一个亮度阈值靠谱：像纯绿 `#00ff00` 这种通道差异极大的颜色，粗略加权公式
/// 会给出中等亮度而选错前景色。
fn contrast_for(hex: &str) -> &'static str {
    /// sRGB 单通道线性化，WCAG 相对亮度的第一步。
    fn linearize(value: u8) -> f32 {
        let channel = value as f32 / 255.0;
        if channel <= 0.04045 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    }

    let channel = |range: std::ops::Range<usize>| {
        u8::from_str_radix(&hex[range], 16).unwrap_or(0)
    };
    let luminance = 0.2126 * linearize(channel(0..2))
        + 0.7152 * linearize(channel(2..4))
        + 0.0722 * linearize(channel(4..6));

    // 对比度公式：(L1 + 0.05) / (L2 + 0.05)，白色 L = 1，黑色 L = 0
    let against_white = 1.05 / (luminance + 0.05);
    let against_black = (luminance + 0.05) / 0.05;

    if against_black >= against_white {
        "#101014"
    } else {
        "#ffffff"
    }
}

// ---------------------------------------------------------------------------
// 读 cookie：两端各有一套实现，但返回同一个值
// ---------------------------------------------------------------------------

/// 服务端：从本次请求的 `Cookie` 头读。`Parts` 由 leptos_axum 提供为 context。
#[cfg(feature = "ssr")]
fn read_cookie_string() -> Option<String> {
    use_context::<axum::http::request::Parts>()
        .and_then(|parts| parts.headers.get(axum::http::header::COOKIE).cloned())
        .and_then(|value| value.to_str().ok().map(str::to_owned))
}

/// 浏览器端：从 `document.cookie` 读。
#[cfg(not(feature = "ssr"))]
fn read_cookie_string() -> Option<String> {
    let document = web_sys::window()?.document()?;
    read_cookie_property(&document)
}

/// 读取 `document.cookie`。
///
/// web-sys 0.3 把 `cookie` 只挂在已废弃的 `HtmlDocument` 接口上，而现代浏览器里
/// `document instanceof HTMLDocument` 并不可靠，所以直接按属性名读写，绕开接口转换。
#[cfg(not(feature = "ssr"))]
fn read_cookie_property(document: &web_sys::Document) -> Option<String> {
    js_sys::Reflect::get(document, &wasm_bindgen::JsValue::from_str("cookie"))
        .ok()?
        .as_string()
}

// ---------------------------------------------------------------------------
// 写 cookie：服务端走 Set-Cookie，浏览器端走 document.cookie
// ---------------------------------------------------------------------------

/// 把一条 cookie 拼成 `name=value; Path=/; Max-Age=…; SameSite=Lax`。
///
/// 主题 cookie **刻意不加 `HttpOnly`**：浏览器端水合时要读它，否则服务端与客户端
/// 可能读到不同的偏好。只有会话 cookie 需要 `HttpOnly`。
fn cookie_string(name: &str, value: Option<&str>) -> String {
    match value {
        Some(value) => format!("{name}={value}; Path=/; Max-Age={COOKIE_MAX_AGE}; SameSite=Lax"),
        // Max-Age=0 是删除 cookie 的标准做法
        None => format!("{name}=; Path=/; Max-Age=0; SameSite=Lax"),
    }
}

/// 服务端：把 cookie 追加到本次响应。
#[cfg(feature = "ssr")]
fn set_cookie(name: &str, value: Option<&str>) {
    use axum::http::header::{HeaderValue, SET_COOKIE};

    let Some(options) = use_context::<leptos_axum::ResponseOptions>() else {
        return;
    };
    match HeaderValue::from_str(&cookie_string(name, value)) {
        Ok(header) => options.append_header(SET_COOKIE, header),
        Err(_) => leptos::logging::warn!("构造 Set-Cookie 失败：{name}"),
    }
}

/// 浏览器端：写 `document.cookie`。
#[cfg(not(feature = "ssr"))]
fn set_cookie(name: &str, value: Option<&str>) {
    let Some(document) = web_sys::window().and_then(|w| w.document()) else {
        return;
    };

    // 给 document.cookie 赋值即可写入。web-sys 0.3 把 cookie 只挂在已废弃的
    // HtmlDocument 上，这里按属性名写，绕开 `instanceof HTMLDocument` 这个在
    // 现代浏览器并不可靠的判断。
    let result = js_sys::Reflect::set(
        &document,
        &wasm_bindgen::JsValue::from_str("cookie"),
        &wasm_bindgen::JsValue::from_str(&cookie_string(name, value)),
    );

    if result.is_err() {
        // 写失败只影响「下次刷新是否记得」，不值得打断用户操作
        leptos::logging::warn!("写入 cookie {name} 失败");
    }
}

/// 把一份偏好完整写进三只 cookie。
pub fn persist_to_cookies(prefs: &ThemePrefs) {
    set_cookie(COOKIE_MODE, Some(prefs.mode.as_str()));
    set_cookie(COOKIE_ACCENT, prefs.accent.as_deref());
    set_cookie(
        COOKIE_BG,
        prefs.background.as_deref().map(encode_background).as_deref(),
    );
}

/// 把背景图 URL 编码成可安全放进 cookie 的形式。
pub fn encode_background(url: &str) -> String {
    use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
    utf8_percent_encode(url, NON_ALPHANUMERIC).to_string()
}

// ---------------------------------------------------------------------------
// 跨组件共享的状态
// ---------------------------------------------------------------------------

/// 外观状态。放进 context 供顶栏按钮与设置页共用。
#[derive(Clone, Copy, Debug)]
pub struct ThemeState {
    prefs: leptos::prelude::RwSignal<ThemePrefs>,
}

impl ThemeState {
    /// 用初始偏好建立状态。初始值由 [`ThemePrefs::current`] 给出，两端一致。
    pub fn new(initial: ThemePrefs) -> Self {
        Self {
            prefs: leptos::prelude::RwSignal::new(initial),
        }
    }

    /// 当前偏好（响应式读取）。
    pub fn get(&self) -> ThemePrefs {
        self.prefs.get()
    }

    /// 改偏好：先写 cookie，再（若已登录）同步进账号。
    ///
    /// cookie 必须写：服务端下次渲染时只能看到 cookie，不写刷新即丢。
    /// 账号是真相来源，cookie 是它的缓存——登录时会从账号镜像一次。
    pub fn update(&self, change: impl FnOnce(&mut ThemePrefs)) {
        self.prefs.update(change);
        let prefs = self.prefs.get_untracked();
        persist_to_cookies(&prefs);

        // 同步进账号。未登录时服务端什么也不做，所以不必先判断登录状态。
        #[cfg(not(feature = "ssr"))]
        leptos::task::spawn_local(async move {
            let result = save_theme_prefs(
                prefs.mode.as_str().to_string(),
                prefs.accent.clone().unwrap_or_default(),
                prefs.background.clone().unwrap_or_default(),
            )
            .await;
            if let Err(error) = result {
                leptos::logging::warn!("同步外观偏好到账号失败: {error}");
            }
        });

        // 服务端这条分支只在有人于渲染期调用 update 时才会走到，正常没有；
        // 显式写出来是为了让「服务端也会落 cookie」这件事在代码里看得见。
        #[cfg(feature = "ssr")]
        let _ = prefs;
    }

    /// 供 `<Html>` 使用的类名。
    pub fn html_class(&self) -> &'static str {
        self.prefs.get().html_class()
    }

    /// 供 `<Html>` 使用的 CSS 变量声明。
    pub fn style_attr(&self) -> Option<String> {
        self.prefs.get().style_attr()
    }
}

// ---------------------------------------------------------------------------
// 账号级偏好
// ---------------------------------------------------------------------------

/// 背景图在库里的存储形态：`(bg_kind, bg_value)`。
///
/// 上传的图片统一由 `/uploads/` 提供，据此与普通外链区分开。
pub fn bg_kind_and_value(background: Option<&str>) -> (&'static str, Option<String>) {
    match background {
        None => ("none", None),
        Some(value) if value.starts_with("/uploads/") => ("upload", Some(value.to_string())),
        Some(value) => ("url", Some(value.to_string())),
    }
}

/// 把外观偏好写进账号。未登录时什么也不做——偏好的载体本来就是 cookie。
///
/// 参数用三个基本类型而不是一个 `ThemePrefs`：一来不必依赖结构体的序列化格式，
/// 二来**服务端必须重新校验**——客户端送来的东西一律不可信，哪怕它由我们自己的
/// 前端构造。空串统一表示「未设置」。
#[server]
pub async fn save_theme_prefs(
    mode: String,
    accent: String,
    background: String,
) -> Result<(), ServerFnError> {
    use crate::state::AppState;

    let app = use_context::<AppState>().expect("AppState 应作为 context 提供");
    let Some(user_id) = crate::auth::current_user_id().await else {
        return Ok(());
    };

    let mode = ThemeMode::parse(&mode);
    let accent = validate_accent(&accent);
    let background = validate_background(&background);
    let (bg_kind, bg_value) = bg_kind_and_value(background.as_deref());

    sqlx::query(
        "INSERT INTO user_prefs (user_id, theme_mode, accent, bg_kind, bg_value, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, datetime('now')) \
         ON CONFLICT(user_id) DO UPDATE SET \
           theme_mode = excluded.theme_mode, \
           accent = excluded.accent, \
           bg_kind = excluded.bg_kind, \
           bg_value = excluded.bg_value, \
           updated_at = datetime('now')",
    )
    .bind(user_id)
    .bind(mode.as_str())
    .bind(accent.as_deref())
    .bind(bg_kind)
    .bind(bg_value.as_deref())
    .execute(&app.pool)
    .await
    .map_err(|e| ServerFnError::new(format!("保存外观偏好失败: {e}")))?;

    Ok(())
}

/// 把账号里的外观偏好镜像进 cookie。
///
/// 主题的读取路径是**同步**的（服务端从请求头读 cookie）。不能为了账号偏好引入一次
/// 异步查询，否则首屏既要多等一轮，也失去「渲染前就知道答案」这个好处。于是定成：
/// **账号是真相来源，cookie 是它的缓存**——登录时镜像一次，之后每次修改两边都写。
#[cfg(feature = "ssr")]
pub async fn mirror_account_prefs(pool: &sqlx::SqlitePool, user_id: i64) -> anyhow::Result<()> {
    use sqlx::Row;

    let row = sqlx::query("SELECT theme_mode, accent, bg_value FROM user_prefs WHERE user_id = ?1")
        .bind(user_id)
        .fetch_optional(pool)
        .await?;

    // 账号还没有偏好记录：保持访客此刻 cookie 里的偏好，等他改一次再入库
    let Some(row) = row else {
        return Ok(());
    };

    let prefs = ThemePrefs {
        mode: ThemeMode::parse(&row.get::<String, _>("theme_mode")),
        // 从库里读出来的值同样要过一遍校验：库里的旧数据不代表一定合法
        accent: row
            .get::<Option<String>, _>("accent")
            .and_then(|value| validate_accent(&value)),
        background: row
            .get::<Option<String>, _>("bg_value")
            .and_then(|value| validate_background(&value)),
    };

    persist_to_cookies(&prefs);
    Ok(())
}
