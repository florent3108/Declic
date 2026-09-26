//! Widgets that adapt their content to the available width.
//!
//! - [`line`]: a single line of text shortened with an ellipsis ("…").
//! - [`priority_row`]: a row whose less important elements give way when
//!   space runs out, so that the essential ones are never cut.

use iced::advanced::text::paragraph::Plain;
use iced::advanced::text;
use iced::advanced::widget::{Operation, Tree, tree};
use iced::advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer};
use iced::{Element, Event, Font, Length, Pixels, Point, Rectangle, Renderer, Size, Theme, Vector, alignment};

type Paragraph = <Renderer as text::Renderer>::Paragraph;
type StyleFn<'a> = Box<dyn Fn(&Theme) -> iced::widget::text::Style + 'a>;

/// A single line of text that takes the available width and, when it does
/// not fit, is shortened with an ellipsis instead of being cut in the middle
/// of a word. Right-to-left text keeps its beginning and stays aligned to the
/// right.
pub fn line<'a>(content: impl Into<String>, size: f32) -> Line<'a> {
    Line { content: content.into(), size, font: None, style: Box::new(|_| iced::widget::text::Style::default()) }
}

pub struct Line<'a> {
    content: String,
    size: f32,
    font: Option<Font>,
    style: StyleFn<'a>,
}

impl<'a> Line<'a> {
    pub fn font(mut self, font: Font) -> Self {
        self.font = Some(font);
        self
    }

    pub fn style(mut self, style: impl Fn(&Theme) -> iced::widget::text::Style + 'a) -> Self {
        self.style = Box::new(style);
        self
    }

    fn spec<'b>(&self, content: &'b str, width: f32, renderer: &Renderer) -> text::Text<&'b str, Font> {
        text::Text {
            content,
            bounds: Size::new(width, f32::INFINITY),
            size: Pixels(self.size),
            line_height: text::LineHeight::default(),
            font: self.font.unwrap_or_else(|| text::Renderer::default_font(renderer)),
            align_x: text::Alignment::Default,
            align_y: alignment::Vertical::Top,
            shaping: text::Shaping::Advanced,
            wrapping: text::Wrapping::None,
        }
    }
}

impl<'a, M: 'a> From<Line<'a>> for Element<'a, M> {
    fn from(line: Line<'a>) -> Self {
        Element::new(line)
    }
}

/// Paragraph of a [`Line`] and the text shown for the last width.
#[derive(Default)]
struct LineState {
    paragraph: Plain<Paragraph>,
    /// (content, width bits, text shown).
    cache: Option<(String, u32, String)>,
}

/// The longest beginning of `content` that fits in `max` once followed by an
/// ellipsis (`measure` gives the width of a candidate).
fn shorten(content: &str, max: f32, mut measure: impl FnMut(&str) -> f32) -> String {
    let ends: Vec<usize> = content.char_indices().map(|(i, _)| i).skip(1).chain([content.len()]).collect();
    let candidate = |kept: usize| {
        let end = if kept == 0 { 0 } else { ends[kept - 1] };
        format!("{}…", content[..end].trim_end())
    };
    let (mut low, mut high) = (0usize, ends.len());
    while low < high {
        let mid = (low + high).div_ceil(2);
        if measure(&candidate(mid)) <= max {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    candidate(low)
}

impl<M> Widget<M, Theme, Renderer> for Line<'_> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<LineState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(LineState::default())
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Shrink)
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) -> layout::Node {
        let state = tree.state.downcast_mut::<LineState>();
        let max = limits.max().width;
        let key = if max.is_finite() { max.to_bits() } else { u32::MAX };
        let cached = state
            .cache
            .as_ref()
            .filter(|(content, width, _)| *content == self.content && *width == key)
            .map(|(_, _, shown)| shown.clone());
        let shown = match cached {
            Some(shown) => shown,
            None => {
                let mut measure = |s: &str| {
                    let _ = state.paragraph.update(self.spec(s, f32::INFINITY, renderer));
                    state.paragraph.min_bounds().width
                };
                let shown =
                    if !max.is_finite() || measure(&self.content) <= max { self.content.clone() } else { shorten(&self.content, max, measure) };
                state.cache = Some((self.content.clone(), key, shown.clone()));
                shown
            }
        };
        // Laid out in the full width so that alignment follows the writing
        // direction (right-to-left text is aligned to the right).
        let _ = state.paragraph.update(self.spec(&shown, max, renderer));
        let bounds = state.paragraph.min_bounds();
        layout::Node::new(Size::new(if max.is_finite() { max } else { bounds.width }, bounds.height))
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<LineState>();
        iced::advanced::widget::text::draw(renderer, style, layout.bounds(), state.paragraph.raw(), (self.style)(theme), viewport);
    }

    fn operate(&mut self, tree: &mut Tree, layout: Layout<'_>, _renderer: &Renderer, operation: &mut dyn Operation) {
        let state = tree.state.downcast_ref::<LineState>();
        let shown = state.cache.as_ref().map(|(_, _, s)| s.as_str()).unwrap_or(&self.content);
        operation.text(None, layout.bounds(), shown);
    }
}

