//! Visual design: colour tokens (light/dark, system accent) and widget styles
//! inspired by Windows 11.

use iced::border::Radius;
use iced::theme::Palette;
use iced::widget::{button, checkbox, container, pick_list, scrollable, text_editor, text_input, toggler};
use iced::{Background, Border, Color, Shadow, Theme, color};

pub const RADIUS: f32 = 6.0;
pub const RADIUS_LARGE: f32 = 8.0;

/// Colour tokens derived from the active theme.
#[derive(Debug, Clone, Copy)]
pub struct Tokens {
    pub dark: bool,
    pub surface: Color,
    pub surface_hover: Color,
    pub surface_pressed: Color,
    pub subtle: Color,
    pub subtle_hover: Color,
    pub stroke: Color,
    pub stroke_strong: Color,
    pub text: Color,
    pub text2: Color,
    pub text3: Color,
    pub accent: Color,
    pub accent_hover: Color,
    pub on_accent: Color,
    pub accent_soft: Color,
    pub danger: Color,
    pub danger_soft: Color,
    pub warning: Color,
    pub warning_soft: Color,
    pub keycap: Color,
    pub keycap_edge: Color,
}

fn mix(a: Color, b: Color, t: f32) -> Color {
    Color::from_rgba(a.r + (b.r - a.r) * t, a.g + (b.g - a.g) * t, a.b + (b.b - a.b) * t, a.a + (b.a - a.a) * t)
}

fn luminance(c: Color) -> f32 {
    0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b
}

fn with_alpha(c: Color, a: f32) -> Color {
    Color { a, ..c }
}

/// Adapts the system accent colour for readability on the theme background.
pub fn adapt_accent(accent: Color, dark: bool) -> Color {
    let l = luminance(accent);
    if dark {
        if l < 0.45 { mix(accent, Color::WHITE, 0.45) } else { accent }
    } else if l > 0.45 {
        mix(accent, Color::BLACK, 0.35)
    } else {
        accent
    }
}

/// Default accents (Windows 11 default blue) when the system one is unknown.
pub fn default_accent(dark: bool) -> Color {
    if dark { color!(0x4CC2FF) } else { color!(0x005FB8) }
}

/// Builds the application theme.
pub fn make_theme(dark: bool, accent: Color) -> Theme {
    let palette = if dark {
        Palette {
            background: color!(0x202020),
            text: Color::WHITE,
            primary: accent,
            success: color!(0x6CCB5F),
            warning: color!(0xFCE100),
            danger: color!(0xFF99A4),
        }
    } else {
        Palette {
            background: color!(0xF3F3F3),
            text: color!(0x1B1B1B),
            primary: accent,
            success: color!(0x0F7B0F),
            warning: color!(0x9D5D00),
            danger: color!(0xC42B1C),
        }
    };
    Theme::custom(if dark { "Declic dark" } else { "Declic light" }, palette)
}

pub fn tokens(theme: &Theme) -> Tokens {
    let palette = theme.palette();
    let dark = theme.extended_palette().is_dark;
    let accent = palette.primary;
    let on_accent = if luminance(accent) > 0.5 { Color::BLACK } else { Color::WHITE };
    if dark {
        Tokens {
            dark,
            surface: color!(0x2B2B2B),
            surface_hover: color!(0x323232),
            surface_pressed: color!(0x282828),
            subtle: Color::from_rgba(1.0, 1.0, 1.0, 0.06),
            subtle_hover: Color::from_rgba(1.0, 1.0, 1.0, 0.09),
            stroke: Color::from_rgba(1.0, 1.0, 1.0, 0.08),
            stroke_strong: Color::from_rgba(1.0, 1.0, 1.0, 0.16),
            text: Color::WHITE,
            text2: color!(0xC8C8C8),
            text3: color!(0x9D9D9D),
            accent,
            accent_hover: mix(accent, Color::BLACK, 0.1),
            on_accent,
            accent_soft: with_alpha(accent, 0.16),
            danger: palette.danger,
            danger_soft: color!(0x442726),
            warning: palette.warning,
            warning_soft: color!(0x433519),
            keycap: color!(0x373737),
            keycap_edge: color!(0x1A1A1A),
        }
    } else {
        Tokens {
            dark,
            surface: color!(0xFFFFFF),
            surface_hover: color!(0xF9F9F9),
            surface_pressed: color!(0xF5F5F5),
            subtle: Color::from_rgba(0.0, 0.0, 0.0, 0.035),
            subtle_hover: Color::from_rgba(0.0, 0.0, 0.0, 0.06),
            stroke: Color::from_rgba(0.0, 0.0, 0.0, 0.07),
            stroke_strong: Color::from_rgba(0.0, 0.0, 0.0, 0.16),
            text: color!(0x1B1B1B),
            text2: color!(0x5D5D5D),
            text3: color!(0x8A8A8A),
            accent,
            accent_hover: mix(accent, Color::WHITE, 0.1),
            on_accent,
            accent_soft: with_alpha(accent, 0.10),
            danger: palette.danger,
            danger_soft: color!(0xFDE7E9),
            warning: palette.warning,
            warning_soft: color!(0xFFF4CE),
            keycap: color!(0xFFFFFF),
            keycap_edge: color!(0xD0D0D0),
        }
    }
}

