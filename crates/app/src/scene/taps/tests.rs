use super::*;

const T: Theme = Theme {
    background: Rgb { r: 0, g: 0, b: 0 },
    text: Rgb { r: 255, g: 255, b: 255 },
    muted: Rgb { r: 2, g: 2, b: 2 },
    error: Rgb { r: 3, g: 3, b: 3 },
    focus: Rgb { r: 4, g: 4, b: 4 },
};
const UNDO: Rect = Rect { x: 10.0, y: 572.0, width: 28.0, height: 28.0 };
const REDO: Rect = Rect { x: 38.0, y: 572.0, width: 28.0, height: 28.0 };

fn at(x: f64, y: f64) -> Point {
    Point { x, y }
}

fn two() -> Taps {
    let mut taps = Taps::default();
    taps.set(vec![(UNDO, Tap::Run(Command::Undo)), (REDO, Tap::Run(Command::Redo))]);
    taps
}

#[test]
fn a_tap_runs_on_the_lift_on_the_target_it_went_down_on() {
    let mut taps = two();
    assert!(taps.press(at(20.0, 580.0)), "the press is the target's: no ink");
    assert!(taps.drag(at(22.0, 581.0)), "so is the drag");
    assert_eq!(taps.lift(at(24.0, 582.0)), Took::Tap((UNDO, Tap::Run(Command::Undo))));
    assert_eq!(taps.lift(at(24.0, 582.0)), Took::No, "one lift, one tap");
}

#[test]
fn a_drag_off_the_target_cancels_and_the_lift_is_used_up() {
    let mut taps = two();
    assert!(taps.press(at(20.0, 580.0)));
    assert!(taps.drag(at(200.0, 300.0)));
    assert_eq!(taps.lit(&T), None, "off the held target nothing lights");
    assert_eq!(taps.lift(at(200.0, 300.0)), Took::Yes, "nothing runs, nothing draws");
    assert!(taps.press(at(20.0, 580.0)));
    assert_eq!(taps.lift(at(50.0, 580.0)), Took::Yes, "a lift on the next target runs neither");
}

#[test]
fn a_press_away_from_every_target_is_not_theirs() {
    let mut taps = two();
    assert!(!taps.press(at(300.0, 300.0)));
    assert!(!taps.drag(at(310.0, 300.0)), "the canvas has the stroke");
    assert_eq!(taps.lift(at(20.0, 580.0)), Took::No, "even lifted over a target");
}

#[test]
fn the_target_under_the_pen_lights_and_more_while_pressed() {
    let mut taps = two();
    assert_eq!(taps.lit(&T), None);
    assert!(taps.hover(at(20.0, 580.0)), "onto undo: draw again");
    assert_eq!(taps.lit(&T), Some((UNDO, style::hover(&T))));
    assert!(!taps.hover(at(21.0, 580.0)), "still undo: nothing to draw");
    assert!(taps.hover(at(40.0, 580.0)), "onto redo");
    assert!(taps.press(at(40.0, 580.0)));
    assert_eq!(taps.lit(&T), Some((REDO, style::selected(&T))));
    assert!(taps.hover(at(300.0, 300.0)), "off every target");
    assert_eq!(taps.lift(at(40.0, 580.0)), Took::Tap((REDO, Tap::Run(Command::Redo))));
}

#[test]
fn a_pop_up_s_answer_is_a_target_s() {
    assert_eq!(Took::from(Taken::No), Took::No);
    assert_eq!(Took::from(Taken::Yes), Took::Yes);
    let Took::Tap((_, tap)) = Took::from(Taken::Run(Command::ZoomIn)) else { panic!() };
    assert_eq!(tap, Tap::Run(Command::ZoomIn));
}
