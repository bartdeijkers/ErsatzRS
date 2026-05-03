/// A curated set of SVG icons (subset of Material / Lucide).
/// Usage: `<span style=icon_style(IconName::Add, "1rem") aria-hidden="true"></span>`.
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
    Info,
    Play,
    Stop,
    Search,
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
    MoreVert,
    Tune,
}

/// Returns inline style for a static icon span.
#[must_use]
pub fn icon_style(name: IconName, size: &'static str) -> String {
    let mask = icon_mask_url(icon_path(name));
    format!(
        "display:inline-block;width:{size};height:{size};flex-shrink:0;background-color:currentColor;vertical-align:middle;mask:{mask} center / contain no-repeat;-webkit-mask:{mask} center / contain no-repeat"
    )
}

fn icon_path(name: IconName) -> &'static str {
    match name {
        IconName::Add => "M12 5v14M5 12h14",
        IconName::Edit => "M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z",
        IconName::Delete => "M3 6h18M8 6V4h8v2M19 6l-1 14H6L5 6",
        IconName::Save => "M19 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11l5 5v11a2 2 0 0 1-2 2zM17 21v-8H7v8M7 3v5h8",
        IconName::Menu => "M3 12h18M3 6h18M3 18h18",
        IconName::Close => "M18 6L6 18M6 6l12 12",
        IconName::Warning => "M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0zM12 9v4M12 17h.01",
        IconName::Error => "M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm1 15h-2v-2h2v2zm0-4h-2V7h2v6z",
        IconName::Check => "M20 6L9 17l-5-5",
        IconName::Info => "M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm1 15h-2v-6h2v6zm0-8h-2V7h2v2z",
        IconName::Play => "M5 3l14 9-14 9V3z",
        IconName::Stop => "M6 6h12v12H6z",
        IconName::Search => "M21 21l-4.35-4.35M17 11A6 6 0 1 1 5 11a6 6 0 0 1 12 0z",
        IconName::Settings => "M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6z M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z",
        IconName::ChevronRight => "M9 18l6-6-6-6",
        IconName::ChevronDown => "M6 9l6 6 6-6",
        IconName::ChevronLeft => "M15 18l-6-6 6-6",
        IconName::Copy => "M8 4H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8l-4-4h-6M14 2v4h4M8 12h8M8 16h5",
        IconName::Refresh => "M23 4v6h-6M1 20v-6h6 M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15",
        IconName::Visibility => "M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z M12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6z",
        IconName::VisibilityOff => "M17.94 17.94A10.07 10.07 0 0 1 12 20c-7 0-11-8-11-8a18.45 18.45 0 0 1 5.06-5.94M9.9 4.24A9.12 9.12 0 0 1 12 4c7 0 11 8 11 8a18.5 18.5 0 0 1-2.16 3.19m-6.72-1.07a3 3 0 1 1-4.24-4.24M1 1l22 22",
        IconName::Upload => "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4M17 8l-5-5-5 5M12 3v12",
        IconName::Download => "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4M7 10l5 5 5-5M12 15V3",
        IconName::MoreVert => "M12 13a1 1 0 1 0 0-2 1 1 0 0 0 0 2zM12 6a1 1 0 1 0 0-2 1 1 0 0 0 0 2zM12 20a1 1 0 1 0 0-2 1 1 0 0 0 0 2z",
        IconName::Tune => "M4 21v-7M4 10V3M12 21v-9M12 8V3M20 21v-5M20 12V3M1 14h6M9 8h6M17 16h6",
    }
}

fn icon_mask_url(path: &str) -> String {
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="{path}" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>"#
    );
    format!("url(\"data:image/svg+xml,{}\")", urlencoding::encode(&svg))
}
