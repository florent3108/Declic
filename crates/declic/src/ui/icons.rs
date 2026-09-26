//! Line icons drawn for Declic (24×24, stroked), tinted by the theme.

use iced::widget::{Svg, svg};
use iced::{Color, Length, Theme};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Icon {
    Search,
    Plus,
    Settings,
    Keyboard,
    Trash,
    Close,
    Open,
    Text,
    Folder,
    Globe,
    File,
    Warning,
    Info,
    Chevron,
    ChevronDown,
    Bolt,
    Window,
    Mail,
    Calendar,
    ChevronUp,
    Copy,
    Pipette,
    Play,
    Star,
    StarFilled,
    Tag,
    Import,
    Export,
    Table,
    Steps,
    Clock,
    Mouse,
    Bell,
    Clipboard,
    Hand,
    Wheel,
    Move,
    Apps,
    Check,
    Edit,
    Grip,
}

fn path_data(icon: Icon) -> &'static str {
    match icon {
        Icon::Search => r#"<circle cx="10.5" cy="10.5" r="6"/><path d="M15 15l5 5"/>"#,
        Icon::Plus => r#"<path d="M12 5v14M5 12h14"/>"#,
        Icon::Settings => r#"<path d="M4 7h9M17 7h3M4 17h3M11 17h9"/><circle cx="15" cy="7" r="2"/><circle cx="9" cy="17" r="2"/>"#,
        Icon::Keyboard => r#"<rect x="3" y="6" width="18" height="12" rx="2"/><path d="M7 10h.01M10 10h.01M13 10h.01M16 10h.01M8 14h8"/>"#,
        Icon::Trash => r#"<path d="M4 7h16M10 4h4M6 7l1 12a2 2 0 0 0 2 2h6a2 2 0 0 0 2-2l1-12M10 11v6M14 11v6"/>"#,
        Icon::Close => r#"<path d="M6 6l12 12M18 6L6 18"/>"#,
        Icon::Open => r#"<path d="M14 4h6v6M20 4l-9 9M18 14v4a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h4"/>"#,
        Icon::Text => r#"<path d="M5 6h14M12 6v13M9 19h6"/>"#,
        Icon::Folder => r#"<path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>"#,
        Icon::Globe => r#"<circle cx="12" cy="12" r="8.5"/><path d="M3.5 12h17M12 3.5c2.5 2.5 3.5 5.5 3.5 8.5s-1 6-3.5 8.5c-2.5-2.5-3.5-5.5-3.5-8.5s1-6 3.5-8.5z"/>"#,
        Icon::File => r#"<path d="M7 3h7l5 5v11a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z"/><path d="M14 3v5h5"/>"#,
        Icon::Warning => r#"<path d="M12 4l9 16H3z"/><path d="M12 10v4M12 17h.01"/>"#,
        Icon::Info => r#"<circle cx="12" cy="12" r="8.5"/><path d="M12 11v5M12 8h.01"/>"#,
        Icon::Chevron => r#"<path d="M9 6l6 6-6 6"/>"#,
        Icon::ChevronDown => r#"<path d="M6 9l6 6 6-6"/>"#,
        Icon::Bolt => r#"<path d="M13 3L5 13.5h6L10 21l8-10.5h-6z"/>"#,
        Icon::Window => r#"<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M3 8.5h18"/>"#,
        Icon::Mail => r#"<rect x="3" y="5" width="18" height="14" rx="2"/><path d="M3.5 6.5L12 13l8.5-6.5"/>"#,
        Icon::Calendar => r#"<rect x="3.5" y="5" width="17" height="15" rx="2"/><path d="M3.5 10h17M8 3v4M16 3v4"/>"#,
        Icon::ChevronUp => r#"<path d="M6 15l6-6 6 6"/>"#,
        Icon::Copy => r#"<rect x="8" y="8" width="12" height="12" rx="2"/><path d="M16 8V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h2"/>"#,
        Icon::Pipette => r#"<path d="M14 6l4 4M16.5 3.5a2.1 2.1 0 0 1 3 3L17 9l-2-2zM15 8L6 17l-1 3 3-1 9-9"/>"#,
        Icon::Play => r#"<path d="M7 4.5v15l12-7.5z"/>"#,
        Icon::Star => r#"<path d="M12 3.5l2.6 5.3 5.9.9-4.3 4.1 1 5.8L12 16.9l-5.2 2.7 1-5.8-4.3-4.1 5.9-.9z"/>"#,
        Icon::StarFilled => r#"<path fill="black" d="M12 3.5l2.6 5.3 5.9.9-4.3 4.1 1 5.8L12 16.9l-5.2 2.7 1-5.8-4.3-4.1 5.9-.9z"/>"#,
        Icon::Tag => r#"<path d="M3.5 12.5V4.5a1 1 0 0 1 1-1h8l8 8-9 9z"/><circle cx="8" cy="8" r="1.3"/>"#,
        Icon::Import => r#"<path d="M12 3v12M7 10l5 5 5-5M4 17v2a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-2"/>"#,
        Icon::Export => r#"<path d="M12 15V3M7 8l5-5 5 5M4 17v2a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-2"/>"#,
        Icon::Table => r#"<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M3 9h18M3 14.5h18M9 9v11"/>"#,
        Icon::Steps => r#"<path d="M9 6h11M9 12h11M9 18h11"/><circle cx="4.5" cy="6" r="1.2"/><circle cx="4.5" cy="12" r="1.2"/><circle cx="4.5" cy="18" r="1.2"/>"#,
        Icon::Clock => r#"<circle cx="12" cy="12" r="8.5"/><path d="M12 7.5V12l3 2"/>"#,
        Icon::Mouse => r#"<rect x="6.5" y="3" width="11" height="18" rx="5.5"/><path d="M12 3v6"/>"#,
        Icon::Bell => r#"<path d="M6 16V11a6 6 0 0 1 12 0v5l1.5 2h-15zM10 20.5a2 2 0 0 0 4 0"/>"#,
        Icon::Clipboard => r#"<rect x="5" y="4.5" width="14" height="16.5" rx="2"/><path d="M9 4.5V3h6v1.5M9 10h6M9 14h6"/>"#,
        Icon::Hand => r#"<path d="M8 13V6a1.5 1.5 0 0 1 3 0v5M11 11V4.5a1.5 1.5 0 0 1 3 0V11M14 11V6a1.5 1.5 0 0 1 3 0v8a6 6 0 0 1-6 6h-.5a5.5 5.5 0 0 1-4.6-2.5L3.5 13.8a1.5 1.5 0 0 1 2.5-1.6L8 14"/>"#,
        Icon::Wheel => r#"<rect x="6.5" y="3" width="11" height="18" rx="5.5"/><path d="M12 6.5v3M10 14l2 2 2-2"/>"#,
        Icon::Move => r#"<path d="M5 3l5.5 15 2.2-6.3L19 9.5z"/><path d="M13.5 13.5l5 5"/>"#,
        Icon::Apps => r#"<rect x="4" y="4" width="6.5" height="6.5" rx="1.5"/><rect x="13.5" y="4" width="6.5" height="6.5" rx="1.5"/><rect x="4" y="13.5" width="6.5" height="6.5" rx="1.5"/><rect x="13.5" y="13.5" width="6.5" height="6.5" rx="1.5"/>"#,
        Icon::Check => r#"<path d="M5 12.5l4.5 4.5L19 7.5"/>"#,
        Icon::Edit => r#"<path d="M4 20h4L19 9l-4-4L4 16zM13.5 6.5l4 4"/>"#,
        Icon::Grip => r#"<g fill="black" stroke="none"><circle cx="9" cy="6" r="1.6"/><circle cx="15" cy="6" r="1.6"/><circle cx="9" cy="12" r="1.6"/><circle cx="15" cy="12" r="1.6"/><circle cx="9" cy="18" r="1.6"/><circle cx="15" cy="18" r="1.6"/></g>"#,
    }
}

