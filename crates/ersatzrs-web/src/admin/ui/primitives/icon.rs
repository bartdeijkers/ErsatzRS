use leptos::prelude::*;

stylance::import_crate_style!(style, "src/admin/ui/primitives/icon.module.css");

/// A curated set of SVG icons (subset of Material / Lucide).
/// Decorative glyphs inherit their surrounding text color.
#[allow(dead_code)] // Variants are reserved for upcoming page components.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum IconName {
    Add,
    Edit,
    Delete,
    Save,
    Menu,
    Close,
    Warning,
    Error,
    Check,
    AlertSuccess,
    Info,
    Play,
    Stop,
    Search,
    FilterList,
    Settings,
    ChevronRight,
    ChevronDown,
    ChevronLeft,
    Copy,
    Refresh,
    Visibility,
    VisibilityOff,
    Upload,
    Download,
    Folder,
    MoreVert,
    Tune,
    FindReplace,
    ArrowUp,
    ArrowDown,
    Calendar,
    Clock,
    RefreshOff,
    Film,
    Tv,
    Teletext,
    MusicNote,
    Image,
    Radio,
    OpenInNew,
}

#[component]
pub fn Icon(name: IconName, #[prop(optional)] class: &'static str) -> impl IntoView {
    let href = format!("{}#glyph", icon_url(name));
    view! {
        <svg class=format!("{} {class}", style::icon) viewBox="0 0 24 24" aria-hidden="true" focusable="false">
            <use href=href></use>
        </svg>
    }
}

/// Mask styling for existing callers that supply an explicit size.
#[must_use]
pub fn icon_style(name: IconName, size: &'static str) -> String {
    let url = icon_url(name);
    format!(
        "display:inline-block;width:{size};height:{size};flex-shrink:0;background-color:currentColor;vertical-align:middle;mask-image:url({url});mask-position:center;mask-size:contain;mask-repeat:no-repeat"
    )
}

fn icon_url(name: IconName) -> &'static str {
    match name {
        IconName::Add => "/public/images/icons/add.svg",
        IconName::Edit => "/public/images/icons/edit.svg",
        IconName::Delete => "/public/images/icons/delete.svg",
        IconName::Save => "/public/images/icons/save.svg",
        IconName::Menu => "/public/images/icons/menu.svg",
        IconName::Close => "/public/images/icons/close.svg",
        IconName::Warning => "/public/images/icons/warning.svg",
        IconName::Error => "/public/images/icons/error.svg",
        IconName::Check => "/public/images/icons/check.svg",
        IconName::AlertSuccess => "/public/images/icons/alert-success.svg",
        IconName::Info => "/public/images/icons/info.svg",
        IconName::Play => "/public/images/icons/play.svg",
        IconName::Stop => "/public/images/icons/stop.svg",
        IconName::Search => "/public/images/icons/search.svg",
        IconName::FilterList => "/public/images/icons/filter-list.svg",
        IconName::Settings => "/public/images/icons/settings.svg",
        IconName::ChevronRight => "/public/images/icons/chevron-right.svg",
        IconName::ChevronDown => "/public/images/icons/chevron-down.svg",
        IconName::ChevronLeft => "/public/images/icons/chevron-left.svg",
        IconName::Copy => "/public/images/icons/copy.svg",
        IconName::Refresh => "/public/images/icons/refresh.svg",
        IconName::Visibility => "/public/images/icons/visibility.svg",
        IconName::VisibilityOff => "/public/images/icons/visibility-off.svg",
        IconName::Upload => "/public/images/icons/upload.svg",
        IconName::Download => "/public/images/icons/download.svg",
        IconName::Folder => "/public/images/icons/folder.svg",
        IconName::MoreVert => "/public/images/icons/more-vert.svg",
        IconName::Tune => "/public/images/icons/tune.svg",
        IconName::FindReplace => "/public/images/icons/find-replace.svg",
        IconName::ArrowUp => "/public/images/icons/arrow-up.svg",
        IconName::ArrowDown => "/public/images/icons/arrow-down.svg",
        IconName::Calendar => "/public/images/icons/calendar.svg",
        IconName::Clock => "/public/images/icons/clock.svg",
        IconName::RefreshOff => "/public/images/icons/refresh-off.svg",
        IconName::Film => "/public/images/icons/film.svg",
        IconName::Teletext => "/public/images/icons/teletext.svg",
        IconName::Tv => "/public/images/icons/tv.svg",
        IconName::MusicNote => "/public/images/icons/music-note.svg",
        IconName::Image => "/public/images/icons/image.svg",
        IconName::Radio => "/public/images/icons/radio.svg",
        IconName::OpenInNew => "/public/images/icons/open-in-new.svg",
    }
}
