//! What a driven window says of its scene (`drive::outline`).

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