/// Keeps a group of elements on one line, giving way progressively when
/// space runs out.
///
/// Every element keeps its natural width, except `main`, which takes the
/// rest (at least `main_min` when possible). `levels` lists which elements
/// are shown, from the richest variant to the most compact one: the first
/// level that leaves `main` its minimum width is used (the last one when
/// none does).
pub fn priority_row<'a, M: 'a>(
    children: Vec<Element<'a, M>>,
    main: usize,
    main_min: f32,
    levels: Vec<Vec<bool>>,
    spacing: f32,
) -> Element<'a, M> {
    let visible = levels.first().cloned().unwrap_or_else(|| vec![true; children.len()]);
    Element::new(PriorityRow { children, main, main_min, levels, spacing, visible })
}

struct PriorityRow<'a, M> {
    children: Vec<Element<'a, M>>,
    main: usize,
    main_min: f32,
    levels: Vec<Vec<bool>>,
    spacing: f32,
    /// Elements shown by the last layout.
    visible: Vec<bool>,
}

/// Index of the first level whose shown elements leave `main_min` for the
/// main element (the last level when none does).
fn choose_level(levels: &[Vec<bool>], natural: &[f32], main: usize, main_min: f32, spacing: f32, width: f32) -> usize {
    levels
        .iter()
        .position(|visible| used_width(visible, natural, main, spacing) + main_min <= width)
        .unwrap_or(levels.len().saturating_sub(1))
}

/// Width taken by the shown elements other than `main`, spacing included.
fn used_width(visible: &[bool], natural: &[f32], main: usize, spacing: f32) -> f32 {
    let shown = visible.iter().filter(|v| **v).count();
    let widths: f32 = natural.iter().zip(visible).enumerate().filter(|(i, (_, v))| **v && *i != main).map(|(_, (w, _))| *w).sum();
    widths + spacing * shown.saturating_sub(1) as f32
}