fn border(color: Color, width: f32, radius: f32) -> Border {
    Border { color, width, radius: Radius::from(radius) }
}

// ---------------------------------------------------------------- containers

pub fn card(theme: &Theme) -> container::Style {
    let t = tokens(theme);
    container::Style {
        background: Some(t.surface.into()),
        border: border(t.stroke, 1.0, RADIUS_LARGE),
        text_color: Some(t.text),
        ..Default::default()
    }
}

pub fn panel(theme: &Theme) -> container::Style {
    let t = tokens(theme);
    container::Style {
        background: Some((if t.dark { color!(0x272727) } else { color!(0xFBFBFB) }).into()),
        border: border(t.stroke, 1.0, RADIUS_LARGE),
        text_color: Some(t.text),
        ..Default::default()
    }
}

// Keycaps are drawn as a face on top of a slightly larger "edge" (no shadow:
// the software renderer's partial redraws do not account for shadows).

pub fn keycap(theme: &Theme) -> container::Style {
    let t = tokens(theme);
    container::Style {
        background: Some(t.keycap.into()),
        border: border(t.stroke_strong, 1.0, 4.0),
        text_color: Some(t.text),
        ..Default::default()
    }
}

pub fn keycap_accent(theme: &Theme) -> container::Style {
    let t = tokens(theme);
    container::Style {
        background: Some(t.keycap.into()),
        border: border(t.accent, 1.0, 4.0),
        text_color: Some(t.text),
        ..Default::default()
    }
}

pub fn keycap_edge(theme: &Theme) -> container::Style {
    let t = tokens(theme);
    container::Style {
        background: Some(t.keycap_edge.into()),
        border: border(Color::TRANSPARENT, 0.0, 4.0),
        ..Default::default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Neutral,
    Accent,
    Warning,
    Danger,
}

pub fn badge(tone: Tone) -> impl Fn(&Theme) -> container::Style {
    move |theme| {
        let t = tokens(theme);
        let (bg, fg) = match tone {
            Tone::Neutral => (t.subtle_hover, t.text2),
            Tone::Accent => (t.accent_soft, t.accent),
            Tone::Warning => (t.warning_soft, if t.dark { t.warning } else { color!(0x8A5300) }),
            Tone::Danger => (t.danger_soft, t.danger),
        };
        container::Style { background: Some(bg.into()), text_color: Some(fg), border: border(Color::TRANSPARENT, 0.0, 10.0), ..Default::default() }
    }
}

pub fn infobar(tone: Tone) -> impl Fn(&Theme) -> container::Style {
    move |theme| {
        let t = tokens(theme);
        let (bg, edge) = match tone {
            Tone::Neutral | Tone::Accent => (t.accent_soft, with_alpha(t.accent, 0.4)),
            Tone::Warning => (t.warning_soft, with_alpha(t.warning, 0.4)),
            Tone::Danger => (t.danger_soft, with_alpha(t.danger, 0.4)),
        };
        container::Style { background: Some(bg.into()), text_color: Some(t.text), border: border(edge, 1.0, RADIUS), ..Default::default() }
    }
}

pub fn icon_circle(theme: &Theme) -> container::Style {
    let t = tokens(theme);
    container::Style { background: Some(t.accent_soft.into()), border: border(Color::TRANSPARENT, 0.0, 8.0), ..Default::default() }
}

pub fn capture_box(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        let t = tokens(theme);
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: Some(Background::Color(if active { t.accent_soft } else if hovered { t.surface_hover } else { t.surface })),
            text_color: t.text,
            border: Border {
                color: if active { t.accent } else { t.stroke_strong },
                width: if active { 2.0 } else { 1.0 },
                radius: Radius::from(RADIUS),
            },
            ..Default::default()
        }
    }
}

// ---------------------------------------------------------------- buttons

pub fn button_primary(theme: &Theme, status: button::Status) -> button::Style {
    let t = tokens(theme);
    let bg = match status {
        button::Status::Hovered => t.accent_hover,
        button::Status::Pressed => with_alpha(t.accent, 0.85),
        button::Status::Disabled => t.subtle_hover,
        button::Status::Active => t.accent,
    };
    button::Style {
        background: Some(bg.into()),
        text_color: if status == button::Status::Disabled { t.text3 } else { t.on_accent },
        border: border(Color::TRANSPARENT, 0.0, RADIUS),
        ..Default::default()
    }
}

