//! `RenderLog` — a diagnostic panel that records which components re-render
//! and when, letting you observe GPUI's caching behaviour in real time.
//!
//! Renders are grouped by frame (delimited by `begin_frame()` calls).
//! Consecutive frames with the same set of components are collapsed into
//! a single line with a repeat counter, so an animation that re-renders
//! `ExampleInput + ExampleEditor` 30 times shows as one line with `×30`.

use std::time::Instant;

use gpui::{App, Context, Entity, IntoViewElement, Window, div, hsla, prelude::*, px};

// ---------------------------------------------------------------------------
// RenderLog entity
// ---------------------------------------------------------------------------

pub struct RenderLog {
    current_frame: Vec<&'static str>,
    frames: Vec<RenderFrame>,
    start_time: Instant,
}

struct RenderFrame {
    components: Vec<&'static str>,
    count: usize,
    last_timestamp: Instant,
}

impl RenderLog {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        Self {
            current_frame: Vec::new(),
            frames: Vec::new(),
            start_time: Instant::now(),
        }
    }

    /// Mark the start of a new render frame. Finalizes the previous frame
    /// and either merges it with the last entry (if the same components
    /// rendered) or pushes a new entry.
    ///
    /// Call this at the top of the root view's `render()`, before any
    /// children have a chance to call `log()`.
    pub fn begin_frame(&mut self) {
        if self.current_frame.is_empty() {
            return;
        }

        let mut components = std::mem::take(&mut self.current_frame);
        components.sort();
        components.dedup();

        let now = Instant::now();

        if let Some(last) = self.frames.last_mut() {
            if last.components == components {
                last.count += 1;
                last.last_timestamp = now;
                return;
            }
        }

        self.frames.push(RenderFrame {
            components,
            count: 1,
            last_timestamp: now,
        });

        if self.frames.len() > 50 {
            self.frames.drain(0..self.frames.len() - 50);
        }
    }

    /// Record that `component` rendered in the current frame.
    /// Does **not** call `cx.notify()` — the panel updates passively when
    /// its parent re-renders, avoiding an infinite invalidation loop.
    pub fn log(&mut self, component: &'static str) {
        self.current_frame.push(component);
    }

    #[cfg(test)]
    fn frame_count(&self) -> usize {
        self.frames.len()
    }

    #[cfg(test)]
    fn frame_at(&self, index: usize) -> Option<(&[&'static str], usize)> {
        self.frames
            .get(index)
            .map(|f| (f.components.as_slice(), f.count))
    }
}

// ---------------------------------------------------------------------------
// RenderLogPanel — stateless ComponentView that displays the log
// ---------------------------------------------------------------------------

#[derive(Hash, IntoViewElement)]
pub struct RenderLogPanel {
    log: Entity<RenderLog>,
}

impl RenderLogPanel {
    pub fn new(log: Entity<RenderLog>) -> Self {
        Self { log }
    }
}

impl gpui::ComponentView for RenderLogPanel {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let log = self.log.read(cx);
        let start = log.start_time;

        div()
            .flex()
            .flex_col()
            .gap(px(1.))
            .p(px(8.))
            .bg(hsla(0., 0., 0.12, 1.))
            .rounded(px(4.))
            .max_h(px(180.))
            .overflow_hidden()
            .child(
                div()
                    .text_xs()
                    .text_color(hsla(0., 0., 0.55, 1.))
                    .mb(px(4.))
                    .child("Render log"),
            )
            .children(
                log.frames
                    .iter()
                    .rev()
                    .take(20)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .map(|frame| {
                        let elapsed = frame.last_timestamp.duration_since(start);
                        let secs = elapsed.as_secs_f64();
                        let names = frame.components.join(", ");
                        let count_str = if frame.count > 1 {
                            format!(" ×{}", frame.count)
                        } else {
                            String::new()
                        };

                        div()
                            .flex()
                            .text_xs()
                            .child(
                                div()
                                    .text_color(hsla(120. / 360., 0.7, 0.65, 1.))
                                    .child(names),
                            )
                            .child(
                                div()
                                    .text_color(hsla(50. / 360., 0.8, 0.65, 1.))
                                    .child(count_str),
                            )
                            .child(
                                div()
                                    .text_color(hsla(0., 0., 0.4, 1.))
                                    .ml(px(8.))
                                    .child(format!("+{:.1}s", secs)),
                            )
                    }),
            )
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn new_log() -> RenderLog {
        RenderLog {
            current_frame: Vec::new(),
            frames: Vec::new(),
            start_time: Instant::now(),
        }
    }

