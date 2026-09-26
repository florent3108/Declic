//! Small reusable view helpers.

use super::Message;
use super::icons::{self, Icon};
use super::style::{self, Tone};
use iced::widget::{Row, button, column, container, row, text, tooltip};
use iced::{Alignment, Element, Font, Length, Padding, Theme};
use std::fmt;

/// Family used when the language does not name one, or names a font that is
/// not installed.
const DEFAULT_FAMILY: &str = "Segoe UI";

static FAMILY: std::sync::OnceLock<&'static str> = std::sync::OnceLock::new();
static RTL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Chooses the interface font for this process (`language.font` of the
/// translation, e.g. "Yu Gothic UI" for Japanese so that Chinese characters
/// get Japanese shapes). Characters missing from it are still drawn with
/// other Windows fonts. Only the first call has an effect.
pub fn set_font_family(name: &str) {
    let name = name.trim();
    let installed = !name.is_empty() && font_installed(name);
    let family: &'static str = if installed { Box::leak(name.to_string().into_boxed_str()) } else { DEFAULT_FAMILY };
    let _ = FAMILY.set(family);
}

fn font_installed(name: &str) -> bool {
    use iced::advanced::graphics::text::font_system;
    let Ok(mut system) = font_system().write() else { return false };
    system.raw().db().faces().any(|face| face.families.iter().any(|(family, _)| family.eq_ignore_ascii_case(name)))
}

/// Regular interface font.
pub fn font() -> Font {
    Font::with_name(FAMILY.get().copied().unwrap_or(DEFAULT_FAMILY))
}

/// Semibold interface font (titles, names).
pub fn font_semibold() -> Font {
    Font { weight: iced::font::Weight::Semibold, ..font() }
}

/// Sets whether the interface language is written right to left.
pub fn set_rtl(rtl: bool) {
    RTL.store(rtl, std::sync::atomic::Ordering::Relaxed);
}

pub fn is_rtl() -> bool {
    RTL.load(std::sync::atomic::Ordering::Relaxed)
}

/// Lays the whole interface out from right to left for Arabic and Hebrew:
/// every element is placed at the mirrored position within its parent (text
/// itself is shaped and aligned by the text engine, not mirrored).
pub fn directional<'a, M: 'a>(content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    if is_rtl() { Element::new(Mirror { content: content.into() }) } else { content.into() }
}

/// Keeps a part of a mirrored interface left to right (key combinations
/// such as "Ctrl + Alt + M" read left to right in every language): mirroring
/// it once more inside the mirrored interface restores its order.
pub fn keep_ltr<'a, M: 'a>(content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    directional(content)
}

struct Mirror<'a, M> {
    content: Element<'a, M>,
}

mod mirror_impl {
    use super::Mirror;
    use iced::advanced::widget::{Operation, Tree, tree};
    use iced::advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer};
    use iced::{Event, Point, Rectangle, Renderer, Size, Theme, Vector};

    /// Places each child at the horizontally mirrored position in its parent.
    fn mirrored(node: &layout::Node) -> layout::Node {
        let width = node.size().width;
        let children = node
            .children()
            .iter()
            .map(|child| {
                let b = child.bounds();
                mirrored(child).move_to(Point::new(width - b.x - b.width, b.y))
            })
            .collect();
        layout::Node::with_children(node.size(), children).move_to(node.bounds().position())
    }

    impl<M> Widget<M, Theme, Renderer> for Mirror<'_, M> {
        fn size(&self) -> Size<iced::Length> {
            self.content.as_widget().size()
        }

        fn size_hint(&self) -> Size<iced::Length> {
            self.content.as_widget().size_hint()
        }

        fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) -> layout::Node {
            mirrored(&self.content.as_widget_mut().layout(tree, renderer, limits))
        }

        fn draw(
            &self,
            tree: &Tree,
            renderer: &mut Renderer,
            theme: &Theme,
            style: &renderer::Style,
            layout: Layout<'_>,
            cursor: mouse::Cursor,
            viewport: &Rectangle,
        ) {
            self.content.as_widget().draw(tree, renderer, theme, style, layout, cursor, viewport);
        }

        fn tag(&self) -> tree::Tag {
            self.content.as_widget().tag()
        }

        fn state(&self) -> tree::State {
            self.content.as_widget().state()
        }

        fn children(&self) -> Vec<Tree> {
            self.content.as_widget().children()
        }

        fn diff(&self, tree: &mut Tree) {
            self.content.as_widget().diff(tree);
        }

        fn operate(&mut self, tree: &mut Tree, layout: Layout<'_>, renderer: &Renderer, operation: &mut dyn Operation) {
            self.content.as_widget_mut().operate(tree, layout, renderer, operation);
        }

        fn update(
            &mut self,
            tree: &mut Tree,
            event: &Event,
            layout: Layout<'_>,
            cursor: mouse::Cursor,
            renderer: &Renderer,
            clipboard: &mut dyn Clipboard,
            shell: &mut Shell<'_, M>,
            viewport: &Rectangle,
        ) {
            self.content.as_widget_mut().update(tree, event, layout, cursor, renderer, clipboard, shell, viewport);
        }

        fn mouse_interaction(
            &self,
            tree: &Tree,
            layout: Layout<'_>,
            cursor: mouse::Cursor,
            viewport: &Rectangle,
            renderer: &Renderer,
        ) -> mouse::Interaction {
            self.content.as_widget().mouse_interaction(tree, layout, cursor, viewport, renderer)
        }

        fn overlay<'b>(
            &'b mut self,
            tree: &'b mut Tree,
            layout: Layout<'b>,
            renderer: &Renderer,
            viewport: &Rectangle,
            translation: Vector,
        ) -> Option<overlay::Element<'b, M, Theme, Renderer>> {
            self.content.as_widget_mut().overlay(tree, layout, renderer, viewport, translation)
        }
    }
}

