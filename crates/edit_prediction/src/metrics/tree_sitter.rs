use std::ops::Range;

use language::BufferSnapshot;

pub fn ts_error_count_in_range(
    edited_buffer_snapshot: &BufferSnapshot,
    range: Range<usize>,
) -> usize {
    edited_buffer_snapshot
        .syntax_layers_for_range(range, true)
        .map(|layer| {
            let node = layer.node();
            let mut count = 0;
            let mut cursor = node.walk();
            let mut done = false;
            while !done {
                let current = cursor.node();
                if current.is_error() || current.is_missing() {
                    count += 1;
                }
                if current.has_error() && cursor.goto_first_child() {
                    continue;
                }
                if cursor.goto_next_sibling() {
                    continue;
                }
                loop {
                    if !cursor.goto_parent() {
                        done = true;
                        break;
                    }
                    if cursor.goto_next_sibling() {
                        break;
                    }
                }
            }
            count
        })
        .sum()
}
