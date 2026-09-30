//! 我的订阅：已订阅的 AI Agent 与容量（OpenCloud / Forgejo）。
//!
//! Agent 列表复用首页原来的卡片区块（数据依赖会话，水合后才渲染）；
//! 容量订阅的订单/用量数据随购买系统上线后补全，这里先给出清晰空态。

use crate::components::PageHeader;
use crate::storage::capacity_pool;
use leptos::prelude::*;
use leptos_meta::{Meta, Title};

use super::MyAgentsSection;

/// 我的订阅页。
#[component]
pub fn SubscriptionsPage() -> impl IntoView {
    view! {
        <Title text="我的订阅 — Wraindrock" />
        <Meta
            name="description"
            content="查看已订阅的 AI Agent 与 OpenCloud / Forgejo 容量及使用情况。"
        />
        <section class="wrap">
            <PageHeader
                title="我的订阅"
                lede="已订阅的 AI Agent 与容量（OpenCloud / Forgejo）集中在这里。"
            />
        </section>

        <MyAgentsSection />

        <section class="wrap section">
            <div class="section-head">
                <h2>"容量池（OpenCloud / Forgejo / 站内媒体）"</h2>
                <span class="muted">"配额按原始大小计；压缩节省归平台"</span>
            </div>
            <CapacityPoolCard />
            <p>
                <a class="btn" href="/services">
                    "去购买订阅"
                </a>
            </p>
        </section>
    }
}

/// 容量池卡片：与「时间池」同款——整行进度条 + 全部/本月/本周/今天 切换。
#[component]
fn CapacityPoolCard() -> impl IntoView {
    let window = RwSignal::new("all".to_string());
    let pool = Resource::new(move || window.get(), |w| capacity_pool(w));

    let tabs = [
        ("all", "全部"),
        ("month", "本月"),
        ("week", "本周"),
        ("day", "今天"),
    ];

    view! {
        <div class="card pool-card">
            <div class="pool-head">
                <h3>"容量池"</h3>
                <div class="pool-tabs" role="tablist">
                    {tabs
                        .into_iter()
                        .map(|(key, label)| {
                            view! {
                                <button
                                    type="button"
                                    class="pool-tab"
                                    class:active=move || window.get() == key
                                    on:click=move |_| window.set(key.to_string())
                                >
                                    {label}
                                </button>
                            }
                        })
                        .collect_view()}
                </div>
            </div>
            <Suspense fallback=move || view! { <p class="muted">"载入中…"</p> }>
                {move || {
                    let data = pool.get().and_then(|result| result.ok());
                    let (total, used) = data
                        .map(|pool| (pool.total_bytes, pool.used_bytes))
                        .unwrap_or((0, 0));
                    let clamped_used = used.clamp(0, total.max(0));
                    let percent = if total > 0 {
                        (clamped_used as f64 / total as f64 * 100.0).clamp(0.0, 100.0)
                    } else {
                        0.0
                    };
                    let opened = total > 0;
                    view! {
                        <div>
                            <div class="pool-bar" role="progressbar" aria-valuenow=percent>
                                <div class="pool-fill" style=format!("width:{percent:.1}%")></div>
                            </div>
                            <div class="pool-nums">
                                <span>
                                    "已用 " <strong>{format_bytes(used)}</strong>
                                </span>
                                <span>
                                    "总容量 " <strong>{format_bytes(total)}</strong>
                                </span>
                            </div>
                            {(!opened)
                                .then(|| {
                                    view! {
                                        <p class="muted pool-hint">
                                            "尚未开通容量池 · 过渡期上传不限量；购买开放后这里显示用量进度"
                                        </p>
                                    }
                                })}
                        </div>
                    }
                }}
            </Suspense>
        </div>
    }
}

/// 字节格式化：B / KB / MB / GB / TB（保留一位小数）。
fn format_bytes(bytes: i64) -> String {
    let value = bytes.max(0) as f64;
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut size = value;
    let mut unit = 0usize;
    while size >= 1024.0 && unit < units.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", value as i64, units[0])
    } else {
        format!("{size:.1} {}", units[unit])
    }
}
