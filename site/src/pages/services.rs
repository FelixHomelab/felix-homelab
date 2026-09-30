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
                <h2>"AI Agent"</h2>
                <span class="muted">"独立实例 · 周期订阅"</span>
            </div>
            <div class="plan-grid">
                <div class="plan-card">
                    <h3>"1 周"</h3>
                    <p class="plan-price">"¥6" <span>"/ 周"</span></p>
                    <p class="plan-note">"适合短期试用"</p>
                </div>
                <div class="plan-card">
                    <h3>"1 个月"</h3>
                    <p class="plan-price">"¥19" <span>"/ 月"</span></p>
                    <p class="plan-note">"≈ ¥4.4 / 周"</p>
                </div>
                <div class="plan-card">
                    <h3>"1 年"</h3>
                    <p class="plan-price">"¥99" <span>"/ 年"</span></p>
                    <p class="plan-note">"≈ ¥8.25 / 月（最划算）"</p>
                </div>
            </div>
            <p class="muted">
                "独立运行环境、数据隔离、常驻可用；支持 OpenCode 与 DSH 两种工作台，开通后按周期续费。"
            </p>
            <p class="muted">
                "订阅时长是账号内全部 Agent 共享的总时长池：只订阅一个 Agent 时可运行满整个时长；"
                "订阅多个时共享同一池，同时运行的 Agent 越多、消耗越快。"
                "计费档位：运行 1×、睡眠半价（0.5×，睡眠仍占用本站资源）、彻底停止不计时；"
                "耗尽或到期后全部停止，续费即恢复。"
            </p>
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
