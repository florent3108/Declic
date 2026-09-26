//! A vertical list whose items can be reordered by dragging a handle
//! (macro steps, F-MAC-02).
//!
//! Only the handle area of each item (see [`Handle`]) starts a drag, so the
//! fields inside the items keep working normally. A drag starts once the
//! pointer has moved a few pixels; the dragged item is then dimmed in place, a
//! lifted copy follows the pointer and an accent line shows where it will be
//! inserted. Releasing the button inside the scrolling area drops the item at
//! the line; releasing it outside (or pressing Esc, handled by the
//! application) cancels. Near the top or bottom of the scrolling area the list
//! asks for auto-scrolling. The position logic lives in
//! [`declic_core::reorder`].

use super::style;
use declic_core::reorder;
use iced::advanced::widget::{Operation, Tree, tree};
use iced::advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer};
use iced::{Background, Border, Color, Element, Event, Length, Point, Rectangle, Renderer, Size, Theme, Vector};
use std::time::{Duration, Instant};

/// Distance the pointer must travel before a press on a handle becomes a drag.
const THRESHOLD: f32 = 4.0;
/// Height of the auto-scroll zones along the top and bottom edges.
const SCROLL_ZONE: f32 = 48.0;
/// Auto-scroll speed at the very edge, in pixels per 16 ms.
const SCROLL_SPEED: f32 = 14.0;
const TICK: Duration = Duration::from_millis(16);

/// Where the drag handle sits inside each item, measured from the item's
/// leading edge (left, or right for right-to-left languages) and top.
#[derive(Debug, Clone, Copy)]
pub struct Handle {
    pub leading: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

type OnStart<'a, M> = Box<dyn Fn(usize) -> M + 'a>;
type OnDrop<'a, M> = Box<dyn Fn(usize, usize) -> M + 'a>;
type OnScroll<'a, M> = Box<dyn Fn(f32) -> M + 'a>;

pub struct DragList<'a, M> {
    items: Vec<Element<'a, M>>,
    spacing: f32,
    handle: Handle,
    /// Item being dragged according to the application (`None` once the
    /// application cancelled the drag, e.g. with Esc).
    dragging: Option<usize>,
    on_start: OnStart<'a, M>,
    on_drop: OnDrop<'a, M>,
    on_cancel: M,
    on_scroll: OnScroll<'a, M>,
}

/// A reorderable list. `on_start(i)` is published when item `i` starts being
/// dragged, `on_drop(from, to)` when it is dropped at a new place,
/// `on_cancel` when the drag ends without change and `on_scroll(dy)` to ask the
/// surrounding scrollable to scroll by `dy` pixels.
#[allow(clippy::too_many_arguments)]
pub fn drag_list<'a, M: Clone + 'a>(
    items: Vec<Element<'a, M>>,
    spacing: f32,
    handle: Handle,
    dragging: Option<usize>,
    on_start: impl Fn(usize) -> M + 'a,
    on_drop: impl Fn(usize, usize) -> M + 'a,
    on_cancel: M,
    on_scroll: impl Fn(f32) -> M + 'a,
) -> Element<'a, M> {
    Element::new(DragList {
        items,
        spacing,
        handle,
        dragging,
        on_start: Box::new(on_start),
        on_drop: Box::new(on_drop),
        on_cancel,
        on_scroll: Box::new(on_scroll),
    })
}

#[derive(Default)]
struct DragState {
    /// Press on the handle of an item, not yet a drag: (item, position).
    pending: Option<(usize, Point)>,
    active: Option<Active>,
}

struct Active {
    from: usize,
    /// Pointer position minus the top of the dragged item when the drag began.
    grab: f32,
    /// Pointer height from the top of the visible part of the scrolling area:
    /// it stays valid while the list scrolls under a still pointer.
    rel_y: f32,
    /// The pointer is outside the scrolling area: releasing it there cancels,
    /// so no insertion line is shown.
    outside: bool,
    /// The application acknowledged the drag (so a later `dragging == None`
    /// means it was cancelled).
    confirmed: bool,
    last_tick: Option<Instant>,
}

impl Active {
    /// Pointer height in the list's coordinates.
    fn pointer_y(&self, viewport: &Rectangle) -> f32 {
        viewport.y + self.rel_y
    }

    /// Follows the pointer, including above or below the scrolling area.
    fn track(&mut self, cursor: mouse::Cursor, viewport: &Rectangle) {
        if let Some(position) = cursor.land().position() {
            self.rel_y = position.y - viewport.y;
        }
        self.outside = cursor.position().is_none();
    }
}

impl<M> DragList<'_, M> {
    fn handle_zone(&self, item: Rectangle) -> Rectangle {
        let h = self.handle;
        let x = if super::widgets::is_rtl() { item.x + item.width - h.leading - h.width } else { item.x + h.leading };
        Rectangle { x, y: item.y + h.top, width: h.width, height: h.height }
    }
}

