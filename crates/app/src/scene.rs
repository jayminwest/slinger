//! What the window shows. Pure: every decision about content is made here, so
//! `platform` only draws what a [`Scene`] says. [`canvas`] decides what a
//! canvas pane shows.

#[cfg_attr(not(test), expect(dead_code, reason = "the canvas pane draws it from slinger-c94c"))]
pub mod canvas;

use crate::backend::Problem;
use crate::layout::{self, Rect, Size};

/// How a label reads. `platform` maps each tone to a system color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Primary,
    Secondary,
    Error,
}

/// One line of text at a fixed place.
#[derive(Debug, Clone, PartialEq)]
pub struct Label {
    pub text: String,
    pub tone: Tone,
    pub frame: Rect,
}

/// One window, fully decided.
#[derive(Debug, Clone, PartialEq)]
pub struct Scene {
    pub title: String,
    pub size: Size,
    pub labels: Vec<Label>,
}

/// The window for a greeting, or for the problem that stopped it: the message
/// and, under it, the hint.
pub fn build(greeting: Result<String, Problem>) -> Scene {
    let texts = match greeting {
        Ok(text) => vec![(text, Tone::Primary)],
        Err(problem) => std::iter::once((format!("✗ {}", problem.message), Tone::Error))
            .chain(problem.hint.map(|hint| (hint, Tone::Secondary)))
            .collect(),
    };
    let size = layout::WINDOW;
    let frames = layout::lines(size, texts.len());
    let labels = texts
        .into_iter()
        .zip(frames)
        .map(|((text, tone), frame)| Label { text, tone, frame })
        .collect();
    Scene { title: "slinger".to_string(), size, labels }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(scene: &Scene) -> Vec<(&str, Tone)> {
        scene.labels.iter().map(|l| (l.text.as_str(), l.tone)).collect()
    }

    #[test]
    fn shows_the_greeting_alone() {
        let scene = build(Ok("hello, ada".into()));
        assert_eq!(scene.title, "slinger");
        assert_eq!(shown(&scene), [("hello, ada", Tone::Primary)]);
    }

    #[test]
    fn shows_a_problem_with_its_hint_under_it() {
        let problem = Problem { message: "name is empty".into(), hint: Some("pass a name".into()) };
        let scene = build(Err(problem));
        assert_eq!(
            shown(&scene),
            [("✗ name is empty", Tone::Error), ("pass a name", Tone::Secondary)]
        );
        assert!(scene.labels[0].frame.y < scene.labels[1].frame.y);
    }

    #[test]
    fn shows_a_problem_without_a_hint_as_one_line() {
        let scene = build(Err(Problem { message: "broken".into(), hint: None }));
        assert_eq!(shown(&scene), [("✗ broken", Tone::Error)]);
    }
}
