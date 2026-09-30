//! 购买订阅页：展示可订阅的服务与价格。
//!
//! 下单与支付流程待接入（随注册/审核完成后实现）；本页先承接导航入口。

use crate::components::PageHeader;
#[cfg(feature = "hydrate")]
use crate::orders::create_order;
use crate::orders::{capacity_price_cents, format_cents};
use leptos::prelude::*;
use leptos_meta::{Meta, Title};

/// 云盘容量档位（5GB 起步，每档 +5GB）。
struct CapacityPlan {
    gb: u32,
    monthly: f64,
    yearly: f64,
}

/// 5 起步，每增加 5GB 的增加部分按 9 折计：月付 ¥4.5、年付 ¥44.1；单用户最高 30GB。
fn capacity_plans() -> Vec<CapacityPlan> {
    (0..=5)
        .map(|n| CapacityPlan {
            gb: 5 + n * 5,
            monthly: 5.0 + f64::from(n) * 4.5,
            yearly: 49.0 + f64::from(n) * 44.1,
        })
        .collect()
}

/// 价格显示：整数不带小数，其余最多一位（口径：不取整）。
fn price(value: f64) -> String {
    if (value - value.round()).abs() < f64::EPSILON {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    }
}

/// 容量价格表（OpenCloud 与 Forgejo 共用）。
fn capacity_table() -> impl IntoView {
    let rows = capacity_plans()
        .into_iter()
        .map(|plan| {
            view! {
                <tr>
                    <td>{format!("{}GB", plan.gb)}</td>
                    <td>{format!("¥{} / 月", price(plan.monthly))}</td>
                    <td>{format!("¥{} / 年", price(plan.yearly))}</td>
                </tr>
            }
        })
        .collect_view();
    view! {
        <table>
            <thead>
                <tr>
                    <th>"容量"</th>
                    <th>"月付"</th>
                    <th>"年付"</th>
                </tr>
            </thead>
            <tbody>{rows}</tbody>
        </table>
    }
}

