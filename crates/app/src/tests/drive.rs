//! What a driven window says of its scene (`drive::outline`), and when
//! it has a new one.

use super::*;

#[test]
fn the_outline_says_the_text_being_typed_and_its_caret() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let config = tempfile::tempdir().unwrap();
        let mut w = world(daemon, config.path());
        w.type_word(300.0, 10.0);
        let scene = view(&w.settings, &mut w.shared, &w.tools, &w.sidebar, &mut w.canvases);
        let said = crate::drive::outline(&scene);
        assert!(said.starts_with("window "), "{said}");
        assert!(said.contains("\"word\""), "{said}");
        assert!(said.contains(" caret"), "{said}");
        assert!(said.contains("footer "), "{said}");
    });
}

/// What `event` gives the window to draw, routed as the window's answer does.
fn routed(w: &mut World, event: Event) -> Step {
    let routed = route(
        event,
        &mut w.shared,
        &mut w.canvases,
        &mut w.tools,
        &w.settings,
        &w.socket,
        &mut w.sidebar,
    );
    match routed {
        Routed::Answer(step) | Routed::Watch(_, step) => step,
        Routed::News(_) | Routed::Launch(_) => Step::Stay,
    }
}

#[test]
fn a_command_draws_its_scene_at_once() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let config = tempfile::tempdir().unwrap();
        let mut w = world(daemon, config.path());
        let canvas_at = |scene: &crate::scene::Scene| {
            crate::drive::outline(scene)
                .lines()
                .find(|l| l.starts_with("canvas "))
                .map(str::to_string)
        };
        let shut =
            canvas_at(&view(&w.settings, &mut w.shared, &w.tools, &w.sidebar, &mut w.canvases));
        for open in [true, false] {
            let Step::Draw(scene) = routed(&mut w, Event::Command(Command::ToggleSidebar)) else {
                panic!("toggle-sidebar draws");
            };
            assert_eq!(w.sidebar.is_open(), open);
            assert_eq!(
                canvas_at(&scene) == shut,
                !open,
                "the canvas pane moves over for the sidebar"
            );
        }
    });
}

#[test]
fn the_outline_says_the_text_typed_in_an_entry_that_drew_nothing() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let config = tempfile::tempdir().unwrap();
        let mut w = world(daemon, config.path());
        w.key(Chord::new(CMD, 'f'), "");
        let mut scene = view(&w.settings, &mut w.shared, &w.tools, &w.sidebar, &mut w.canvases);
        assert!(scene.entry.is_some(), "find opens its entry");
        let step = routed(&mut w, Event::Typed("abc".into()));
        assert!(!matches!(step, Step::Draw(_)), "the field shows its own typing");
        crate::drive::shown(&mut scene, Some("abc".into()), &step);
        let said = crate::drive::outline(&scene);
        assert!(said.contains("entry ") && said.contains("\"abc\""), "{said}");
    });
}
