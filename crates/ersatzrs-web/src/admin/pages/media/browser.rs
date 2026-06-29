//! Shared media browser helper: search input, letter-bar, card grid, pager.
//! Ports the shell of `Pages/MovieList.razor`, `TelevisionShowList.razor`, and
//! `ImageList.razor` (which all follow the same Card/LetterBar/MudPagination
//! pattern in `ErsatzTV`).
#![allow(clippy::too_many_lines, dead_code)]
use std::collections::HashSet;
use std::future::Future;

use leptos::prelude::*;

use crate::admin::i18n::{content_variant_badge_label, season_display_name, tr, TextKey};
#[cfg(target_arch = "wasm32")]
use crate::admin::server_fns::layout::media_state_revision;
use crate::admin::server_fns::media_browser::MediaCardRow;
use crate::admin::ui::primitives::card::{MediaCard, MediaCardPager};
use crate::admin::ui::primitives::letter_bar::{letter_bucket, FragmentLetterAnchor, LetterBar};

stylance::import_crate_style!(style, "src/admin/pages/media/browser.module.css");

pub const PAGE_SIZE: u32 = 50;

/// Server-function wrapper: accepts `(search, page, page_size)` and returns
/// `(rows, total)`. Keeps the loader generic over movies/shows/images.
pub type Loader = fn(
    String,
    u32,
    u32,
    bool,
) -> std::pin::Pin<
    Box<
        dyn Future<Output = Result<(Vec<MediaCardRow>, u64), leptos::prelude::ServerFnError>>
            + Send,
    >,
>;

/// Renders the shared browser shell. `loader` is typed as a boxed async fn so
/// each page can inject its own server function (e.g. `list_movies`).
#[component]
pub fn MediaBrowserPage<L, Fut>(
    title: TextKey,
    loader: L,
    /// Href prefix for per-card links — final link is `{href_prefix}/{id}`.
    /// Pass `""` to disable linking.
    #[prop(default = "")]
    href_prefix: &'static str,
) -> impl IntoView
where
    L: Fn(String, u32, u32, bool) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<(Vec<MediaCardRow>, u64), ServerFnError>> + Send + 'static,
{
    let search = RwSignal::new(String::new());
    let page = RwSignal::new(0u32);
    let media_revision = RwSignal::new(String::new());

    Effect::new(move |_| {
        let _ = search.get();
        page.set(0);
    });

    let data: Resource<(Vec<MediaCardRow>, u64)> = Resource::new(
        move || (search.get(), page.get(), media_revision.get()),
        move |(q, p, _)| {
            let loader = loader.clone();
            async move { loader(q, p, PAGE_SIZE, true).await.unwrap_or_default() }
        },
    );

    Effect::new(move |_| {
        #[cfg(target_arch = "wasm32")]
        {
            leptos::task::spawn_local(async move {
                loop {
                    if let Ok(next) = media_state_revision().await {
                        if media_revision.get_untracked() != next {
                            media_revision.set(next);
                        }
                    }
                    gloo_timers::future::TimeoutFuture::new(1_500).await;
                }
            });
        }
    });

    let total = Signal::derive(move || data.get().map(|(_, t)| t));

    let present: Signal<Option<HashSet<char>>> = Signal::derive(move || {
        data.get().map(|(rows, _)| {
            rows.iter()
                .map(|r| letter_bucket(&r.sort_title))
                .collect::<HashSet<_>>()
        })
    });

    view! {
        <div class=style::page>
            <div class=style::toolbar>
                <h1 class=style::page_title>{tr(title)}</h1>
                <input
                    class=style::search
                    type="search"
                    placeholder=tr(TextKey::Search)
                    prop:value=move || search.get()
                    on:input=move |e| search.set(event_target_value(&e))
                />
            </div>

            <LetterBar present=present/>

            <MediaCardPager total=total page=page page_size=PAGE_SIZE/>

            <Suspense fallback=move || view! { <p class=style::empty>{tr(TextKey::Loading)}</p> }>
                {move || {
                    data.get()
                        .map(|(rows, _)| {
                            if rows.is_empty() {
                                return view! {
                                    <p class=style::empty>{tr(TextKey::NoItemsToDisplay)}</p>
                                }
                                .into_any();
                            }
                            let hp = href_prefix;
                            view! {
                                <div class=style::grid>
                                    <FragmentLetterAnchor
                                        cards=rows
                                        sort_title=|r: &MediaCardRow| r.sort_title.as_str()
                                        render=move |r: MediaCardRow| {
                                            let href = if hp.is_empty() {
                                                None
                                            } else {
                                                Some(format!("{hp}/{}", r.id))
                                            };
                                            let placeholder = crate::admin::ui::primitives::card::first_letter_placeholder(&r.sort_title);
                                            let title = r
                                                .season_number
                                                .map(season_display_name)
                                                .unwrap_or(r.title);
                                            view! {
                                            <MediaCard
                                                title=title
                                                subtitle=r.subtitle
                                                artwork_url=r.artwork_path
                                                href=href
                                                placeholder=placeholder
                                                badge=content_variant_badge_label(
                                                    r.variant.content_kind,
                                                    r.variant.sub_kind.as_deref(),
                                                )
                                            />
                                            }
                                            .into_any()
                                        }
                                    />
                                </div>
                            }
                            .into_any()
                        })
                }}
            </Suspense>

            <MediaCardPager total=total page=page page_size=PAGE_SIZE/>
        </div>
    }
}
