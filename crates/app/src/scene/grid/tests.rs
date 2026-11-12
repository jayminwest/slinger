use super::*;
use crate::config::canvas::Background;
use crate::layout::Size;
use crate::scene::canvas;

const CM: f64 = 28.35;

fn grid(spacing: f64) -> Grid {
    Grid::new(spacing, &Theme::default())
}

fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect { x, y, width, height }
}

fn dots(g: Grid, view: Viewport, area: Rect) -> Vec<(f64, f64)> {
    g.dots(view, area).map(|p| (p.x, p.y)).collect()
}

/// A viewport at `zoom` about the screen's top left, the canvas origin there.
fn zoomed(zoom: f64) -> Viewport {
    Viewport::default().zoom_to(Point { x: 0.0, y: 0.0 }, zoom)
}

#[test]
fn every_dot_shows_at_100_percent_one_spacing_apart_on_the_canvas() {
    let g = grid(10.0);
    let all = dots(g, Viewport::default(), rect(0.0, 0.0, 25.0, 15.0));
    let expected: Vec<_> =
        [0.0, 10.0].iter().flat_map(|&y| [0.0, 10.0, 20.0].map(|x| (x, y))).collect();
    assert_eq!(all, expected, "row by row, each dot whose ink touches the box");
    assert_eq!(g.step(1.0), 1);
}

#[test]
fn dots_are_fixed_to_the_canvas_so_they_pan_and_zoom_with_the_ink() {
    let g = grid(CM);
    let view = Viewport::start().pan(5.0, -7.0).zoom_about(Point { x: 40.0, y: 30.0 }, 1.5);
    let screen = rect(0.0, 0.0, 400.0, 300.0);
    let shown = dots(g, view, screen);
    assert_ne!(shown, []);
    for (x, y) in shown {
        let at = view.to_canvas(Point { x, y });
        let off = |v: f64| ((v / CM) - (v / CM).round()).abs();
        assert!(off(at.x) < 1e-9 && off(at.y) < 1e-9, "a dot on the canvas grid: {at:?}");
    }
}

#[test]
fn a_repaint_draws_only_the_dots_inside_its_box() {
    let g = grid(CM);
    let view = Viewport::start();
    let small = rect(100.0, 100.0, 20.0, 20.0);
    let few = dots(g, view, small);
    assert!(few.len() <= 4, "a stroke's small box holds a few dots: {few:?}");
    for (x, y) in &few {
        let touches = |p: f64, lo: f64, len: f64| p >= lo - RADIUS && p <= lo + len + RADIUS;
        assert!(touches(*x, small.x, small.width) && touches(*y, small.y, small.height));
    }
    let whole = dots(g, view, rect(0.0, 0.0, 800.0, 600.0));
    assert!(few.iter().all(|d| whole.contains(d)), "the same dots as a whole repaint");
    assert!(dots(g, view, rect(0.0, 0.0, 0.0, 0.0)).len() <= 1);
}

#[test]
fn a_dot_just_outside_the_box_whose_ink_reaches_in_is_drawn() {
    let g = grid(10.0);
    let edge = dots(g, Viewport::default(), rect(10.5, 10.5, 5.0, 5.0));
    assert_eq!(edge, [(10.0, 10.0)], "its 1-point radius reaches into the box");
    assert_eq!(dots(g, Viewport::default(), rect(11.5, 11.5, 5.0, 5.0)), []);
}

