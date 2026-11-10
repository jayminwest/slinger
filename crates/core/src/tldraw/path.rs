//! The points of one segment of a tldraw draw shape, in the shape's own space.
//!
//! Newer files pack the points into a base64 string (`path`): the first point
//! as little-endian 32-bit floats, then each later point as the difference
//! from the one before in 16-bit floats. `dim` (2 or 3, default 3) says how
//! many numbers a point has; the third is pen pressure, which Slinger drops
//! (D6). Older files list the points as objects (`points`).

use serde_json::Value;

use crate::canvas::Point;

/// The segment's points, or `None` when it has none or they cannot be read.
pub(super) fn points(segment: &Value) -> Option<Vec<Point>> {
    if let Some(path) = segment.get("path").and_then(Value::as_str) {
        let dim = segment.get("dim").and_then(Value::as_u64).unwrap_or(3);
        return packed(&base64(path)?, usize::try_from(dim).ok().filter(|d| (2..=3).contains(d))?);
    }
    let points = segment.get("points")?.as_array()?;
    points.iter().map(|p| Some(Point::new(p.get("x")?.as_f64()?, p.get("y")?.as_f64()?))).collect()
}

fn packed(bytes: &[u8], dim: usize) -> Option<Vec<Point>> {
    let first = bytes.get(..4 * dim)?;
    let rest = &bytes[4 * dim..];
    if !rest.len().is_multiple_of(2 * dim) {
        return None;
    }
    let f32_at = |i: usize| {
        f64::from(f32::from_le_bytes([first[i], first[i + 1], first[i + 2], first[i + 3]]))
    };
    let mut at = Point::new(f32_at(0), f32_at(4));
    let mut out = vec![at];
    for delta in rest.chunks_exact(2 * dim) {
        at = Point::new(at.x + f16(delta[0], delta[1]), at.y + f16(delta[2], delta[3]));
        out.push(at);
    }
    Some(out)
}

/// A little-endian IEEE 754 half-precision float.
fn f16(lo: u8, hi: u8) -> f64 {
    let bits = u16::from_le_bytes([lo, hi]);
    let sign = if bits & 0x8000 == 0 { 1.0 } else { -1.0 };
    let exp = i32::from((bits >> 10) & 0x1F);
    let frac = f64::from(bits & 0x3FF);
    sign * match exp {
        0 => frac * 2f64.powi(-24),
        31 if frac == 0.0 => f64::INFINITY,
        31 => f64::NAN,
        _ => (1.0 + frac / 1024.0) * 2f64.powi(exp - 15),
    }
}

/// Standard base64, padding optional.
fn base64(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in text.bytes().filter(|&c| c != b'=') {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(u8::try_from((acc >> bits) & 0xFF).ok()?);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn decodes_base64_with_and_without_padding() {
        assert_eq!(base64("aGk=").unwrap(), b"hi");
        assert_eq!(base64("aGk").unwrap(), b"hi");
        assert_eq!(base64("+/+/").unwrap(), [0xFB, 0xFF, 0xBF]);
        assert!(base64("a*").is_none());
    }

    #[test]
    fn decodes_half_floats() {
        assert!((f16(0x00, 0x3C) - 1.0).abs() < f64::EPSILON);
        assert!((f16(0x00, 0xC0) + 2.0).abs() < f64::EPSILON);
        assert!((f16(0x01, 0x00) - 2f64.powi(-24)).abs() < f64::EPSILON);
        assert!(f16(0x00, 0x7C).is_infinite());
        assert!(f16(0x01, 0x7C).is_nan());
    }

    #[test]
    fn a_packed_path_is_a_start_then_differences() {
        // From the real board: a pen stroke (three numbers a point) and an
        // agent's stroke (two numbers a point).
        let pen = points(&json!({"path": "AAAAAAAAAAAK1yM9uMF4xmYy"})).unwrap();
        assert_eq!(pen.len(), 2);
        assert_eq!(pen[0], Point::new(0.0, 0.0));
        assert!((pen[1].x + 2.859).abs() < 0.001 && (pen[1].y + 6.469).abs() < 0.001);
        let flat = points(&json!({"path": "AAAAAAAAAAApPVLB", "dim": 2})).unwrap();
        assert_eq!(flat.len(), 2);
        assert!((flat[1].x - 1.290).abs() < 0.001 && (flat[1].y + 2.660).abs() < 0.001);
    }

    #[test]
    fn listed_points_are_read_as_they_are() {
        let s = json!({"points": [{"x": 1, "y": 2, "z": 0.5}, {"x": 3.5, "y": -4}]});
        assert_eq!(points(&s).unwrap(), [Point::new(1.0, 2.0), Point::new(3.5, -4.0)]);
    }

    #[test]
    fn an_unreadable_segment_has_no_points() {
        assert!(points(&json!({"path": "AAAA"})).is_none());
        assert!(points(&json!({"path": "AAAAAAAAAAAAAA", "dim": 3})).is_none());
        assert!(points(&json!({"path": "AAAAAAAAAAAK1yM9", "dim": 4})).is_none());
        assert!(points(&json!({"points": [{"x": 1}]})).is_none());
        assert!(points(&json!({})).is_none());
    }
}
