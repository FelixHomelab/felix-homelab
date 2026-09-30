//! 我的订阅：已订阅的 AI Agent 与容量（OpenCloud / Forgejo）。
//!
//! Agent 列表复用首页原来的卡片区块（数据依赖会话，水合后才渲染）；
//! 容量订阅的订单/用量数据随购买系统上线后补全，这里先给出清晰空态。

use crate::components::PageHeader;
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
                <h2>"容量订阅（OpenCloud / Forgejo）"</h2>
                <span class="muted">"购买订阅后显示容量与使用情况"</span>
            </div>
            <div class="card">
                <p class="muted">"暂无容量订阅。"</p>
                <p>
                    "OpenCloud 与 Forgejo 共用一份容量；购买入口开放后，这里会显示："
                    "已购容量、已使用、剩余额度与到期时间。"
                </p>
                <p>
                    <a class="btn" href="/services">
                        "去购买订阅"
                    </a>
                </p>
            </div>
        </section>
    }
}
