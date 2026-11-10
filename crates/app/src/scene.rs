//! What the window shows. Pure: every decision about content is made here, so
//! `platform` only draws what a [`Scene`] says. [`canvas`] decides what a
//! canvas pane shows.

#[cfg_attr(not(test), expect(dead_code, reason = "the canvas pane draws it from slinger-c94c"))]
pub mod canvas;

use crate::backend::Problem;
use crate::config::{Rgb, Theme};
use crate::layout::{self, Rect, Size};

/// One line of text at a fixed place.
#[derive(Debug, Clone, PartialEq)]
pub struct Label {
    pub text: String,
    pub colour: Rgb,
    pub frame: Rect,
}

/// One window, fully decided.
#[derive(Debug, Clone, PartialEq)]
pub struct Scene {
    pub title: String,
    pub size: Size,
    pub background: Rgb,
    pub labels: Vec<Label>,
}

/// The window for a greeting, or for the problem that stopped it, in
/// `theme`'s colours. Under it, the problem with the config files, if any:
/// the window keeps working on the last good config and says what is wrong.
pub fn build(greeting: &Result<String, Problem>, theme: &Theme, config: Option<&Problem>) -> Scene {
    let mut texts = Vec::new();
    let problem = |texts: &mut Vec<_>, p: &Problem| {
        texts.push((format!("✗ {}", p.message), theme.error));
        if let Some(hint) = &p.hint {
            texts.push((hint.clone(), theme.muted));
        }
    };
    match greeting {
        Ok(text) => texts.push((text.clone(), theme.text)),
        Err(p) => problem(&mut texts, p),
    }
    if let Some(p) = config {
        problem(&mut texts, p);
    }
    let size = layout::WINDOW;
    let frames = layout::lines(size, texts.len());
    let labels = texts
        .into_iter()
        .zip(frames)
        .map(|((text, colour), frame)| Label { text, colour, frame })
        .collect();
    Scene { title: "slinger".to_string(), size, background: theme.background, labels }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: Theme = Theme {
        background: Rgb { r: 0, g: 0, b: 0 },
        text: Rgb { r: 1, g: 1, b: 1 },
        muted: Rgb { r: 2, g: 2, b: 2 },
        error: Rgb { r: 3, g: 3, b: 3 },
    };

    fn shown(scene: &Scene) -> Vec<(&str, Rgb)> {
        scene.labels.iter().map(|l| (l.text.as_str(), l.colour)).collect()
    }

    #[test]
    fn shows_the_greeting_alone_in_the_themes_colours() {
        let scene = build(&Ok("hello, ada".into()), &T, None);
        assert_eq!(scene.title, "slinger");
        assert_eq!(scene.background, T.background);
        assert_eq!(shown(&scene), [("hello, ada", T.text)]);
    }

    #[test]
    fn shows_a_problem_with_its_hint_under_it() {
        let problem = Problem { message: "name is empty".into(), hint: Some("pass a name".into()) };
        let scene = build(&Err(problem), &T, None);
        assert_eq!(shown(&scene), [("✗ name is empty", T.error), ("pass a name", T.muted)]);
        assert!(scene.labels[0].frame.y < scene.labels[1].frame.y);
    }

    #[test]
    fn shows_a_problem_without_a_hint_as_one_line() {
        let scene = build(&Err(Problem { message: "broken".into(), hint: None }), &T, None);
        assert_eq!(shown(&scene), [("✗ broken", T.error)]);
    }

    #[test]
    fn shows_a_config_problem_under_the_greeting() {
        let config = Problem { message: "config.toml line 2: x".into(), hint: Some("h".into()) };
        let scene = build(&Ok("hello".into()), &T, Some(&config));
        assert_eq!(
            shown(&scene),
            [("hello", T.text), ("✗ config.toml line 2: x", T.error), ("h", T.muted)]
        );
    }
}