/// Top and bottom of every item.
fn spans(layout: Layout<'_>) -> Vec<(f32, f32)> {
    layout.children().map(|l| (l.bounds().y, l.bounds().y + l.bounds().height)).collect()
}

impl<M: Clone> DragList<'_, M> {
    /// Publishes an auto-scroll request when the pointer is near an edge of
    /// the visible area, and asks to be called again shortly.
    fn auto_scroll(&self, state: &mut DragState, viewport: &Rectangle, shell: &mut Shell<'_, M>) {
        let Some(active) = state.active.as_mut() else { return };
        let speed = reorder::auto_scroll(active.rel_y, 0.0, viewport.height, SCROLL_ZONE, SCROLL_SPEED);
        let now = Instant::now();
        if speed == 0.0 {
            active.last_tick = None;
            return;
        }
        let elapsed = active.last_tick.map(|t| now.duration_since(t)).unwrap_or(TICK);
        if elapsed < TICK / 2 {
            // Already scrolled for this frame.
            shell.request_redraw_at(now + TICK - elapsed);
            return;
        }
        let factor = (elapsed.as_secs_f32() / TICK.as_secs_f32()).min(3.0);
        active.last_tick = Some(now);
        shell.publish((self.on_scroll)(speed * factor));
        shell.request_redraw_at(now + TICK);
    }
}

