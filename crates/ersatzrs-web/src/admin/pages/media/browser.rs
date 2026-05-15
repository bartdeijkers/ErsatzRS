//! Shared media browser helper: search input, letter-bar, card grid, pager.
//! Ports the shell of `Pages/MovieList.razor`, `TelevisionShowList.razor`, and
//! `ImageList.razor` (which all follow the same Card/LetterBar/MudPagination
//! pattern in `ErsatzTV`).
#![allow(clippy::too_many_lines, dead_code)]
use std::collections::HashSet;
use std::future::Future;

use leptos::prelude::*;

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
    title: &'static str,
    loader: L,
    /// Href prefix for per-card links — final link is `{href_prefix}/{id}`.
    /// Pass `""` to disable linking.
    #[prop(default = "")]
    href_prefix: &'static str,
) -> impl IntoView
where
    L: Fn(String, u32, u32) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<(Vec<MediaCardRow>, u64), ServerFnError>> + Send + 'static,
{
    let search = RwSignal::new(String::new());
    let page = RwSignal::new(0u32);

    Effect::new(move |_| {
        let _ = search.get();
        page.set(0);
    });

    let data: Resource<(Vec<MediaCardRow>, u64)> = Resource::new(
        move || (search.get(), page.get()),
        move |(q, p)| {
            let loader = loader.clone();
            async move { loader(q, p, PAGE_SIZE).await.unwrap_or_default() }
        },
    );

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
                <h1 class=style::page_title>{title}</h1>
                <input
                    class=style::search
                    type="search"
                    placeholder="Search…"
                    prop:value=move || search.get()
                    on:input=move |e| search.set(event_target_value(&e))
                />
            </div>

            <LetterBar present=present/>

            <MediaCardPager total=total page=page page_size=PAGE_SIZE/>

            <Suspense fallback=move || view! { <p class=style::empty>"Loading…"</p> }>
                {move || {
                    data.get()
                        .map(|(rows, _)| {
                            if rows.is_empty() {
                                return view! {
                                    <p class=style::empty>"No items to display."</p>
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
                                            view! {
                                                <MediaCard
                                                    title=r.title
                                                    subtitle=r.subtitle
                                                    artwork_url=r.artwork_path
                                                    href=href
                                                    placeholder=placeholder
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
