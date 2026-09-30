//! 购买订阅页：展示可订阅的服务与价格。
//!
//! 下单与支付流程待接入（随注册/审核完成后实现）；本页先承接导航入口。

use crate::components::PageHeader;
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
                </div>
                <div class="plan-card">
                    <h3>"充值 ¥19"</h3>
                    <p class="plan-price">"= 30 天" <span>"运行时间"</span></p>
                    <p class="plan-note">"基准费率：¥0.63 / 天"</p>
                </div>
                <div class="plan-card">
                    <h3>"充值 ¥99"</h3>
                    <p class="plan-price">"≈ 156 天" <span>"运行时间"</span></p>
                    <p class="plan-note">"也可充值任意金额，按费率折算"</p>
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