impl<M: Clone> Widget<M, Theme, Renderer> for DragList<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<DragState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(DragState::default())
    }

    fn children(&self) -> Vec<Tree> {
        self.items.iter().map(Tree::new).collect()
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&self.items);
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Shrink)
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) -> layout::Node {
        let width = limits.max().width;
        let item_limits = layout::Limits::new(Size::ZERO, Size::new(width, f32::INFINITY));
        let mut y = 0.0;
        let mut nodes = Vec::with_capacity(self.items.len());
        for (item, state) in self.items.iter_mut().zip(&mut tree.children) {
            let node = item.as_widget_mut().layout(state, renderer, &item_limits).move_to(Point::new(0.0, y));
            y += node.size().height + self.spacing;
            nodes.push(node);
        }
        let height = if nodes.is_empty() { 0.0 } else { y - self.spacing };
        layout::Node::with_children(Size::new(width, height), nodes)
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
        let state = tree.state.downcast_mut::<DragState>();
        // The application is the reference for "is a drag in progress".
        if let Some(active) = state.active.as_mut() {
            match self.dragging {
                Some(i) if i == active.from => active.confirmed = true,
                None if active.confirmed => {
                    state.active = None;
                    shell.request_redraw();
                }
                _ => {}
            }
        }
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) if state.active.is_none() => {
                if let Some(position) = cursor.position()
                    && let Some(index) = layout.children().position(|l| self.handle_zone(l.bounds()).contains(position))
                {
                    state.pending = Some((index, position));
                    shell.capture_event();
                    return;
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if let Some((index, start)) = state.pending
                    && let Some(position) = cursor.position()
                    && ((position.y - start.y).abs() > THRESHOLD || (position.x - start.x).abs() > THRESHOLD)
                {
                    let top = layout.children().nth(index).map(|l| l.bounds().y).unwrap_or(start.y);
                    state.pending = None;
                    state.active = Some(Active { from: index, grab: start.y - top, rel_y: position.y - viewport.y, outside: false, confirmed: false, last_tick: None });
                    shell.publish((self.on_start)(index));
                }
                if let Some(active) = state.active.as_mut() {
                    active.track(cursor, viewport);
                    self.auto_scroll(state, viewport, shell);
                    shell.request_redraw();
                    shell.capture_event();
                    return;
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if state.pending.take().is_some() {
                    // A simple click on the handle: nothing to reorder.
                    shell.capture_event();
                    return;
                }
                if let Some(active) = state.active.take() {
                    // Inside the scrolling area: drop at the insertion line.
                    // Outside: cancel.
                    let message = match cursor.position() {
                        Some(position) => {
                            let spans = spans(layout);
                            let slot = reorder::insertion_slot(&spans, position.y);
                            match reorder::target_index(active.from, slot, spans.len()) {
                                Some(to) => (self.on_drop)(active.from, to),
                                None => self.on_cancel.clone(),
                            }
                        }
                        None => self.on_cancel.clone(),
                    };
                    shell.publish(message);
                    shell.request_redraw();
                    shell.capture_event();
                    return;
                }
            }
            Event::Window(iced::window::Event::RedrawRequested(_)) if state.active.is_some() => {
                self.auto_scroll(state, viewport, shell);
            }
            _ => {}
        }
        // While dragging, the items do not react to the mouse.
        if state.active.is_some() && matches!(event, Event::Mouse(_)) {
            return;
        }
        for ((item, child), layout) in self.items.iter_mut().zip(&mut tree.children).zip(layout.children()) {
            item.as_widget_mut().update(child, event, layout, cursor, renderer, clipboard, shell, viewport);
        }
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
        use iced::advanced::Renderer as _;
        let state = tree.state.downcast_ref::<DragState>();
        let active = state.active.as_ref();
        let tokens = style::tokens(theme);
        for (i, ((item, child), item_layout)) in self.items.iter().zip(&tree.children).zip(layout.children()).enumerate() {
            item.as_widget().draw(child, renderer, theme, style, item_layout, cursor, viewport);
            if active.is_some_and(|a| a.from == i) {
                // The dragged item stays in place, dimmed.
                let bounds = item_layout.bounds();
                renderer.with_layer(bounds, |r| {
                    r.fill_quad(
                        renderer::Quad { bounds, border: Border { radius: 8.0.into(), ..Border::default() }, ..renderer::Quad::default() },
                        Background::Color(Color { a: 0.65, ..style::panel_background(theme) }),
                    );
                });
            }
        }
        let Some(active) = active else { return };
        let Some(from_layout) = layout.children().nth(active.from) else { return };
        let Some((item, child)) = self.items.get(active.from).zip(tree.children.get(active.from)) else { return };
        // A lifted copy follows the pointer vertically.
        let bounds = from_layout.bounds();
        let pointer_y = active.pointer_y(viewport);
        let dy = pointer_y - active.grab - bounds.y;
        renderer.with_layer(*viewport, |r| {
            r.with_translation(Vector::new(0.0, dy), |r| {
                // A soft shadow made of a few translucent layers (the software
                // renderer neither clips nor repaints blurred quad shadows).
                for (grow, alpha) in [(7.0, 0.05), (4.5, 0.07), (2.0, 0.12)] {
                    r.fill_quad(
                        renderer::Quad {
                            bounds: Rectangle {
                                x: bounds.x - grow,
                                y: bounds.y - grow + 5.0,
                                width: bounds.width + 2.0 * grow,
                                height: bounds.height + 2.0 * grow,
                            },
                            border: Border { radius: (8.0 + grow).into(), ..Border::default() },
                            ..renderer::Quad::default()
                        },
                        Background::Color(Color::from_rgba(0.0, 0.0, 0.0, alpha)),
                    );
                }
                r.fill_quad(
                    renderer::Quad {
                        bounds,
                        border: Border { radius: 8.0.into(), width: 1.5, color: tokens.accent },
                        ..renderer::Quad::default()
                    },
                    Background::Color(style::panel_background(theme)),
                );
                item.as_widget().draw(child, r, theme, style, from_layout, mouse::Cursor::Unavailable, &Rectangle { y: viewport.y - dy, ..*viewport });
            });
        });
        // Insertion line (none outside the scrolling area, where releasing cancels).
        if active.outside {
            return;
        }
        let spans = spans(layout);
        let slot = reorder::insertion_slot(&spans, pointer_y);
        if let Some(y) = reorder::indicator_y(&spans, slot, self.spacing) {
            let list = layout.bounds();
            renderer.with_layer(*viewport, |r| {
                r.fill_quad(
                    renderer::Quad {
                        bounds: Rectangle { x: list.x, y: y - 1.5, width: list.width, height: 3.0 },
                        border: Border { radius: 1.5.into(), ..Border::default() },
                        ..renderer::Quad::default()
                    },
                    Background::Color(tokens.accent),
                );
                let dot = if super::widgets::is_rtl() { list.x + list.width - 9.0 } else { list.x };
                r.fill_quad(
                    renderer::Quad {
                        bounds: Rectangle { x: dot, y: y - 4.5, width: 9.0, height: 9.0 },
                        border: Border { radius: 4.5.into(), ..Border::default() },
                        ..renderer::Quad::default()
                    },
                    Background::Color(tokens.accent),
                );
            });
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
        let state = tree.state.downcast_ref::<DragState>();
        if state.active.is_some() {
            return mouse::Interaction::Grabbing;
        }
        if let Some(position) = cursor.position()
            && layout.children().any(|l| self.handle_zone(l.bounds()).contains(position))
        {
            return mouse::Interaction::Grab;
        }
        self.items
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
            .map(|((item, child), layout)| item.as_widget().mouse_interaction(child, layout, cursor, viewport, renderer))
            .max()
            .unwrap_or_default()
    }

    fn operate(&mut self, tree: &mut Tree, layout: Layout<'_>, renderer: &Renderer, operation: &mut dyn Operation) {
        operation.container(None, layout.bounds());
        let items = &mut self.items;
        operation.traverse(&mut |operation| {
            for ((item, child), layout) in items.iter_mut().zip(&mut tree.children).zip(layout.children()) {
                item.as_widget_mut().operate(child, layout, renderer, operation);
            }
        });
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, M, Theme, Renderer>> {
        overlay::from_children(&mut self.items, tree, layout, renderer, viewport, translation)
    }
}
