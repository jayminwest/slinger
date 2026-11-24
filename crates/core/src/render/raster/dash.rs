//! A dashed or dotted line (docs/design/figures.md, "Draw order"): a line
//! cut into the runs that are drawn. A dash starts at a line's first point,
//! so a box's at its top left corner.

use crate::canvas::{Dash, Point};

/// A dash is this many line widths long, and the gap after it this many.
/// At a box's width of 2 units, 8 on and 6 off.
const DASH_ON: f64 = 4.0;
const DASH_OFF: f64 = 3.0;

/// A dot, as round as the line is wide, every this many line widths: at a
/// box's width, every 6 units.
const DOT_EVERY: f64 = 3.0;

/// The parts of `line` that are drawn, for a line `width` wide (in the
/// same units as the points). A solid line is itself; a dot is a run of one
/// point.
pub(super) fn cut(line: Vec<Point>, dash: Dash, width: f64) -> Vec<Vec<Point>> {
    let (on, off) = match dash {
        _ if line.len() < 2 => return vec![line],
        Dash::Solid => return vec![line],
        Dash::Dashed => (DASH_ON * width, DASH_OFF * width),
        Dash::Dotted => (0.0, DOT_EVERY * width),
    };
    if on + off <= 0.0 {
        return vec![line];
    }
    let period = on + off;
    let mut runs = Vec::new();
    let mut run: Vec<Point> = Vec::new();
    // How far along the pattern the walk is, from 0 up to one period.
    let mut phase = 0.0;
    for pair in line.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let length = (b.x - a.x).hypot(b.y - a.y);
        let at = |d: f64| {
            let t = if length > 0.0 { d / length } else { 0.0 };
            Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
        };
        let mut done = 0.0;
        while done < length || (length == 0.0 && done == 0.0) {
            if phase < on {
                if run.is_empty() {
                    run.push(at(done));
                }
                let step = (on - phase).min(length - done);
                done += step;
                phase += step;
                run.push(at(done));
                if phase >= on {
                    runs.push(std::mem::take(&mut run));
                }
            } else {
                if on == 0.0 && phase == 0.0 {
                    runs.push(vec![at(done)]);
                }
                let step = (period - phase).min(length - done);
                done += step;
                phase += step;
                if phase >= period {
                    phase = 0.0;
                }
            }
            if length == 0.0 {
                break;
            }
        }
    }
    if on == 0.0
        && phase == 0.0
        && let Some(&end) = line.last()
    {
        runs.push(vec![end]);
    }
    if run.len() > 1 {
        runs.push(run);
    }
    runs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(to: f64) -> Vec<Point> {
        vec![Point::new(0.0, 0.0), Point::new(to, 0.0)]
    }

    #[test]
    fn a_solid_line_is_itself() {
        assert_eq!(cut(line(10.0), Dash::Solid, 2.0), vec![line(10.0)]);
    }

    #[test]
    fn dashes_are_four_widths_on_and_three_off_from_the_first_point() {
        let runs = cut(line(30.0), Dash::Dashed, 2.0);
        let xs: Vec<(f64, f64)> = runs.iter().map(|r| (r[0].x, r[r.len() - 1].x)).collect();
        assert_eq!(xs, vec![(0.0, 8.0), (14.0, 22.0), (28.0, 30.0)]);
    }

    #[test]
    fn a_dash_runs_on_round_a_corner() {
        let corner = vec![Point::new(0.0, 0.0), Point::new(4.0, 0.0), Point::new(4.0, 10.0)];
        let runs = cut(corner, Dash::Dashed, 2.0);
        assert_eq!(runs[0], vec![Point::new(0.0, 0.0), Point::new(4.0, 0.0), Point::new(4.0, 4.0)]);
    }

    #[test]
    fn dots_are_every_three_widths() {
        let runs = cut(line(12.0), Dash::Dotted, 2.0);
        let xs: Vec<f64> = runs.iter().map(|r| r[0].x).collect();
        assert!(runs.iter().all(|r| r.len() == 1));
        assert_eq!(xs, vec![0.0, 6.0, 12.0]);
    }
}