/// 购买订阅页。
#[component]
pub fn ServicesPage() -> impl IntoView {
    view! {
        <Title text="购买订阅 — Wraindrock" />
        <Meta
            name="description"
            content="Wraindrock 订阅：AI Agent 按周期订阅；OpenCloud 与 Forgejo 按容量订阅，价格与容量说明。"
        />
        <section class="wrap">
            <PageHeader
                title="购买订阅"
                lede="Wraindrock 的三项自托管订阅：AI Agent 按周期；OpenCloud 与 Forgejo 按容量（共用一份）。下单入口即将开放。"
            />
        </section>

        <section class="wrap section">
            <div class="section-head">
                <h2>"AI Agent（时间池充值）"</h2>
                <span class="muted">"自助开启 · 按量自动计费"</span>
            </div>
            <div class="plan-grid">
                <div class="plan-card">
                    <h3>"充值 ¥6"</h3>
                    <p class="plan-price">"≈ 9.5 天" <span>"运行时间"</span></p>
                    <p class="plan-note">"适合短期试用"</p>
                    <OrderButton product="agent_time".to_string() option="600".to_string() label="充值 ¥6".to_string() />
                </div>
                <div class="plan-card">
                    <h3>"充值 ¥19"</h3>
                    <p class="plan-price">"= 30 天" <span>"运行时间"</span></p>
                    <p class="plan-note">"基准费率：¥0.63 / 天"</p>
                    <OrderButton product="agent_time".to_string() option="1900".to_string() label="充值 ¥19".to_string() />
                </div>
                <div class="plan-card">
                    <h3>"充值 ¥99"</h3>
                    <p class="plan-price">"≈ 156 天" <span>"运行时间"</span></p>
                    <p class="plan-note">"也可充值任意金额，按费率折算"</p>
                    <OrderButton product="agent_time".to_string() option="9900".to_string() label="充值 ¥99".to_string() />
                </div>
            </div>
            <div class="prose">
                <ul>
                    <li>
                        "Agent 由你自己选择并随时开启 / 停止；充值的时长进入账号「时间池」，按实际使用自动扣费。"
                    </li>
                    <li>
                        "扣费档位：运行 1×、睡眠 0.5×（睡眠仍占用本站资源）、彻底停止不计时。"
                    </li>
                    <li>
                        "时间用完后数据默认保留 1 天；需要继续保留按 0.3× 计费（约 ¥0.19 / 天）。"
                    </li>
                    <li>
                        "可选开启「允许透支」：最低可至 −¥5，用于延长使用与数据保存；欠费超限即停止。"
                    </li>
                    <li>
                        "本站维护期间不计费；当前为单一费率试运行，如遇资源紧张会提前公告调整。"
                    </li>
                </ul>
            </div>
        </section>

        <section class="wrap section">
            <div class="section-head">
                <h2>"OpenCloud 云盘"</h2>
                <span class="muted">"按容量与时长订阅"</span>
            </div>
            <div class="prose">
                <p>
                    "5GB 起步；每增加 5GB，增加部分按 9 折计（月付 ¥4.5、年付 ¥44.1）。"
                </p>
                <CapacityOrderCard />
                {capacity_table()}
                <p class="muted">
                    "单用户最高 30GB；全站容量上限 200GB，售完即止。"
                    "每位用户另分配 1GB 免费临时空间（全站预留 50GB，用于中转与分享）。"
                </p>
            </div>
        </section>

        <section class="wrap section">
            <div class="section-head">
                <h2>"Forgejo 代码托管"</h2>
                <span class="muted">"与 OpenCloud 同价、容量共享"</span>
            </div>
            <div class="prose">
                <p>
                    "价格与 OpenCloud 完全一致，且容量在两个服务之间 "
                    <strong>"共享"</strong>
                    "：购买的容量既能放文件，也能放代码仓库，不重复计费。"
                </p>
                {capacity_table()}
            </div>
        </section>

        <section class="wrap section">
            <div class="section-head">
                <h2>"个人外置云存储"</h2>
                <span class="muted">"独立订阅 · 不占用本站容量池"</span>
            </div>
            <div class="plan-grid">
                <div class="plan-card">
                    <h3>"按月"</h3>
                    <p class="plan-price">"¥9" <span>"/ 月"</span></p>
                    <p class="plan-note">"随时可停"</p>
                    <OrderButton product="external_storage".to_string() option="month".to_string() label="订阅".to_string() />
                </div>
                <div class="plan-card">
                    <h3>"按季"</h3>
                    <p class="plan-price">"¥29" <span>"/ 季"</span></p>
                    <p class="plan-note">"≈ ¥9.7 / 月"</p>
                    <OrderButton product="external_storage".to_string() option="quarter".to_string() label="订阅".to_string() />
                </div>
                <div class="plan-card">
                    <h3>"按年"</h3>
                    <p class="plan-price">"¥119" <span>"/ 年"</span></p>
                    <p class="plan-note">"≈ ¥9.9 / 月（暂定）"</p>
                    <OrderButton product="external_storage".to_string() option="year".to_string() label="订阅".to_string() />
                </div>
            </div>
            <div class="prose">
                <ul>
                    <li>
                        "绑定你自己的云存储（WebDAV 协议，遵循 "
                        <a href="https://www.rfc-editor.org/rfc/rfc4918" target="_blank" rel="noreferrer">"RFC 4918"</a>
                        "；后续扩展 S3 等），站内媒体与文件优先写入你的外置存储。"
                    </li>
                    <li>"默认加密存储（可关闭）：文件在写入你的云存储前先加密，密钥由本站托管。"</li>
                    <li>"外置存储作为独立的「存储池」展示，不占用本站容量池额度。"</li>
                    <li>"管理员可用自有外置存储扩容本站全站最大容量（提升 OpenCloud / Forgejo 等全站上限）。"</li>
                </ul>
            </div>
        </section>

        <section class="wrap section">
            <div class="section-head">
                <h2>"购买说明"</h2>
            </div>
            <div class="prose">
                <ul>
                    <li>"下单与支付入口即将开放；当前如需购买，请通过「关于」页的联系方式找站长。"</li>
                    <li>"订阅到期前可续费；到期未续费的服务会先冻结、保留数据一段时间后再清理。"</li>
                    <li>"AI Agent 与容量订阅均为虚拟服务，开通后不支持退款。"</li>
                </ul>
            </div>
        </section>
    }
}