/// A value shown in a pick list with a translated label.
#[derive(Debug, Clone, PartialEq)]
pub struct Choice<T> {
    pub value: T,
    pub label: String,
}

impl<T> Choice<T> {
    pub fn new(value: T, label: impl Into<String>) -> Self {
        Choice { value, label: label.into() }
    }
}

impl<T> fmt::Display for Choice<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.label)
    }
}

/// Keycaps of a combination.
pub fn keycaps<'a>(labels: Vec<String>, size: f32, accent: bool) -> Element<'a, Message> {
    let caps = labels.into_iter().map(|label| {
        let face = container(text(label).size(size).font(font_semibold()))
            .padding(Padding::from([2.0, 7.0]))
            .style(if accent { style::keycap_accent } else { style::keycap });
        container(face).padding(Padding::ZERO.bottom(2.0)).style(style::keycap_edge).into()
    });
    keep_ltr(Row::with_children(caps).spacing(5).align_y(Alignment::Center))
}

pub fn badge<'a>(label: String, tone: Tone) -> Element<'a, Message> {
    container(text(label).size(11.5).font(font_semibold()))
        .padding(Padding::from([2.0, 8.0]))
        .style(style::badge(tone))
        .into()
}

pub fn section_title<'a>(label: &'a str) -> Element<'a, Message> {
    text(label).size(13).font(font_semibold()).style(style::text_secondary).into()
}

pub fn field_label<'a>(label: &'a str) -> Element<'a, Message> {
    text(label).size(13).into()
}

pub fn hint<'a>(label: impl text::IntoFragment<'a>) -> Element<'a, Message> {
    text(label).size(12).style(style::text_tertiary).into()
}

pub fn error<'a>(label: impl text::IntoFragment<'a>) -> Element<'a, Message> {
    row![icons::icon(Icon::Warning, 14.0, Some(icons::danger)), text(label).size(12).style(style::text_danger)]
        .spacing(6)
        .align_y(Alignment::Center)
        .into()
}

pub fn infobar<'a>(
    icon: Icon,
    tone: Tone,
    body: impl Into<Element<'a, Message>>,
    action: Option<Element<'a, Message>>,
) -> Element<'a, Message> {
    let color: fn(&Theme) -> iced::Color = match tone {
        Tone::Danger => icons::danger,
        Tone::Warning => icons::warning,
        _ => icons::accent,
    };
    let mut content = row![icons::icon(icon, 16.0, Some(color)), container(body).width(Length::Fill)]
        .spacing(10)
        .align_y(Alignment::Center);
    if let Some(action) = action {
        content = content.push(action);
    }
    container(content).padding(Padding::from([8.0, 12.0])).style(style::infobar(tone)).width(Length::Fill).into()
}

pub fn icon_button<'a>(icon: Icon, tip: &'a str, message: Option<Message>) -> Element<'a, Message> {
    let b = button(icons::icon(icon, 18.0, None))
        .padding(8)
        .style(style::button_ghost)
        .on_press_maybe(message);
    tooltip(b, container(text(tip).size(12)).padding(6).style(style::card), tooltip::Position::Bottom)
        .gap(4)
        .into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    Primary,
    Secondary,
    Ghost,
}

