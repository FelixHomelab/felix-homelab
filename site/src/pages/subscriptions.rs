//! 我的订阅：已订阅的 AI Agent 与容量（OpenCloud / Forgejo）。
//!
//! Agent 列表复用首页原来的卡片区块（数据依赖会话，水合后才渲染）；
//! 容量订阅的订单/用量数据随购买系统上线后补全，这里先给出清晰空态。

use crate::components::PageHeader;
use crate::agents::{agent_kind_label, my_agent_access};
use crate::archive::{
    archive_status_label, hold_fee_text, mark_notifications_read, my_archive_cases,
    my_notifications, respond_archive_case,
};
use crate::orders::{
    format_bytes, format_cents, my_orders, my_subscription_state, order_status_label,
    product_label,
};
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

        <AgentAccessSection />
        <SubscriptionStatusSection />
        <ArchiveSection />
        <NotificationsSection />
    }
}

/// 数据保管：容量过期后的宽限/待确认/平台保管案例。
#[component]
fn ArchiveSection() -> impl IntoView {
    let revision = RwSignal::new(0_u32);
    let message = RwSignal::new(String::new());
    let cases = Resource::new(move || revision.get(), |_| my_archive_cases());

    view! {
        <section class="wrap section">
            <div class="section-head">
                <h2>"数据保管"</h2>
                <span class="muted">"容量到期后的宽限、确认与删除倒计时"</span>
            </div>
            {move || {
                let text = message.get();
                (!text.is_empty()).then(|| view! { <p class="notice" role="status">{text}</p> })
            }}
            <Suspense fallback=move || view! { <p class="muted">"载入中…"</p> }>
                {move || match cases.get().and_then(|result| result.ok()) {
                    Some(list) if list.is_empty() => view! {
                        <p class="muted">"暂无需要保管的数据。"</p>
                    }
                    .into_any(),
                    Some(list) => view! {
                        <div class="card">
                            <table>
                                <thead>
                                    <tr>
                                        <th>"案例"</th>
                                        <th>"状态"</th>
                                        <th>"文件"</th>
                                        <th>"倒计时"</th>
                                        <th>"操作"</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {list
                                        .into_iter()
                                        .map(|case| {
                                            let case_id = case.id;
                                            let status = case.status.clone();
                                            let can_respond = status == "notified";
                                            let countdown = case
                                                .delete_after
                                                .clone()
                                                .or(case.grace_until.clone())
                                                .unwrap_or_else(|| "—".to_string());
                                            view! {
                                                <tr>
                                                    <td>{format!("#{}", case.id)}</td>
                                                    <td>{archive_status_label(&case.status)}</td>
                                                    <td>
                                                        {format!("{} 个 / {}", case.media_count, format_bytes(case.bytes))}
                                                    </td>
                                                    <td>{countdown}</td>
                                                    <td>
                                                        {can_respond
                                                            .then(|| {
                                                                view! {
                                                                    <div class="admin-actions">
                                                                        <button
                                                                            class="btn btn-small"
                                                                            type="button"
                                                                            on:click=move |_| {
                                                                                let message = message;
                                                                                let revision = revision;
                                                                                leptos::task::spawn_local(async move {
                                                                                    match respond_archive_case(case_id, true).await {
                                                                                        Ok(text) => {
                                                                                            message.set(text);
                                                                                            revision.update(|n| *n += 1);
                                                                                        }
                                                                                        Err(error) => message.set(format!("请求失败：{error}")),
                                                                                    }
                                                                                });
                                                                            }
                                                                        >
                                                                            {format!("需要（{}）", hold_fee_text())}
                                                                        </button>
                                                                        <button
                                                                            class="btn btn-small"
                                                                            type="button"
                                                                            on:click=move |_| {
                                                                                let message = message;
                                                                                let revision = revision;
                                                                                leptos::task::spawn_local(async move {
                                                                                    match respond_archive_case(case_id, false).await {
                                                                                        Ok(text) => {
                                                                                            message.set(text);
                                                                                            revision.update(|n| *n += 1);
                                                                                        }
                                                                                        Err(error) => message.set(format!("请求失败：{error}")),
                                                                                    }
                                                                                });
                                                                            }
                                                                        >
                                                                            "不需要（留存一周后删除）"
                                                                        </button>
                                                                    </div>
                                                                }
                                                            })}
                                                    </td>
                                                </tr>
                                            }
                                        })
                                        .collect_view()}
                                </tbody>
                            </table>
                        </div>
                    }
                    .into_any(),
                    None => ().into_any(),
                }}
            </Suspense>
        </section>
    }
}

/// 站内通知列表。
#[component]
fn NotificationsSection() -> impl IntoView {
    let revision = RwSignal::new(0_u32);
    let notifications = Resource::new(move || revision.get(), |_| my_notifications());

    view! {
        <section class="wrap section">
            <div class="section-head">
                <h2>"通知"</h2>
                <button
                    class="btn btn-small"
                    type="button"
                    on:click=move |_| {
                        let revision = revision;
                        leptos::task::spawn_local(async move {
                            let _ = mark_notifications_read().await;
                            revision.update(|n| *n += 1);
                        });
                    }
                >
                    "全部标为已读"
                </button>
            </div>
            <Suspense fallback=move || view! { <p class="muted">"载入中…"</p> }>
                {move || match notifications.get().and_then(|result| result.ok()) {
                    Some(list) if list.is_empty() => view! {
                        <p class="muted">"暂无通知。"</p>
                    }
                    .into_any(),
                    Some(list) => view! {
                        <ul class="notice-list">
                            {list
                                .into_iter()
                                .map(|item| {
                                    view! {
                                        <li class:unread=!item.read>
                                            <strong>{item.title}</strong>
                                            <span class="muted">" · " {item.created_at}</span>
                                            <p>{item.body}</p>
                                        </li>
                                    }
                                })
                                .collect_view()}
                        </ul>
                    }
                    .into_any(),
                    None => ().into_any(),
                }}
            </Suspense>
        </section>
    }
}