pub fn button_secondary(theme: &Theme, status: button::Status) -> button::Style {
    let t = tokens(theme);
    let bg = match status {
        button::Status::Hovered => t.surface_hover,
        button::Status::Pressed => t.surface_pressed,
        _ => t.surface,
    };
    button::Style {
        background: Some(bg.into()),
        text_color: if status == button::Status::Disabled { t.text3 } else { t.text },
        border: border(t.stroke_strong, 1.0, RADIUS),
        ..Default::default()
    }
}

pub fn button_ghost(theme: &Theme, status: button::Status) -> button::Style {
    let t = tokens(theme);
    let bg = match status {
        button::Status::Hovered => Some(t.subtle_hover.into()),
        button::Status::Pressed => Some(t.subtle.into()),
        _ => None,
    };
    button::Style {
        background: bg,
        text_color: if status == button::Status::Disabled { t.text3 } else { t.text },
        border: border(Color::TRANSPARENT, 0.0, RADIUS),
        ..Default::default()
    }
}

pub fn button_link(theme: &Theme, status: button::Status) -> button::Style {
    let t = tokens(theme);
    button::Style {
        background: None,
        text_color: if status == button::Status::Hovered { t.accent_hover } else { t.accent },
        border: border(Color::TRANSPARENT, 0.0, RADIUS),
        ..Default::default()
    }
}

pub fn button_danger(theme: &Theme, status: button::Status) -> button::Style {
    let t = tokens(theme);
    let bg = match status {
        button::Status::Hovered | button::Status::Pressed => Some(t.danger_soft.into()),
        _ => None,
    };
    button::Style { background: bg, text_color: t.danger, border: border(Color::TRANSPARENT, 0.0, RADIUS), ..Default::default() }
}

/// Segment of a segmented control / filter chip.
pub fn button_segment(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        let t = tokens(theme);
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let (bg, fg, edge) = if selected {
            (t.accent_soft, t.accent, with_alpha(t.accent, 0.5))
        } else if hovered {
            (t.subtle_hover, t.text, t.stroke)
        } else {
            (t.surface, t.text2, t.stroke_strong)
        };
        button::Style { background: Some(bg.into()), text_color: fg, border: border(edge, 1.0, RADIUS), ..Default::default() }
    }
}

/// A row of the shortcut list.
pub fn button_row(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        let t = tokens(theme);
        let bg = if selected {
            t.accent_soft
        } else {
            match status {
                button::Status::Hovered => t.surface_hover,
                button::Status::Pressed => t.surface_pressed,
                _ => t.surface,
            }
        };
        button::Style {
            background: Some(bg.into()),
            text_color: t.text,
            border: border(if selected { with_alpha(t.accent, 0.5) } else { t.stroke }, 1.0, RADIUS_LARGE),
            ..Default::default()
        }
    }
}

// ---------------------------------------------------------------- inputs

pub fn input(theme: &Theme, status: text_input::Status) -> text_input::Style {
    let t = tokens(theme);
    let focused = matches!(status, text_input::Status::Focused { .. });
    let hovered = matches!(status, text_input::Status::Hovered);
    text_input::Style {
        background: Background::Color(if focused { t.surface } else if hovered { t.surface_hover } else { t.surface }),
        border: Border {
            color: if focused { t.accent } else { t.stroke_strong },
            width: if focused { 1.5 } else { 1.0 },
            radius: Radius::from(RADIUS),
        },
        icon: t.text2,
        placeholder: t.text3,
        value: t.text,
        selection: with_alpha(t.accent, 0.35),
    }
}

pub fn editor(theme: &Theme, status: text_editor::Status) -> text_editor::Style {
    let t = tokens(theme);
    let focused = matches!(status, text_editor::Status::Focused { .. });
    text_editor::Style {
        background: Background::Color(t.surface),
        border: Border {
            color: if focused { t.accent } else { t.stroke_strong },
            width: if focused { 1.5 } else { 1.0 },
            radius: Radius::from(RADIUS),
        },
        placeholder: t.text3,
        value: t.text,
        selection: with_alpha(t.accent, 0.35),
    }
}

pub fn pick(theme: &Theme, status: pick_list::Status) -> pick_list::Style {
    let t = tokens(theme);
    let hovered = matches!(status, pick_list::Status::Hovered | pick_list::Status::Opened { .. });
    pick_list::Style {
        text_color: t.text,
        placeholder_color: t.text3,
        handle_color: t.text2,
        background: Background::Color(if hovered { t.surface_hover } else { t.surface }),
        border: border(t.stroke_strong, 1.0, RADIUS),
    }
}

