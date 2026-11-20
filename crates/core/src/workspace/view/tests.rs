//! Kept pane views: `set` refuses a number that is not finite or a zoom of 0
//! or less, `keep` drops gone panes, `copy`, a view only for the canvas the
//! pane shows, and the views file's text read back.

use super::*;

fn view(canvas: &str, x: f64, zoom: f64) -> PaneView {
    PaneView { canvas: canvas.into(), page: Some("p2".into()), x, y: -4.5, zoom }
}

#[test]
fn a_view_keeps_finite_numbers_and_a_zoom_above_0() {
    let mut views = Views::default();
    views.set(1, view("notes", 10.0, 0.5)).unwrap();
    for bad in [
        view("notes", f64::NAN, 1.0),
        view("notes", f64::INFINITY, 1.0),
        view("notes", 0.0, 0.0),
        view("notes", 0.0, -1.0),
        view("notes", 0.0, f64::NAN),
    ] {
        let err = views.set(1, bad.clone()).unwrap_err();
        assert_eq!(err.code(), "invalid", "{bad:?}");
        assert!(err.hint().is_some(), "{bad:?}");
    }
    assert_eq!(
        views.view_of(1, "notes"),
        Some(&view("notes", 10.0, 0.5)),
        "a refusal keeps the view"
    );
}

#[test]
fn a_view_is_given_only_for_the_canvas_its_pane_shows() {
    let mut views = Views::default();
    views.set(3, view("notes", 1.0, 1.0)).unwrap();
    assert_eq!(views.view_of(3, "notes"), Some(&view("notes", 1.0, 1.0)));
    assert_eq!(views.view_of(3, "board"), None, "the pane shows another canvas now");
    assert_eq!(views.view_of(4, "notes"), None, "no view kept");
}

#[test]
fn keep_drops_the_views_and_the_focus_of_panes_that_are_gone() {
    let mut views = Views::default();
    for id in [1, 2, 3] {
        views.set(id, view("notes", f64::from(id), 1.0)).unwrap();
    }
    views.focus = Some(2);
    views.keep(&[1, 2, 3]);
    assert_eq!((views.focus, views.panes.len()), (Some(2), 3), "nothing gone, nothing dropped");
    views.keep(&[1, 3]);
    assert_eq!(views.focus, None);
    assert_eq!(views.panes.keys().copied().collect::<Vec<_>>(), [1, 3]);
    views.keep(&[]);
    assert!(views.is_empty());
}

#[test]
fn copy_gives_a_new_pane_the_source_view_and_nothing_when_there_is_none() {
    let mut views = Views::default();
    views.set(1, view("notes", 7.0, 2.0)).unwrap();
    views.copy(1, 4);
    assert_eq!(views.view_of(4, "notes"), views.view_of(1, "notes"));
    views.copy(9, 5);
    assert_eq!(views.view_of(5, "notes"), None);
}

#[test]
fn the_views_of_each_workspace_read_back_from_their_text() {
    let mut kept = Kept::default();
    let mut a = Views { focus: Some(2), ..Views::default() };
    a.set(1, view("notes", 1.5, 0.25)).unwrap();
    a.set(2, PaneView { page: None, ..view("board", 0.0, 1.0) }).unwrap();
    kept.put("/w/a", a.clone());
    kept.put("/w/b", Views { focus: Some(1), ..Views::default() });
    kept.put("/w/c", Views::default());
    let text = kept.to_text();
    assert!(text.starts_with(r#"{"v":1,"workspaces":{"/w/a":{"focus":2,"panes":{"1":"#), "{text}");
    assert!(text.ends_with("}\n") && !text.contains("/w/c"), "{text}");
    let back = Kept::read(&text);
    assert_eq!(back, kept);
    assert_eq!(back.of("/w/a"), a);
    assert_eq!(back.of("/w/none"), Views::default());
    kept.put("/w/a", Views::default());
    assert_eq!(kept.of("/w/a"), Views::default(), "empty views leave the workspace out");
}

#[test]
fn text_that_cannot_be_read_is_no_views_and_a_bad_view_is_left_out_alone() {
    for text in ["", "not json", "[]", r#"{"v":2,"workspaces":{"/w":{"focus":1}}}"#, r#"{"v":1}"#] {
        assert_eq!(Kept::read(text), Kept::default(), "{text}");
    }
    let text = r#"{"v":1,"workspaces":{
        "/w":{"focus":1,"panes":{
            "1":{"canvas":"notes","x":1,"y":2,"zoom":1},
            "2":{"canvas":"notes","x":1,"y":2,"zoom":0},
            "x":{"canvas":"notes","x":1,"y":2,"zoom":1},
            "3":{"canvas":"notes","x":"far","y":2,"zoom":1}}},
        "/bad":7,
        "/empty":{"panes":{"1":{"zoom":-1}}}}}"#;
    let kept = Kept::read(text);
    let w = kept.of("/w");
    assert_eq!(w.focus, Some(1));
    assert_eq!(w.panes.keys().copied().collect::<Vec<_>>(), [1]);
    let one = PaneView { canvas: "notes".into(), page: None, x: 1.0, y: 2.0, zoom: 1.0 };
    assert_eq!(w.view_of(1, "notes"), Some(&one));
    assert_eq!(kept.workspaces.len(), 1, "{kept:?}");
}