/// Agent 控制台接入：需要手动粘贴令牌的实例（目前 OpenClaw）。
#[component]
fn AgentAccessSection() -> impl IntoView {
    let access = Resource::new(|| (), |_| my_agent_access());
    view! {
        <Suspense fallback=move || ()>
            {move || match access.get().and_then(|result| result.ok()) {
                Some(list) if list.is_empty() => ().into_any(),
                Some(list) => view! {
                    <section class="wrap section">
                        <div class="section-head">
                            <h2>"Agent 控制台接入"</h2>
                            <span class="muted">"打开控制台后粘贴对应令牌即可连接（浏览器会记住）"</span>
                        </div>
                        <div class="card">
                            {list
                                .into_iter()
                                .map(|item| {
                                    let slot = item.slot;
                                    let label = format!(
                                        "{} · 实例 #{}",
                                        agent_kind_label(&item.kind),
                                        slot,
                                    );
                                    let token_show = item.token.clone();
                                    let token_copy = item.token.clone();
                                    let shown = RwSignal::new(false);
                                    view! {
                                        <div class="order-row">
                                            <strong>{label}</strong>
                                            {move || {
                                                if shown.get() {
                                                    view! {
                                                        <code class="token-reveal">{token_show.clone()}</code>
                                                    }
                                                        .into_any()
                                                } else {
                                                    view! {
                                                        <span class="muted">"令牌已隐藏"</span>
                                                    }
                                                        .into_any()
                                                }
                                            }}
                                            <button
                                                class="btn btn-small"
                                                type="button"
                                                on:click=move |_| shown.update(|v| *v = !*v)
                                            >
                                                {move || if shown.get() { "隐藏" } else { "显示令牌" }}
                                            </button>
                                            <button
                                                class="btn btn-small"
                                                type="button"
                                                on:click=move |_| {
                                                    #[cfg(feature = "hydrate")]
                                                    {
                                                        if let Some(window) = web_sys::window() {
                                                            let _ = window
                                                                .navigator()
                                                                .clipboard()
                                                                .write_text(&token_copy);
                                                        }
                                                    }
                                                }
                                            >
                                                "复制"
                                            </button>
                                        </div>
                                    }
                                })
                                .collect_view()}
                        </div>
                    </section>
                }
                .into_any(),
                None => ().into_any(),
            }}
        </Suspense>
    }
}

/// 订阅状态与外置存储 + 最近订单。
#[component]
fn SubscriptionStatusSection() -> impl IntoView {
    let state = Resource::new(|| (), |_| my_subscription_state());
    let orders = Resource::new(|| (), |_| my_orders());

    view! {
        <section class="wrap section">
            <div class="section-head">
                <h2>"外置云存储与订单"</h2>
                <span class="muted">"独立订阅；订单确认后自动发放"</span>
            </div>
            <div class="card">
                <Suspense fallback=move || view! { <p class="muted">"载入中…"</p> }>
                    {move || match state.get().and_then(|result| result.ok()) {
                        Some(state) => match state.external_storage_until {
                            Some(until) => view! {
                                <p>
                                    "个人外置云存储：" <strong>"已开通"</strong>
                                    "（有效期至 " {until} "）"
                                </p>
                            }
                            .into_any(),
                            None => view! {
                                <p class="muted">
                                    "个人外置云存储：未开通（订阅后可绑定你的 WebDAV，默认加密、独立存储池）"
                                </p>
                            }
                            .into_any(),
                        },
                        None => view! { <p class="muted">"载入中…"</p> }.into_any(),
                    }}
                </Suspense>
                <Suspense fallback=move || ()>
                    {move || match orders.get().and_then(|result| result.ok()) {
                        Some(list) if list.is_empty() => view! {
                            <p class="muted">"还没有订单。"</p>
                        }
                        .into_any(),
                        Some(list) => view! {
                            <table>
                                <thead>
                                    <tr>
                                        <th>"订单"</th>
                                        <th>"内容"</th>
                                        <th>"金额"</th>
                                        <th>"状态"</th>
                                        <th>"时间"</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {list
                                        .into_iter()
                                        .map(|order| {
                                            view! {
                                                <tr>
                                                    <td>{format!("#{}", order.id)}</td>
                                                    <td>{product_label(&order.product, &order.option)}</td>
                                                    <td>{format_cents(order.amount_cents)}</td>
                                                    <td>{order_status_label(&order.status)}</td>
                                                    <td>{order.created_at}</td>
                                                </tr>
                                            }
                                        })
                                        .collect_view()}
                                </tbody>
                            </table>
                        }
                        .into_any(),
                        None => ().into_any(),
                    }}
                </Suspense>
            </div>
        </section>
    }
}

/// 支付成功回跳页（Creem `success_url` 指向这里）。
#[component]
pub fn SubscriptionSuccessPage() -> impl IntoView {
    view! {
        <Title text="支付完成 — Wraindrock" />
        <section class="wrap section">
            <PageHeader
                title="支付完成"
                lede="已收到支付结果；权益会在回调确认后自动发放（人工通道需管理员确认收款）。"
            />
            <p>
                <a class="btn" href="/subscriptions">
                    "查看我的订阅"
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


