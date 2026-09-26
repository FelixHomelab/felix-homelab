//! 评论区的界面部分。

use leptos::prelude::*;

use crate::auth::UserState;
use crate::comments::{load_comment_thread, submit_comment, CommentView};

/// 某篇文章（或光遇页面）下的评论区。
///
/// 列表用**阻塞型资源**：评论要出现在首屏 HTML 里，禁用 JS 的访客与爬虫才看得到。
#[component]
pub fn CommentSection(
    /// `post` 或 `sky`。是编译期常量，不给用户输入留口子。
    target_kind: &'static str,
    target_slug: String,
) -> impl IntoView {
    let user_state = use_context::<UserState>().expect("UserState 应由 App 提供");

    // 提交成功后自增，触发列表重新拉取
    let revision = RwSignal::new(0u32);
    // 正在回复哪条评论：(评论 id, 作者名)。None 表示发顶层评论。
    let reply_to = RwSignal::new(None::<(i64, String)>);
    let draft = RwSignal::new(String::new());
    let message = RwSignal::new(String::new());
    let busy = RwSignal::new(false);

    let kind = target_kind.to_string();
    let slug = target_slug.clone();

    let comments = Resource::new_blocking(
        {
            let kind = kind.clone();
            let slug = slug.clone();
            move || (revision.get(), kind.clone(), slug.clone())
        },
        |(_, kind, slug)| load_comment_thread(kind, slug),
    );

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let body = draft.get_untracked();

        if body.trim().is_empty() {
            message.set("评论不能为空。".to_string());
            return;
        }

        let parent = reply_to.get_untracked().map(|(id, _)| id);
        let kind = kind.clone();
        let slug = slug.clone();

        message.set(String::new());
        busy.set(true);
        leptos::task::spawn_local(async move {
            match submit_comment(kind, slug, body, parent).await {
                Ok(Ok(())) => {
                    draft.set(String::new());
                    reply_to.set(None);
                    message.set("已提交，审核通过后会显示。".to_string());
                    revision.update(|n| *n += 1);
                }
                Ok(Err(text)) => message.set(text),
                Err(error) => message.set(format!("请求失败：{error}")),
            }
            busy.set(false);
        });
    };

    // 读一次就够，不必做成响应式：登录、登出都会整页重载。
    // 阻塞型资源保证这里拿到的一定是最终值，不会出现「先当成未登录、再跳变」。
    let logged_in = matches!(user_state.get(), Some(Ok(Some(_))));

    view! {
        <section class="comments">
            <h2>"评论"</h2>

            <Suspense fallback=|| view! { <p class="muted">"载入中…"</p> }>
            {move || {
                let Some(result) = comments.get() else {
                    return view! { <p class="muted">"载入中…"</p> }.into_any();
                };
                let thread = match result {
                    Ok(thread) => thread,
                    Err(error) => {
                        return view! {
                            <p class="error">"载入评论失败："{error.to_string()}</p>
                        }
                        .into_any();
                    }
                };
                let (list, has_pending) = (thread.comments, thread.viewer_has_pending);

                // 提交者刷新后要能看到「还在排队」，否则会以为提交失败而反复重发
                let pending_hint = has_pending.then(|| {
                    view! { <p class="notice" role="status">"你有评论正在等待审核。"</p> }
                });

                if list.is_empty() {
                    return view! {
                        {pending_hint}
                        <p class="muted">"还没有评论。"</p>
                    }
                    .into_any();
                }

                // 父评论可能还没过审。这种情况下把回复当顶层显示——否则它会凭空消失，
                // 作者会以为自己的回复丢了。
                let visible: std::collections::HashSet<i64> = list.iter().map(|c| c.id).collect();
                let roots: Vec<CommentView> = list
                    .iter()
                    .filter(|c| c.parent_id.is_none_or(|p| !visible.contains(&p)))
                    .cloned()
                    .collect();

                view! {
                    {pending_hint}
                    <div class="comment-list">
                        {roots
                            .into_iter()
                            .map(|root| {
                                let replies: Vec<CommentView> = list
                                    .iter()
                                    .filter(|c| c.parent_id == Some(root.id))
                                    .cloned()
                                    .collect();
                                view! {
                                    <article class="comment">
                                        <CommentBody comment=root reply_to=reply_to />
                                        {(!replies.is_empty())
                                            .then(|| {
                                                view! {
                                                    <div class="comment-replies">
                                                        {replies
                                                            .into_iter()
                                                            .map(|reply| {
                                                                view! { <CommentBody comment=reply reply_to=reply_to /> }
                                                            })
                                                            .collect_view()}
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

            // 发言区：登录了才出现
            {logged_in.then(|| view! {
                <form class="comment-form" on:submit=on_submit>
                        {move || {
                            reply_to
                                .get()
                                .map(|(_, author)| {
                                    view! {
                                        <p class="muted reply-hint">
                                            "正在回复 "<strong>{author}</strong>
                                            <button
                                                type="button"
                                                class="link-button"
                                                on:click=move |_| reply_to.set(None)
                                            >
                                                "取消"
                                            </button>
                                        </p>
                                    }
                                })
                        }}
                        <textarea
                            class="comment-input"
                            rows="4"
                            placeholder="说点什么…"
                            prop:value=move || draft.get()
                            on:input=move |ev| draft.set(event_target_value(&ev))
                        ></textarea>
                        {move || {
                            let text = message.get();
                            (!text.is_empty())
                                .then(|| view! { <p class="notice" role="status">{text}</p> })
                        }}
                        <button class="btn btn-primary" type="submit" disabled=move || busy.get()>
                            {move || if busy.get() { "提交中…" } else { "提交评论" }}
                        </button>
                </form>
            })}
            {(!logged_in)
                .then(|| {
                    view! { <p class="muted">"登录后可以评论。" <a href="/login">"去登录"</a></p> }
                })}
        </section>
    }
}

/// 单条评论。正文用 `inner_html` 渲染——内容在入库前已经过滤掉裸 HTML。
#[component]
fn CommentBody(
    comment: CommentView,
    reply_to: RwSignal<Option<(i64, String)>>,
) -> impl IntoView {
    let id = comment.id;
    let username = comment.author_username.clone();
    let author = comment.author.clone();
    let reply_author = comment.author.clone();
    let href = format!("/user/{username}");
    let created_at = comment.created_at.clone();

    view! {
        <div class="comment-main">
            <p class="comment-meta">
                <a class="comment-author" href=href>{author}</a>
                <span class="comment-time">{created_at}</span>
                <button
                    type="button"
                    class="link-button"
                    on:click=move |_| reply_to.set(Some((id, reply_author.clone())))
                >
                    "回复"
                </button>
            </p>
            <div class="prose comment-body" inner_html=comment.body_html></div>
        </div>
    }
}