#[test]
fn zoomed_out_the_dots_thin_to_every_2nd_5th_or_10th() {
    let g = grid(CM);
    assert_eq!(g.step(1.0), 1);
    assert_eq!(g.step(0.29), 1, "28.35 x 0.29 = 8.2 points apart");
    assert_eq!(g.step(0.25), 2);
    assert_eq!(g.step(0.12), 5);
    assert_eq!(grid(4.0).step(0.1), 20);
    assert_eq!(grid(1.0).step(0.1), 100);
    for zoom in [0.1, 0.13, 0.2, 0.3, 0.5, 1.0, 2.0, 8.0] {
        for spacing in [1.0, 5.0, CM, 100.0] {
            let g = grid(spacing);
            let k = f64::from(g.step(zoom));
            assert!(spacing * k * zoom >= 8.0, "{spacing} at {zoom}");
            let shown = dots(g, zoomed(zoom), rect(0.0, 0.0, 2000.0, 10.0));
            let gap = shown[1].0 - shown[0].0;
            assert!((gap - spacing * k * zoom).abs() < 1e-6 && gap >= 8.0 - 1e-9);
        }
    }
}

#[test]
fn the_dots_that_stay_when_zoomed_out_are_dots_of_the_full_grid() {
    let g = grid(CM);
    let view = zoomed(0.15);
    let pitch = CM * f64::from(g.step(0.15));
    for (x, y) in dots(g, view, rect(0.0, 0.0, 500.0, 500.0)) {
        let at = view.to_canvas(Point { x, y });
        assert!(((at.x / pitch) - (at.x / pitch).round()).abs() < 1e-9, "counted from the origin");
    }
}

#[test]
fn a_spacing_that_is_not_a_positive_number_shows_nothing() {
    for spacing in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert_eq!(dots(grid(spacing), Viewport::default(), rect(0.0, 0.0, 50.0, 50.0)), []);
    }
}

#[test]
fn the_dot_colour_is_a_quiet_mix_of_the_background_and_muted() {
    let theme = Theme::default();
    let c = grid(CM).colour;
    let between = |v: u8, a: u8, b: u8| (a.min(b)..=a.max(b)).contains(&v);
    assert!(between(c.g, theme.background.g, theme.muted.g));
    assert_ne!(c, theme.background);
    let half = u8::midpoint(theme.background.g, theme.muted.g);
    assert!(c.g < half, "nearer the paper than the muted colour");
}

#[test]
fn the_config_sets_the_paper_at_start_and_the_toggle_flips_it() {
    let theme = Theme::default();
    let dots = Canvas { background: Background::Dots, ..Canvas::default() };
    let mut paper = Paper::default();
    paper.follow(Canvas::default());
    assert_eq!(paper.grid(&theme), None, "plain by default");
    assert_eq!(paper.toggle(), "Dotted grid on");
    let shown = paper.grid(&theme).unwrap();
    assert!((shown.spacing - CM).abs() < 1e-9, "one centimetre at 100%");
    paper.follow(Canvas::default());
    assert!(paper.grid(&theme).is_some(), "the same config keeps the toggle");
    paper.follow(dots);
    assert!(paper.grid(&theme).is_some());
    assert_eq!(paper.toggle(), "Dotted grid off");
    let wider = Canvas { grid: crate::config::canvas::Grid::default(), ..dots };
    paper.follow(wider);
    assert_eq!(paper.grid(&theme), None, "a config that keeps the background keeps the toggle");
    paper.follow(Canvas::default());
    paper.follow(dots);
    assert!(paper.grid(&theme).is_some(), "a new background sets it again");
}

#[test]
fn a_new_paper_repaints_the_whole_pane_and_the_same_one_does_not() {
    let screen = Size { width: 300.0, height: 200.0 };
    let theme = Theme::default();
    let build =
        |last: Option<&Scene>| canvas::build(&[], Viewport::default(), screen, &theme, last);
    let plain = build(None).on(None, None);
    assert_eq!(plain.grid, None);
    let same = build(Some(&plain)).on(None, Some(&plain));
    assert_eq!(same.changed, None);
    let g = Some(grid(CM));
    let dotted = build(Some(&plain)).on(g, Some(&plain));
    assert_eq!(dotted.changed, Some(screen.rect()));
    assert_eq!(dotted.grid, g);
    assert_eq!(build(Some(&dotted)).on(g, Some(&dotted)).changed, None);
}
