use super::*;

const WIDE: Rect = Rect { x: 0.0, y: 0.0, width: 400.0, height: 200.0 };

#[test]
fn centers_one_line() {
    let expected = Rect { x: 24.0, y: 89.0, width: 352.0, height: 22.0 };
    assert_eq!(lines(WIDE, 1), [expected]);
}

#[test]
fn stacks_lines_with_a_gap_and_centers_the_stack() {
    let at = |y| Rect { x: 24.0, y, width: 352.0, height: 22.0 };
    assert_eq!(lines(WIDE, 2), [at(75.0), at(103.0)]);
}

#[test]
fn no_lines_is_no_rectangles() {
    assert_eq!(lines(WIDE, 0), []);
}

#[test]
fn a_window_too_small_keeps_the_top_margin_and_a_zero_width() {
    let rects = lines(Rect { x: 0.0, y: 0.0, width: 30.0, height: 40.0 }, 2);
    assert_eq!(rects[0], Rect { x: 24.0, y: 24.0, width: 0.0, height: 22.0 });
    assert_eq!(rects[1].y, 52.0);
}

const SCREEN: Size = Size { width: 800.0, height: 600.0 };

fn pt(x: f64, y: f64) -> Point {
    Point { x, y }
}

fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect { x, y, width, height }
}

fn near(a: Point, b: Point) -> bool {
    let scale = 1.0_f64.max(a.x.abs()).max(a.y.abs());
    (a.x - b.x).abs() <= 1e-9 * scale && (a.y - b.y).abs() <= 1e-9 * scale
}

/// A viewport that is neither at the origin nor at 100%.
fn moved() -> Viewport {
    Viewport::default().pan(-130.0, 45.0).zoom_about(pt(300.0, 200.0), 2.5)
}

#[test]
fn around_no_points_is_none_and_around_points_is_their_box() {
    assert_eq!(Rect::around([]), None);
    assert_eq!(Rect::around([pt(3.0, 4.0)]), Some(rect(3.0, 4.0, 0.0, 0.0)));
    assert_eq!(
        Rect::around([pt(3.0, 4.0), pt(-1.0, 9.0), pt(5.0, 0.0)]),
        Some(rect(-1.0, 0.0, 6.0, 9.0))
    );
}

#[test]
fn union_holds_both_rectangles() {
    let union = rect(0.0, 0.0, 10.0, 10.0).union(rect(20.0, -5.0, 5.0, 5.0));
    assert_eq!(union, rect(0.0, -5.0, 25.0, 15.0));
}

#[test]
fn intersection_is_the_shared_part_or_none() {
    let a = rect(0.0, 0.0, 10.0, 10.0);
    assert_eq!(a.intersection(rect(5.0, 5.0, 10.0, 10.0)), Some(rect(5.0, 5.0, 5.0, 5.0)));
    assert_eq!(a.intersection(rect(10.0, 0.0, 5.0, 5.0)), Some(rect(10.0, 0.0, 0.0, 5.0)));
    assert_eq!(a.intersection(rect(11.0, 0.0, 5.0, 5.0)), None);
    assert_eq!(a.intersection(rect(0.0, 11.0, 5.0, 5.0)), None);
}

#[test]
fn outset_grows_every_side() {
    assert_eq!(rect(0.0, 0.0, 10.0, 4.0).outset(2.0), rect(-2.0, -2.0, 14.0, 8.0));
}

#[test]
fn the_default_viewport_is_the_identity() {
    let view = Viewport::default();
    assert_eq!(view.zoom(), 1.0);
    assert_eq!(view.to_screen(pt(12.0, -7.0)), pt(12.0, -7.0));
    assert_eq!(view.visible(SCREEN), rect(0.0, 0.0, 800.0, 600.0));
}

#[test]
fn a_new_view_starts_with_the_canvas_origin_in_from_the_panes_corner() {
    let view = Viewport::start();
    assert_eq!(view.zoom(), 1.0);
    const { assert!(CANVAS_MARGIN > 0.0) };
    assert_eq!(view.to_screen(pt(0.0, 0.0)), pt(CANVAS_MARGIN, CANVAS_MARGIN));
    assert_eq!(view.to_screen(pt(12.0, -7.0)), pt(12.0 + CANVAS_MARGIN, -7.0 + CANVAS_MARGIN));
}

#[test]
fn screen_to_canvas_and_back_is_the_identity() {
    for view in [Viewport::default(), moved(), moved().zoom_about(pt(0.0, 0.0), 0.01)] {
        for p in [pt(0.0, 0.0), pt(799.0, 1.0), pt(-50.5, 1234.25)] {
            assert!(near(view.to_screen(view.to_canvas(p)), p), "{view:?} {p:?}");
            assert!(near(view.to_canvas(view.to_screen(p)), p), "{view:?} {p:?}");
        }
    }
}

