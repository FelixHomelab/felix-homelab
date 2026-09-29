//! 光遇代跑评价区的界面部分。

use leptos::prelude::*;

use crate::auth::UserState;
use crate::reviews::{load_review_board, submit_sky_review, RATING_MAX, RATING_MIN};

/// 代跑评价区：评分概览 + 评价列表 + 提交表单。
///
/// 列表同样用阻塞型资源，让评价出现在首屏 HTML 里。
#[component]
pub fn ReviewSection() -> impl IntoView {
    let user_state = use_context::<UserState>().expect("UserState 应由 App 提供");

    let revision = RwSignal::new(0u32);
    // 不预设分数：默认给满分等于引导好评，默认给低分又像在抹黑。让用户自己选。
    let rating = RwSignal::new(None::<i64>);
    let draft = RwSignal::new(String::new());
    let message = RwSignal::new(String::new());
    let busy = RwSignal::new(false);

    let reviews = Resource::new_blocking(move || revision.get(), |_| load_review_board());

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let body = draft.get_untracked();

        if body.trim().is_empty() {
            message.set("评价不能为空。".to_string());
            return;
        }
        let Some(score) = rating.get_untracked() else {
            message.set("请先选择评分。".to_string());
            return;
        };
        message.set(String::new());
        busy.set(true);
        leptos::task::spawn_local(async move {
            match submit_sky_review(score, body).await {
                Ok(Ok(())) => {
                    draft.set(String::new());
                    message.set("已提交，审核通过后会显示。".to_string());
                    revision.update(|n| *n += 1);
                }
                Ok(Err(text)) => message.set(text),
                Err(error) => message.set(format!("请求失败：{error}")),
            }
            busy.set(false);
        });
    };

    // 会话依赖区块在“水合完成后”再渲染：SSR 与客户端首帧都为空，
    // 避免资源两端就绪时机不同造成 hydration 失配。登录/登出会整页重载。
    let ready = crate::components::ready_after_hydration();
    let logged_in = move || matches!(user_state.get(), Some(Ok(Some(_))));
    // 两个视图各用一份克隆（闭包非 Copy，不能同时 move 进两处）
    let logged_in_form = logged_in.clone();
    let logged_in_hint = logged_in.clone();

    view! {
        <section class="reviews">
            <h2>"代跑评价"</h2>

            <Suspense fallback=|| view! { <p class="muted">"载入中…"</p> }>
            {move || {
                let Some(result) = reviews.get() else {
                    return view! { <p class="muted">"载入中…"</p> }.into_any();
                };
                let board = match result {
                    Ok(board) => board,
                    Err(error) => {
                        return view! {
                            <p class="error">"载入评价失败："{error.to_string()}</p>
                        }
                        .into_any();
                    }
                };
                let (list, has_pending) = (board.reviews, board.viewer_has_pending);

                let pending_hint = has_pending.then(|| {
                    view! { <p class="notice" role="status">"你有评价正在等待审核。"</p> }
                });

                if list.is_empty() {
                    return view! {
                        {pending_hint}
                        <p class="muted">"还没有评价。"</p>
                    }
                    .into_any();
                }

                let count = list.len();
                let average =
                    list.iter().map(|review| review.rating).sum::<i64>() as f64 / count as f64;

                view! {
                    {pending_hint}
                    <p class="review-summary">
                        <span class="stars">"★"</span>
                        <strong>{format!("{average:.1}")}</strong>
                        <span class="muted">{format!("共 {count} 条评价")}</span>
                    </p>
                    <div class="review-list">
                        {list
                            .into_iter()
                            .map(|review| {
                                let stars = "★".repeat(review.rating as usize);
                                let empty = "☆".repeat((RATING_MAX - review.rating).max(0) as usize);
                                let featured = review.featured;
                                view! {
                                    <article class="review">
                                        <p class="review-meta">
                                            <span class="stars">{stars}{empty}</span>
                                            {featured.then(|| view! {
                                                <span class="badge badge-featured">"精选"</span>
                                            })}
                                            <strong>{review.author}</strong>
                                            <span class="comment-time">{review.created_at}</span>
                                        </p>
                                        <p class="review-body">{review.body}</p>
                                        {review
                                            .reply
                                            .map(|reply| {
                                                view! {
                                                    <div class="review-reply">
                                                        <strong>"站长回复："</strong>
                                                        {reply}
                                                    </div>
                                                }
                                            })}
                                    </article>
                                }
                            })
                            .collect_view()}
                    </div>
                }
                .into_any()
            }}
            </Suspense>

            {move || {
                let on_submit = on_submit.clone();
                (ready.get() && logged_in_form()).then(|| view! {
                <form class="comment-form" on:submit=on_submit>
                        <label class="field">
                            <span>"评分"</span>
                            <select
                                class="text-input"
                                prop:value=move || {
                                    rating.get().map(|value| value.to_string()).unwrap_or_default()
                                }
                                on:change=move |ev| {
                                    let raw = event_target_value(&ev);
                                    rating.set(raw.parse::<i64>().ok());
                                }
                            >
                                <option value="">"请选择评分"</option>
                                {(RATING_MIN..=RATING_MAX)
                                    .rev()
                                    .map(|value| {
                                        view! {
                                            <option value=value.to_string()>
                                                {format!("{value} 星")}
                                            </option>
                                        }
                                    })
                                    .collect_view()}
                            </select>
                        </label>
                        <textarea
                            class="comment-input"
                            rows="4"
                            placeholder="说说代跑体验…"
                            prop:value=move || draft.get()
                            on:input=move |ev| draft.set(event_target_value(&ev))
                        ></textarea>
                        {move || {
                            let text = message.get();
                            (!text.is_empty())
                                .then(|| view! { <p class="notice" role="status">{text}</p> })
                        }}
                        <button class="btn btn-primary" type="submit" disabled=move || busy.get()>
                            {move || if busy.get() { "提交中…" } else { "提交评价" }}
                        </button>
                </form>
                })
            }}
            {move || {
                (ready.get() && !logged_in_hint())
                    .then(|| {
                        view! {
                            <p class="muted">
                                "登录后可以提交评价。" <a href="/login">"去登录"</a>
                            </p>
                        }
                    })
            }}
        </section>
    }
}