    #[test]
    fn test_log_groups_by_frame() {
        let mut log = new_log();

        log.log("ExampleInput");
        log.log("ExampleEditor");
        log.begin_frame();

        assert_eq!(log.frame_count(), 1);
        assert_eq!(
            log.frame_at(0),
            Some((["ExampleEditor", "ExampleInput"].as_slice(), 1))
        );
    }

    #[test]
    fn test_consecutive_identical_frames_collapse() {
        let mut log = new_log();

        // Three identical frames: Input + Editor
        log.log("ExampleInput");
        log.log("ExampleEditor");
        log.begin_frame();

        log.log("ExampleInput");
        log.log("ExampleEditor");
        log.begin_frame();

        log.log("ExampleEditor");
        log.log("ExampleInput");
        log.begin_frame();

        // Should collapse to one entry with count 3
        assert_eq!(log.frame_count(), 1);
        assert_eq!(
            log.frame_at(0),
            Some((["ExampleEditor", "ExampleInput"].as_slice(), 3))
        );
    }

    #[test]
    fn test_different_frames_dont_collapse() {
        let mut log = new_log();

        log.log("ExampleInput");
        log.log("ExampleEditor");
        log.begin_frame();

        log.log("EditorInfo");
        log.begin_frame();

        assert_eq!(log.frame_count(), 2);
        assert_eq!(
            log.frame_at(0),
            Some((["ExampleEditor", "ExampleInput"].as_slice(), 1))
        );
        assert_eq!(log.frame_at(1), Some((["EditorInfo"].as_slice(), 1)));
    }

    #[test]
    fn test_collapse_resumes_after_different_frame() {
        let mut log = new_log();

        // 2x Input+Editor, then 1x EditorInfo, then 3x Input+Editor
        for _ in 0..2 {
            log.log("ExampleInput");
            log.log("ExampleEditor");
            log.begin_frame();
        }

        log.log("EditorInfo");
        log.begin_frame();

        for _ in 0..3 {
            log.log("ExampleInput");
            log.log("ExampleEditor");
            log.begin_frame();
        }

        assert_eq!(log.frame_count(), 3);
        assert_eq!(log.frame_at(0).map(|(_, c)| c), Some(2));
        assert_eq!(log.frame_at(1).map(|(_, c)| c), Some(1));
        assert_eq!(log.frame_at(2).map(|(_, c)| c), Some(3));
    }

    #[test]
    fn test_empty_frame_is_ignored() {
        let mut log = new_log();

        log.begin_frame();
        assert_eq!(log.frame_count(), 0);

        log.log("ExampleInput");
        log.begin_frame();
        assert_eq!(log.frame_count(), 1);

        log.begin_frame();
        assert_eq!(log.frame_count(), 1);
    }

    #[test]
    fn test_duplicate_components_in_frame_are_deduped() {
        let mut log = new_log();

        log.log("ExampleInput");
        log.log("ExampleInput");
        log.log("ExampleEditor");
        log.begin_frame();

        assert_eq!(log.frame_count(), 1);
        assert_eq!(
            log.frame_at(0),
            Some((["ExampleEditor", "ExampleInput"].as_slice(), 1))
        );
    }
}
