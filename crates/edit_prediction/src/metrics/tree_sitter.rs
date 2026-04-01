use language::SyntaxLayer;

pub fn count_tree_sitter_errors<'a>(layers: impl Iterator<Item = SyntaxLayer<'a>>) -> usize {
    layers
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

#[cfg(test)]
mod tests {
    use std::ops::Range;

    use super::count_tree_sitter_errors;
    use gpui::{AppContext as _, TestAppContext};
    use language::{Buffer, BufferSnapshot, rust_lang};

    fn error_count_in_range(edited_buffer_snapshot: &BufferSnapshot, range: Range<usize>) -> usize {
        let layers = edited_buffer_snapshot.syntax_layers_for_range(range, true);
        count_tree_sitter_errors(layers)
    }

    fn rust_snapshot(text: &str, cx: &mut TestAppContext) -> BufferSnapshot {
        let buffer = cx.new(|cx| Buffer::local(text, cx).with_language(rust_lang(), cx));
        while buffer.read_with(cx, |buffer, _| buffer.is_parsing()) {
            cx.run_until_parked();
        }
        buffer.read_with(cx, |buffer, _| buffer.snapshot())
    }

    #[gpui::test]
    async fn counts_no_errors_for_valid_rust(cx: &mut TestAppContext) {
        let text = "fn helper(value: usize) -> usize {\n    value + 1\n}\n";
        let snapshot = rust_snapshot(text, cx);

        assert_eq!(error_count_in_range(&snapshot, 0..snapshot.text.len()), 0);
    }

    #[gpui::test]
    async fn counts_errors_for_invalid_rust(cx: &mut TestAppContext) {
        let text = "fn helper(value: usize) -> usize {\n    let total = ;\n    if value > 0 {\n        value +\n    }\n}\n";
        let snapshot = rust_snapshot(text, cx);

        assert_eq!(error_count_in_range(&snapshot, 0..snapshot.text.len()), 2);
    }

    #[gpui::test]
    async fn counts_no_errors_for_subrange_of_valid_rust(cx: &mut TestAppContext) {
        let text = "fn first() -> usize {\n    let value = 1;\n    value + 1\n}\n";
        let snapshot = rust_snapshot(text, cx);
        let body_start = text.find("let value").unwrap();
        let body_end = body_start + "let value = 1;".len();

        assert_eq!(error_count_in_range(&snapshot, body_start..body_end), 0);
    }

    #[gpui::test]
    async fn counts_errors_for_subrange_of_invalid_rust(cx: &mut TestAppContext) {
        let text = "fn second() -> usize {\n    let broken = ;\n    broken\n}\n";
        let snapshot = rust_snapshot(text, cx);
        let error_start = text.find("let broken = ;").unwrap();
        let error_end = error_start + "let broken = ;".len();

        assert_eq!(error_count_in_range(&snapshot, error_start..error_end), 1);
    }
}
