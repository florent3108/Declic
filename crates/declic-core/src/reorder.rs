//! Reordering a vertical list by drag-and-drop (macro steps): where an item
//! dropped at a given pointer position lands, and how the list changes.
//!
//! Positions are vertical coordinates in the list's own space; each item is
//! described by its top and bottom edges, in display order.

/// Slot (0 ..= number of items) before which a dragged item would be inserted
/// when the pointer is at `y`: before the first item whose middle is below
/// the pointer, or after the last one.
pub fn insertion_slot(items: &[(f32, f32)], y: f32) -> usize {
    items.iter().position(|(top, bottom)| y < (top + bottom) / 2.0).unwrap_or(items.len())
}

/// New index of the item at `from` when it is inserted at `slot` in a list of
/// `len` items, or `None` when the order would not change.
pub fn target_index(from: usize, slot: usize, len: usize) -> Option<usize> {
    if from >= len || len < 2 {
        return None;
    }
    let slot = slot.min(len);
    let to = if slot > from { slot - 1 } else { slot };
    (to != from).then_some(to)
}

/// Vertical position of the insertion line for `slot`: in the middle of the
/// gap between two items, or just outside the first or last one.
pub fn indicator_y(items: &[(f32, f32)], slot: usize, gap: f32) -> Option<f32> {
    let first = items.first()?;
    let last = items.last()?;
    Some(if slot == 0 {
        first.0 - gap / 2.0
    } else if slot >= items.len() {
        last.1 + gap / 2.0
    } else {
        (items[slot - 1].1 + items[slot].0) / 2.0
    })
}

/// Moves the item at `from` to index `to` (the others keep their order).
pub fn move_item<T>(items: &mut Vec<T>, from: usize, to: usize) {
    if from < items.len() && to < items.len() && from != to {
        let item = items.remove(from);
        items.insert(to, item);
    }
}

/// Index, after moving `from` to `to`, of the item that was at `index`.
pub fn index_after_move(index: usize, from: usize, to: usize) -> usize {
    if index == from {
        to
    } else if from < index && index <= to {
        index - 1
    } else if to <= index && index < from {
        index + 1
    } else {
        index
    }
}

/// Auto-scroll speed (pixels per tick, negative upwards) when dragging at `y`
/// in a visible area from `top` to `bottom`: zero in the middle, growing up to
/// `max` as the pointer goes into the `zone` along an edge (and beyond).
pub fn auto_scroll(y: f32, top: f32, bottom: f32, zone: f32, max: f32) -> f32 {
    if bottom - top <= 2.0 * zone || zone <= 0.0 {
        return 0.0;
    }
    if y < top + zone {
        -max * ((top + zone - y) / zone).min(1.0)
    } else if y > bottom - zone {
        max * ((y - (bottom - zone)) / zone).min(1.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Five items of 40 px separated by 10 px: 0-40, 50-90, 100-140, 150-190, 200-240.
    fn items() -> Vec<(f32, f32)> {
        (0..5).map(|i| (i as f32 * 50.0, i as f32 * 50.0 + 40.0)).collect()
    }

    #[test]
    fn slot_follows_item_middles() {
        let items = items();
        assert_eq!(insertion_slot(&items, -30.0), 0);
        assert_eq!(insertion_slot(&items, 19.0), 0);
        assert_eq!(insertion_slot(&items, 21.0), 1);
        assert_eq!(insertion_slot(&items, 95.0), 2);
        assert_eq!(insertion_slot(&items, 221.0), 5);
        assert_eq!(insertion_slot(&items, 900.0), 5);
        assert_eq!(insertion_slot(&[], 10.0), 0);
    }

    #[test]
    fn target_index_edge_cases() {
        // Dropping over its own place (before or after itself) changes nothing.
        assert_eq!(target_index(2, 2, 5), None);
        assert_eq!(target_index(2, 3, 5), None);
        // First to last, last to first.
        assert_eq!(target_index(0, 5, 5), Some(4));
        assert_eq!(target_index(4, 0, 5), Some(0));
        // Down and up by one.
        assert_eq!(target_index(1, 3, 5), Some(2));
        assert_eq!(target_index(3, 2, 5), Some(2));
        // A one-step macro cannot be reordered; out-of-range values are safe.
        assert_eq!(target_index(0, 1, 1), None);
        assert_eq!(target_index(0, 0, 1), None);
        assert_eq!(target_index(7, 0, 5), None);
        assert_eq!(target_index(0, 99, 5), Some(4));
    }

    #[test]
    fn pointer_position_to_new_order() {
        let items = items();
        let drop = |from: usize, y: f32| {
            let mut list = vec!['a', 'b', 'c', 'd', 'e'];
            if let Some(to) = target_index(from, insertion_slot(&items, y), list.len()) {
                move_item(&mut list, from, to);
            }
            list.into_iter().collect::<String>()
        };
        // Drag "a" below "c" (pointer between c and d).
        assert_eq!(drop(0, 145.0), "bcade");
        // Drag "e" to the top.
        assert_eq!(drop(4, 5.0), "eabcd");
        // Drag "b" onto its own middle: unchanged.
        assert_eq!(drop(1, 70.0), "abcde");
        // Drag "c" past the end.
        assert_eq!(drop(2, 400.0), "abdec");
    }

    #[test]
    fn indicator_between_items() {
        let items = items();
        assert_eq!(indicator_y(&items, 0, 10.0), Some(-5.0));
        assert_eq!(indicator_y(&items, 2, 10.0), Some(95.0));
        assert_eq!(indicator_y(&items, 5, 10.0), Some(245.0));
        assert_eq!(indicator_y(&[], 0, 10.0), None);
    }

    #[test]
    fn selection_follows_the_moved_item() {
        let mut list = vec![0, 1, 2, 3, 4];
        move_item(&mut list, 1, 3);
        assert_eq!(list, [0, 2, 3, 1, 4]);
        for (old, value) in [0, 1, 2, 3, 4].iter().enumerate() {
            assert_eq!(list[index_after_move(old, 1, 3)], *value);
        }
        let mut list = vec![0, 1, 2, 3, 4];
        move_item(&mut list, 4, 0);
        for (old, value) in [0, 1, 2, 3, 4].iter().enumerate() {
            assert_eq!(list[index_after_move(old, 4, 0)], *value);
        }
        // Invalid moves leave the list untouched.
        move_item(&mut list, 9, 0);
        assert_eq!(list.len(), 5);
    }

    #[test]
    fn auto_scroll_near_edges() {
        assert_eq!(auto_scroll(300.0, 100.0, 500.0, 40.0, 12.0), 0.0);
        assert_eq!(auto_scroll(120.0, 100.0, 500.0, 40.0, 12.0), -6.0);
        assert_eq!(auto_scroll(50.0, 100.0, 500.0, 40.0, 12.0), -12.0);
        assert_eq!(auto_scroll(490.0, 100.0, 500.0, 40.0, 12.0), 9.0);
        assert_eq!(auto_scroll(700.0, 100.0, 500.0, 40.0, 12.0), 12.0);
        // Area too small for two zones: no auto-scroll.
        assert_eq!(auto_scroll(110.0, 100.0, 150.0, 40.0, 12.0), 0.0);
    }
}