fn document(icon: Icon) -> String {
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">{}</svg>"#,
        path_data(icon)
    )
}

/// An icon of the given size, coloured by `color` (or the text colour).
pub fn icon<'a>(icon: Icon, size: f32, color: Option<fn(&Theme) -> Color>) -> Svg<'a, Theme> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static CACHE: OnceLock<Mutex<HashMap<Icon, svg::Handle>>> = OnceLock::new();
    let handle = CACHE
        .get_or_init(Default::default)
        .lock()
        .map(|mut cache| cache.entry(icon).or_insert_with(|| svg::Handle::from_memory(document(icon).into_bytes())).clone())
        .unwrap_or_else(|_| svg::Handle::from_memory(document(icon).into_bytes()));
    svg(handle).width(Length::Fixed(size)).height(Length::Fixed(size)).style(move |theme: &Theme, _| svg::Style {
        color: Some(match color {
            Some(f) => f(theme),
            None => theme.palette().text,
        }),
    })
}

pub fn accent(theme: &Theme) -> Color {
    super::style::tokens(theme).accent
}

pub fn secondary(theme: &Theme) -> Color {
    super::style::tokens(theme).text2
}

pub fn warning(theme: &Theme) -> Color {
    let t = super::style::tokens(theme);
    if t.dark { t.warning } else { iced::color!(0x9D5D00) }
}

pub fn danger(theme: &Theme) -> Color {
    super::style::tokens(theme).danger
}

pub fn on_accent(theme: &Theme) -> Color {
    super::style::tokens(theme).on_accent
}