/// 下单按钮：调用 `create_order`；有在线收款地址则跳转，否则显示人工通道说明。
#[component]
fn OrderButton(product: String, option: String, label: String) -> impl IntoView {
    let busy = RwSignal::new(false);
    let message = RwSignal::new(String::new());

    let onclick = move |_| {
        #[cfg(feature = "hydrate")]
        {
            let product = product.clone();
            let option = option.clone();
            busy.set(true);
            message.set(String::new());
            leptos::task::spawn_local(async move {
                match create_order(product, option).await {
                    Ok(result) => match result.checkout_url {
                        Some(url) => {
                            if let Some(window) = web_sys::window() {
                                let _ = window.location().set_href(&url);
                            }
                        }
                        None => message.set(result.message),
                    },
                    Err(error) => message.set(format!("请求失败：{error}")),
                }
                busy.set(false);
            });
        }
    };

    view! {
        <div class="order-cta">
            <button class="btn btn-small" type="button" on:click=onclick disabled=move || busy.get()>
                {move || if busy.get() { "下单中…".to_string() } else { label.clone() }}
            </button>
            {move || {
                let text = message.get();
                (!text.is_empty()).then(|| view! { <span class="muted media-tools-msg">{text}</span> })
            }}
        </div>
    }
}

/// 容量下单：选容量（5-30GB，步进 5）+ 周期（月/年），实时显示价格。
#[component]
fn CapacityOrderCard() -> impl IntoView {
    let gb = RwSignal::new(5_i64);
    let period = RwSignal::new("month".to_string());
    let busy = RwSignal::new(false);
    let message = RwSignal::new(String::new());

    let price_text = move || {
        capacity_price_cents(gb.get(), &period.get())
            .map(format_cents)
            .unwrap_or_else(|| "—".to_string())
    };

    let submit = move |_| {
        #[cfg(feature = "hydrate")]
        {
            let option = format!("{}:{}", gb.get_untracked(), period.get_untracked());
            busy.set(true);
            message.set(String::new());
            leptos::task::spawn_local(async move {
                match create_order("capacity".to_string(), option).await {
                    Ok(result) => match result.checkout_url {
                        Some(url) => {
                            if let Some(window) = web_sys::window() {
                                let _ = window.location().set_href(&url);
                            }
                        }
                        None => message.set(result.message),
                    },
                    Err(error) => message.set(format!("请求失败：{error}")),
                }
                busy.set(false);
            });
        }
    };

    view! {
        <div class="order-row">
            <label class="order-field">
                <span>"容量"</span>
                <select on:change=move |ev| {
                    gb.set(event_target_value(&ev).parse().unwrap_or(5))
                }>
                    {(1..=6)
                        .map(|n| {
                            let value = n * 5;
                            view! { <option value=value.to_string()>{format!("{value}GB")}</option> }
                        })
                        .collect_view()}
                </select>
            </label>
            <label class="order-field">
                <span>"周期"</span>
                <select on:change=move |ev| period.set(event_target_value(&ev))>
                    <option value="month">"按月"</option>
                    <option value="year">"按年"</option>
                </select>
            </label>
            <span class="order-price">{move || format!("价格 {}", price_text())}</span>
            <button class="btn btn-small" type="button" on:click=submit disabled=move || busy.get()>
                {move || if busy.get() { "下单中…" } else { "订阅容量" }}
            </button>
            {move || {
                let text = message.get();
                (!text.is_empty()).then(|| view! { <span class="muted media-tools-msg">{text}</span> })
            }}
        </div>
    }
}