#[test]
fn a_canvas_rectangle_on_screen_has_its_corners_where_the_points_go() {
    let view = moved();
    let canvas = rect(10.0, 20.0, 30.0, 40.0);
    let screen = view.rect_to_screen(canvas);
    assert!(near(pt(screen.x, screen.y), view.to_screen(pt(10.0, 20.0))));
    let far = view.to_screen(pt(40.0, 60.0));
    assert!(near(pt(screen.x + screen.width, screen.y + screen.height), far));
}

#[test]
fn pan_moves_the_canvas_with_the_pointer() {
    let view = moved();
    let under = view.to_canvas(pt(100.0, 100.0));
    let panned = view.pan(30.0, -20.0);
    assert!(near(panned.to_screen(under), pt(130.0, 80.0)));
    assert_eq!(panned.zoom(), view.zoom());
}

#[test]
fn a_pan_that_is_not_finite_moves_nothing() {
    let view = moved();
    assert_eq!(view.pan(f64::NAN, 0.0), view);
    assert_eq!(view.pan(0.0, f64::INFINITY), view);
}

#[test]
fn the_canvas_has_no_edge() {
    let far = Viewport::default().pan(-1e12, 1e12);
    assert_eq!(far.visible(SCREEN), rect(1e12, -1e12, 800.0, 600.0));
    let p = pt(1e12 + 5.0, -1e12 + 7.0);
    assert!(near(far.to_canvas(far.to_screen(p)), p));
}

#[test]
fn zoom_about_a_point_keeps_that_point_still() {
    let view = moved();
    for (about, factor) in [(pt(250.0, 125.0), 1.7), (pt(0.0, 600.0), 0.4), (pt(9.0, 9.0), 1e9)] {
        let under = view.to_canvas(about);
        let zoomed = view.zoom_about(about, factor);
        assert!(near(zoomed.to_screen(under), about), "{about:?} x{factor}");
    }
}

#[test]
fn zoom_changes_what_is_visible() {
    let view = Viewport::default().zoom_about(pt(0.0, 0.0), 2.0);
    assert_eq!(view.zoom(), 2.0);
    assert_eq!(view.visible(SCREEN), rect(0.0, 0.0, 400.0, 300.0));
}

#[test]
fn zoom_has_limits() {
    let view = moved();
    let about = pt(10.0, 10.0);
    assert_eq!(view.zoom_about(about, 1e9).zoom(), MAX_ZOOM);
    assert_eq!(view.zoom_about(about, 1e-9).zoom(), MIN_ZOOM);
    assert_eq!(view.zoom_about(about, 0.0).zoom(), MIN_ZOOM);
    assert_eq!(view.zoom_about(about, -3.0).zoom(), MIN_ZOOM);
    assert_eq!(view.zoom_about(about, f64::INFINITY).zoom(), MAX_ZOOM);
}

#[test]
fn a_zoom_that_is_not_a_number_or_about_no_point_changes_nothing() {
    let view = moved();
    assert_eq!(view.zoom_about(pt(10.0, 10.0), f64::NAN), view);
    assert_eq!(view.zoom_about(pt(f64::NAN, 10.0), 2.0), view);
    assert_eq!(view.zoom_about(pt(10.0, f64::NEG_INFINITY), 2.0), view);
}

#[test]
fn zoom_to_sets_the_zoom_about_a_point_and_says_it() {
    let view = moved().zoom_about(pt(0.0, 0.0), 1.37);
    let about = pt(300.0, 200.0);
    let back = view.zoom_to(about, 1.0);
    assert_eq!(back.zoom(), 1.0, "exactly, not by a factor");
    assert!(near(back.to_screen(view.to_canvas(about)), about));
    assert_eq!(view.zoom_to(about, 99.0).zoom(), MAX_ZOOM);
    assert_eq!(view.zoom_to(about, f64::NAN), view);
    assert_eq!(back.zoom_label(), "Zoom 100%");
    assert_eq!(back.zoom_to(about, 1.505).zoom_label(), "Zoom 151%");
    assert_eq!(back.zoom_to(about, 0.0).zoom_label(), "Zoom 10%");
}

