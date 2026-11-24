use super::writing;
use crate::layout::{Point, Size};

#[test]
fn the_column_and_four_cells_each_side_fill_the_width_its_top_a_row_down() {
    let (at, size) = (Point { x: 100.0, y: 200.0 }, 20.0);
    let screen = Size { width: 880.0, height: 600.0 };
    let view = writing(at, size, screen);
    // 88 cells of 10 points fill 880 points at 100%.
    assert!((view.zoom() - 1.0).abs() < 1e-9, "{view:?}");
    let left = view.to_screen(Point { x: 100.0 - 40.0, y: 200.0 });
    assert!(left.x.abs() < 1e-9 && (left.y - 20.0).abs() < 1e-9, "{left:?}");
    let right = view.to_screen(Point { x: 100.0 + 800.0 + 40.0, y: 200.0 });
    assert!((right.x - 880.0).abs() < 1e-9, "{right:?}");

    let wide = writing(at, size, Size { width: 1760.0, height: 600.0 });
    assert!((wide.zoom() - 2.0).abs() < 1e-9);
    assert!((wide.to_screen(at).y - 40.0).abs() < 1e-9, "a row of the zoomed size");
}

#[test]
fn a_zoom_past_the_limit_is_held_and_the_column_stays_centred() {
    let (at, size) = (Point { x: 0.0, y: 0.0 }, 0.5);
    let screen = Size { width: 2000.0, height: 600.0 };
    let view = writing(at, size, screen);
    assert!((view.zoom() - crate::layout::MAX_ZOOM).abs() < 1e-9);
    let middle = view.to_screen(Point { x: 80.0 * 0.25 / 2.0, y: 0.0 });
    assert!((middle.x - 1000.0).abs() < 1e-9, "{middle:?}");
}
