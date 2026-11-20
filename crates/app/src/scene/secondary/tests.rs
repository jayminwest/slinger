//! A right or middle drag past the slop pans; a right click pans nothing;
//! the middle button never clicks.

use super::*;

fn p(x: f64, y: f64) -> Point {
    Point { x, y }
}

#[test]
fn a_lift_inside_the_slop_is_a_click_at_the_press_point() {
    let mut s = Secondary::default();
    s.down(Button::Secondary, p(10.0, 10.0));
    assert_eq!(s.drag(p(12.0, 11.0)), None, "jitter pans nothing");
    assert_eq!(s.drag(p(13.0, 12.0)), None);
    assert_eq!(s.up(p(12.0, 12.0)), Some(p(10.0, 10.0)));
    assert_eq!(s.up(p(12.0, 12.0)), None, "one click per press");
}

#[test]
fn a_drag_past_the_slop_pans_the_whole_way_and_its_lift_is_no_click() {
    let mut s = Secondary::default();
    s.down(Button::Secondary, p(10.0, 10.0));
    assert_eq!(s.drag(p(12.0, 10.0)), None);
    assert_eq!(s.drag(p(20.0, 13.0)), Some((p(10.0, 10.0), 10.0, 3.0)), "all of it at once");
    assert_eq!(s.drag(p(21.0, 11.0)), Some((p(10.0, 10.0), 1.0, -2.0)), "then since the last");
    assert_eq!(s.up(p(21.0, 11.0)), None);
}

#[test]
fn coming_back_inside_the_slop_after_leaving_it_is_still_a_pan() {
    let mut s = Secondary::default();
    s.down(Button::Secondary, p(0.0, 0.0));
    assert!(s.drag(p(10.0, 0.0)).is_some());
    assert_eq!(s.drag(p(1.0, 0.0)), Some((p(0.0, 0.0), -9.0, 0.0)));
    assert_eq!(s.up(p(0.0, 0.0)), None, "no click");
}

#[test]
fn a_lift_past_the_slop_with_no_drag_is_no_click() {
    let mut s = Secondary::default();
    s.down(Button::Secondary, p(0.0, 0.0));
    assert_eq!(s.up(p(30.0, 0.0)), None);
}

#[test]
fn the_middle_button_pans_and_never_clicks() {
    let mut s = Secondary::default();
    s.down(Button::Middle, p(5.0, 5.0));
    assert_eq!(s.up(p(5.0, 5.0)), None);
    s.down(Button::Middle, p(5.0, 5.0));
    assert_eq!(s.drag(p(5.0, 15.0)), Some((p(5.0, 5.0), 0.0, 10.0)));
    assert_eq!(s.up(p(5.0, 15.0)), None);
}

#[test]
fn an_up_or_a_drag_with_no_down_is_nothing() {
    let mut s = Secondary::default();
    assert_eq!(s.drag(p(50.0, 50.0)), None);
    assert_eq!(s.up(p(50.0, 50.0)), None);
    s.down(Button::Secondary, p(0.0, 0.0));
    s.drop_press();
    assert_eq!(s.drag(p(50.0, 50.0)), None, "a dropped press is gone");
    assert_eq!(s.up(p(0.0, 0.0)), None);
}

#[test]
fn a_second_down_replaces_the_first() {
    let mut s = Secondary::default();
    s.down(Button::Secondary, p(0.0, 0.0));
    assert!(s.drag(p(40.0, 0.0)).is_some(), "panning, then its lift is lost");
    s.down(Button::Secondary, p(100.0, 100.0));
    assert_eq!(s.drag(p(101.0, 100.0)), None, "a new press: in its slop");
    assert_eq!(s.up(p(101.0, 100.0)), Some(p(100.0, 100.0)));
}