pub fn menu(theme: &Theme) -> iced::overlay::menu::Style {
    let t = tokens(theme);
    iced::overlay::menu::Style {
        background: Background::Color(if t.dark { color!(0x2C2C2C) } else { Color::WHITE }),
        border: border(t.stroke_strong, 1.0, RADIUS),
        text_color: t.text,
        selected_text_color: t.text,
        selected_background: Background::Color(t.accent_soft),
        shadow: Shadow::default(),
    }
}

pub fn check(theme: &Theme, status: checkbox::Status) -> checkbox::Style {
    let t = tokens(theme);
    let checked = match status {
        checkbox::Status::Active { is_checked } | checkbox::Status::Hovered { is_checked } | checkbox::Status::Disabled { is_checked } => is_checked,
    };
    checkbox::Style {
        background: Background::Color(if checked { t.accent } else { t.surface }),
        icon_color: t.on_accent,
        border: border(if checked { t.accent } else { t.text3 }, 1.0, 4.0),
        text_color: Some(t.text),
    }
}

pub fn switch(theme: &Theme, status: toggler::Status) -> toggler::Style {
    let t = tokens(theme);
    let on = match status {
        toggler::Status::Active { is_toggled } | toggler::Status::Hovered { is_toggled } | toggler::Status::Disabled { is_toggled } => is_toggled,
    };
    toggler::Style {
        background: Background::Color(if on { t.accent } else { Color::TRANSPARENT }),
        background_border_width: if on { 0.0 } else { 1.0 },
        background_border_color: t.text2,
        foreground: Background::Color(if on { t.on_accent } else { t.text2 }),
        foreground_border_width: 0.0,
        foreground_border_color: Color::TRANSPARENT,
        text_color: Some(t.text),
        border_radius: None,
        padding_ratio: 0.2,
    }
}

pub fn scroll(theme: &Theme, status: scrollable::Status) -> scrollable::Style {
    let t = tokens(theme);
    let hovered = matches!(status, scrollable::Status::Hovered { .. } | scrollable::Status::Dragged { .. });
    let rail = scrollable::Rail {
        background: None,
        border: border(Color::TRANSPARENT, 0.0, 3.0),
        scroller: scrollable::Scroller {
            background: Background::Color(if hovered { t.text3 } else { with_alpha(t.text3, 0.5) }),
            border: border(Color::TRANSPARENT, 0.0, 3.0),
        },
    };
    scrollable::Style {
        container: container::Style::default(),
        vertical_rail: rail,
        horizontal_rail: rail,
        gap: None,
        auto_scroll: scrollable::AutoScroll {
            background: Background::Color(t.surface),
            border: border(t.stroke, 1.0, 8.0),
            shadow: Shadow::default(),
            icon: t.text2,
        },
    }
}

/// Theme-aware text colours for `text(...).style(...)`.
pub fn text_secondary(theme: &Theme) -> iced::widget::text::Style {
    iced::widget::text::Style { color: Some(tokens(theme).text2) }
}

pub fn text_tertiary(theme: &Theme) -> iced::widget::text::Style {
    iced::widget::text::Style { color: Some(tokens(theme).text3) }
}

pub fn text_accent(theme: &Theme) -> iced::widget::text::Style {
    iced::widget::text::Style { color: Some(tokens(theme).accent) }
}

pub fn text_danger(theme: &Theme) -> iced::widget::text::Style {
    iced::widget::text::Style { color: Some(tokens(theme).danger) }
}

/// Background of side panels.
pub fn panel_background(theme: &Theme) -> Color {
    if tokens(theme).dark { color!(0x272727) } else { color!(0xFBFBFB) }
}

/// A macro step card.
pub fn step_card(selected: bool, enabled: bool) -> impl Fn(&Theme) -> container::Style {
    move |theme| {
        let t = tokens(theme);
        container::Style {
            background: Some((if enabled { t.surface } else { t.surface_pressed }).into()),
            border: border(if selected { with_alpha(t.accent, 0.7) } else { t.stroke }, if selected { 1.5 } else { 1.0 }, RADIUS_LARGE),
            text_color: Some(if enabled { t.text } else { t.text2 }),
            ..Default::default()
        }
    }
}

/// An item of the side panel (groups, filters).
pub fn button_side(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        let t = tokens(theme);
        let bg = if selected {
            Some(t.accent_soft.into())
        } else {
            match status {
                button::Status::Hovered => Some(t.subtle_hover.into()),
                button::Status::Pressed => Some(t.subtle.into()),
                _ => None,
            }
        };
        button::Style { background: bg, text_color: if selected { t.accent } else { t.text }, border: border(Color::TRANSPARENT, 0.0, RADIUS), ..Default::default() }
    }
}