pub fn labeled_button<'a>(icon: Icon, label: &'a str, message: Option<Message>, kind: ButtonKind) -> button::Button<'a, Message> {
    type ColorFn = fn(&Theme) -> iced::Color;
    type StyleFn = fn(&Theme, button::Status) -> button::Style;
    let (color, style): (Option<ColorFn>, StyleFn) = match kind {
        ButtonKind::Primary => (Some(icons::on_accent), style::button_primary),
        ButtonKind::Secondary => (None, style::button_secondary),
        ButtonKind::Ghost => (None, style::button_ghost),
    };
    button(row![icons::icon(icon, 16.0, color), text(label).size(13)].spacing(8).align_y(Alignment::Center))
        .padding(Padding::from([7.0, 12.0]))
        .style(style)
        .on_press_maybe(message)
}

pub fn text_button<'a>(
    label: &'a str,
    message: Option<Message>,
    style: fn(&Theme, button::Status) -> button::Style,
) -> button::Button<'a, Message> {
    button(text(label).size(13)).padding(Padding::from([6.0, 12.0])).style(style).on_press_maybe(message)
}

/// A titled group of fields.
pub fn field<'a>(label: &'a str, content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    column![field_label(label), content.into()].spacing(6).into()
}

/// Wraps the content of a scrollable area so that all of its text is clipped
/// at the edge of the area.
///
/// With the software renderer, text drawn by some widgets (drop-down lists,
/// check marks) is only clipped when the viewport they receive is larger than
/// the clipping layer; inside a scrollable both are equal, so a drop-down
/// half-hidden at the bottom of a panel had its label drawn outside the panel
/// (and left traces when scrolling). Handing the content a viewport one pixel
/// larger on every side makes the renderer clip that text like the rest.
pub fn clip_text<'a, M: 'a>(content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    Element::new(ClipText { content: content.into() })
}

struct ClipText<'a, M> {
    content: Element<'a, M>,
}

mod clip_text_impl {
    use super::ClipText;
    use iced::advanced::widget::{Operation, Tree, tree};
    use iced::advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer};
    use iced::{Event, Rectangle, Renderer, Size, Theme, Vector};

    impl<M> Widget<M, Theme, Renderer> for ClipText<'_, M> {
        fn size(&self) -> Size<iced::Length> {
            self.content.as_widget().size()
        }

        fn size_hint(&self) -> Size<iced::Length> {
            self.content.as_widget().size_hint()
        }

        fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) -> layout::Node {
            self.content.as_widget_mut().layout(tree, renderer, limits)
        }

        fn draw(
            &self,
            tree: &Tree,
            renderer: &mut Renderer,
            theme: &Theme,
            style: &renderer::Style,
            layout: Layout<'_>,
            cursor: mouse::Cursor,
            viewport: &Rectangle,
        ) {
            let enlarged = Rectangle {
                x: viewport.x - 1.0,
                y: viewport.y - 1.0,
                width: viewport.width + 2.0,
                height: viewport.height + 2.0,
            };
            self.content.as_widget().draw(tree, renderer, theme, style, layout, cursor, &enlarged);
        }

        fn tag(&self) -> tree::Tag {
            self.content.as_widget().tag()
        }

        fn state(&self) -> tree::State {
            self.content.as_widget().state()
        }

        fn children(&self) -> Vec<Tree> {
            self.content.as_widget().children()
        }

        fn diff(&self, tree: &mut Tree) {
            self.content.as_widget().diff(tree);
        }

        fn operate(&mut self, tree: &mut Tree, layout: Layout<'_>, renderer: &Renderer, operation: &mut dyn Operation) {
            self.content.as_widget_mut().operate(tree, layout, renderer, operation);
        }

        fn update(
            &mut self,
            tree: &mut Tree,
            event: &Event,
            layout: Layout<'_>,
            cursor: mouse::Cursor,
            renderer: &Renderer,
            clipboard: &mut dyn Clipboard,
            shell: &mut Shell<'_, M>,
            viewport: &Rectangle,
        ) {
            self.content.as_widget_mut().update(tree, event, layout, cursor, renderer, clipboard, shell, viewport);
        }

        fn mouse_interaction(
            &self,
            tree: &Tree,
            layout: Layout<'_>,
            cursor: mouse::Cursor,
            viewport: &Rectangle,
            renderer: &Renderer,
        ) -> mouse::Interaction {
            self.content.as_widget().mouse_interaction(tree, layout, cursor, viewport, renderer)
        }

        fn overlay<'b>(
            &'b mut self,
            tree: &'b mut Tree,
            layout: Layout<'b>,
            renderer: &Renderer,
            viewport: &Rectangle,
            translation: Vector,
        ) -> Option<overlay::Element<'b, M, Theme, Renderer>> {
            self.content.as_widget_mut().overlay(tree, layout, renderer, viewport, translation)
        }
    }
}