#[test]
fn a_scroll_away_zooms_in_and_one_back_zooms_out_as_much() {
    assert!(scroll_zoom(16.0) > 1.1 && scroll_zoom(16.0) < 1.25, "a wheel's line");
    assert!((scroll_zoom(16.0) * scroll_zoom(-16.0) - 1.0).abs() < 1e-12);
    assert_eq!(scroll_zoom(0.0), 1.0);
    assert_eq!(scroll_zoom(f64::NAN), 1.0);
    assert_eq!(scroll_zoom(f64::INFINITY), 1.0);
}

#[test]
fn lines_sit_inside_their_area() {
    let rects = lines(rect(100.0, 50.0, 400.0, 200.0), 1);
    assert_eq!(rects, [rect(124.0, 139.0, 352.0, 22.0)]);
}

#[test]
fn the_window_fits_two_lines_inside_its_margins() {
    let rects = lines(WINDOW.rect(), 2);
    assert!(rects[0].y >= MARGIN);
    assert!(rects[1].y + rects[1].height <= WINDOW.height - MARGIN);
}

#[test]
fn a_popup_is_centred_on_the_pen_with_its_cells_in_a_row() {
    let pop = popup(pt(200.0, 100.0), 3, SWATCH, SCREEN.rect());
    // 3 cells of 24 and 4 pads of 6 wide; 24 and 2 pads high.
    assert_eq!(pop.panel, rect(200.0 - 48.0, 100.0 - 18.0, 96.0, 36.0));
    let xs: Vec<f64> = pop.cells.iter().map(|c| c.x).collect();
    assert_eq!(xs, [158.0, 188.0, 218.0]);
    for cell in &pop.cells {
        assert_eq!((cell.y, cell.width, cell.height), (88.0, 24.0, 24.0));
        assert!(pop.panel.contains(pt(cell.x, cell.y)));
        assert!(pop.panel.contains(pt(cell.x + cell.width, cell.y + cell.height)));
    }
}

#[test]
fn a_popup_near_an_edge_moves_inside_it() {
    let size = |p: &Popup| (p.panel.width, p.panel.height);
    let top_left = popup(pt(3.0, 2.0), 4, WORD, SCREEN.rect());
    assert_eq!((top_left.panel.x, top_left.panel.y), (8.0, 8.0));
    let bottom_right = popup(pt(799.0, 599.0), 4, WORD, SCREEN.rect());
    let (w, h) = size(&bottom_right);
    assert_eq!((bottom_right.panel.x, bottom_right.panel.y), (800.0 - 8.0 - w, 600.0 - 8.0 - h));
    let off_screen = popup(pt(-500.0, 9000.0), 4, WORD, SCREEN.rect());
    assert_eq!((off_screen.panel.x, off_screen.panel.y), (8.0, 600.0 - 8.0 - h));
    assert_eq!(off_screen.cells[0].x, 8.0 + 6.0);
}

#[test]
fn a_popup_stays_inside_a_pane_away_from_the_origin() {
    let pane = rect(400.0, 300.0, 400.0, 300.0);
    let pop = popup(pt(401.0, 599.0), 4, WORD, pane);
    let h = pop.panel.height;
    assert_eq!((pop.panel.x, pop.panel.y), (408.0, 600.0 - 8.0 - h));
}

#[test]
fn a_popup_too_big_for_its_space_keeps_its_top_left_in() {
    let pop = popup(pt(20.0, 20.0), 9, WORD, rect(0.0, 0.0, 100.0, 30.0));
    assert_eq!((pop.panel.x, pop.panel.y), (8.0, 8.0));
    assert!(pop.panel.width > 100.0);
}

#[test]
fn a_popup_of_no_cells_is_an_empty_panel() {
    let pop = popup(pt(100.0, 100.0), 0, SWATCH, SCREEN.rect());
    assert_eq!(pop.cells, []);
    assert_eq!(pop.panel, rect(97.0, 82.0, 6.0, 36.0));
}

#[test]
fn contains_includes_the_edges() {
    let r = rect(10.0, 10.0, 5.0, 5.0);
    assert!(r.contains(pt(10.0, 10.0)) && r.contains(pt(15.0, 15.0)));
    assert!(!r.contains(pt(9.9, 12.0)) && !r.contains(pt(12.0, 15.1)));
}

#[test]
fn the_foot_line_sits_at_the_bottom_inside_the_margins_or_at_the_top_of_a_short_area() {
    let area = Rect { x: 10.0, y: 20.0, width: 300.0, height: 200.0 };
    assert_eq!(foot(area), Rect { x: 34.0, y: 192.0, width: 252.0, height: 22.0 });
    let short = Rect { x: 0.0, y: 5.0, width: 10.0, height: 10.0 };
    assert_eq!(foot(short), Rect { x: 24.0, y: 5.0, width: 0.0, height: 22.0 });
}