impl<M> Widget<M, Theme, Renderer> for PriorityRow<'_, M> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Shrink)
    }

    fn children(&self) -> Vec<Tree> {
        self.children.iter().map(Tree::new).collect()
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&self.children);
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) -> layout::Node {
        let max = limits.max();
        let loose = layout::Limits::new(Size::ZERO, Size::new(f32::INFINITY, max.height));
        let main = self.main;
        let natural: Vec<f32> = self
            .children
            .iter_mut()
            .zip(&mut tree.children)
            .enumerate()
            .map(|(i, (child, state))| if i == main { 0.0 } else { child.as_widget_mut().layout(state, renderer, &loose).size().width })
            .collect();
        let level = choose_level(&self.levels, &natural, main, self.main_min, self.spacing, max.width);
        self.visible = self.levels.get(level).cloned().unwrap_or_else(|| vec![true; self.children.len()]);
        let main_width = (max.width - used_width(&self.visible, &natural, main, self.spacing)).max(0.0);
        let mut nodes = Vec::with_capacity(self.children.len());
        let mut height: f32 = 0.0;
        for (i, (child, state)) in self.children.iter_mut().zip(&mut tree.children).enumerate() {
            if !self.visible[i] {
                nodes.push(layout::Node::new(Size::ZERO));
                continue;
            }
            let width = if i == main { main_width } else { natural[i] };
            let node = child.as_widget_mut().layout(state, renderer, &layout::Limits::new(Size::ZERO, Size::new(width, max.height)));
            height = height.max(node.size().height);
            nodes.push(node);
        }
        // Left to right, centred vertically.
        let mut x = 0.0;
        for (i, node) in nodes.iter_mut().enumerate() {
            if !self.visible[i] {
                continue;
            }
            let size = node.size();
            node.move_to_mut(Point::new(x, (height - size.height) / 2.0));
            x += size.width + self.spacing;
        }
        layout::Node::with_children(Size::new(max.width, height), nodes)
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
        for (i, ((child, state), layout)) in self.children.iter().zip(&tree.children).zip(layout.children()).enumerate() {
            if self.visible[i] {
                child.as_widget().draw(state, renderer, theme, style, layout, cursor, viewport);
            }
        }
    }

    fn operate(&mut self, tree: &mut Tree, layout: Layout<'_>, renderer: &Renderer, operation: &mut dyn Operation) {
        operation.container(None, layout.bounds());
        let visible = &self.visible;
        let children = &mut self.children;
        operation.traverse(&mut |operation| {
            for (i, ((child, state), layout)) in children.iter_mut().zip(&mut tree.children).zip(layout.children()).enumerate() {
                if visible[i] {
                    child.as_widget_mut().operate(state, layout, renderer, operation);
                }
            }
        });
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
        for (i, ((child, state), layout)) in self.children.iter_mut().zip(&mut tree.children).zip(layout.children()).enumerate() {
            if self.visible[i] {
                child.as_widget_mut().update(state, event, layout, cursor, renderer, clipboard, shell, viewport);
            }
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.children
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
            .enumerate()
            .filter(|(i, _)| self.visible[*i])
            .map(|(_, ((child, state), layout))| child.as_widget().mouse_interaction(state, layout, cursor, viewport, renderer))
            .max()
            .unwrap_or_default()
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, M, Theme, Renderer>> {
        // Only the shown elements: hidden ones have an empty layout.
        let visible = &self.visible;
        let overlays: Vec<_> = self
            .children
            .iter_mut()
            .zip(&mut tree.children)
            .zip(layout.children())
            .enumerate()
            .filter(|(i, _)| visible[*i])
            .filter_map(|(_, ((child, state), layout))| child.as_widget_mut().overlay(state, layout, renderer, viewport, translation))
            .collect();
        (!overlays.is_empty()).then(|| overlay::Group::with_children(overlays).overlay())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortening_keeps_the_longest_beginning_that_fits() {
        // 10 px per character.
        let measure = |s: &str| s.chars().count() as f32 * 10.0;
        assert_eq!(shorten("Compte rendu du jour", 100.0, measure), "Compte re…");
        // Spaces before the ellipsis are dropped.
        assert_eq!(shorten("Compte rendu du jour", 80.0, measure), "Compte…");
        assert_eq!(shorten("abc", 5.0, measure), "…");
        assert_eq!(shorten("été à l'école", 60.0, measure), "été à…");
    }

    #[test]
    fn levels_give_way_in_order() {
        // checkbox, main, badges (full), badges (compact), uses, keycaps.
        let natural = [20.0, 0.0, 150.0, 40.0, 30.0, 100.0];
        let levels = vec![
            vec![true, true, true, false, true, true],
            vec![true, true, false, true, true, true],
            vec![true, true, false, true, false, true],
            vec![true, true, false, false, false, true],
        ];
        let pick = |width: f32| choose_level(&levels, &natural, 1, 120.0, 10.0, width);
        // Everything: 20 + 150 + 30 + 100 + 4 × 10 = 340, plus 120 for the name.
        assert_eq!(pick(500.0), 0);
        assert_eq!(pick(460.0), 0);
        assert_eq!(pick(459.0), 1);
        // Compact badges: 20 + 40 + 30 + 100 + 40 = 230 (+120 = 350).
        assert_eq!(pick(350.0), 1);
        // Without the usage count: 20 + 40 + 100 + 30 = 190 (+120 = 310).
        assert_eq!(pick(320.0), 2);
        // Without badges: 20 + 100 + 20 = 140 (+120 = 260).
        assert_eq!(pick(270.0), 3);
        // Too narrow for any level: the most compact one.
        assert_eq!(pick(100.0), 3);
        assert_eq!(used_width(&levels[3], &natural, 1, 10.0), 140.0);
    }
}
