//! Times as RFC 3339 text: written as UTC with milliseconds
//! (`2026-11-10T16:41:46.982Z`), read with any offset and any number of
//! fraction digits (past the third, they are dropped).

use crate::canvas::Time;

const DAY_MS: i64 = 86_400_000;

/// Days since 1970-01-01 of a civil date (H. Hinnant's `days_from_civil`).
fn days(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let (era, yoe) = (y.div_euclid(400), y.rem_euclid(400));
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468
}

/// The civil date of a day since 1970-01-01 (H. Hinnant's `civil_from_days`).
fn civil(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let (era, doe) = (z.div_euclid(146_097), z.rem_euclid(146_097));
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { yoe + era * 400 + 1 } else { yoe + era * 400 }, m, d)
}

pub(super) fn format(time: Time) -> String {
    let ms = i64::try_from(time.0).unwrap_or(i64::MAX);
    let (day, rest) = (ms / DAY_MS, ms % DAY_MS);
    let (y, m, d) = civil(day);
    let (hour, min, sec, frac) =
        (rest / 3_600_000, rest / 60_000 % 60, rest / 1000 % 60, rest % 1000);
    format!("{y:04}-{m:02}-{d:02}T{hour:02}:{min:02}:{sec:02}.{frac:03}Z")
}

pub(super) fn parse(text: &str) -> Result<Time, String> {
    let bad = || format!("{text:?} is not an RFC 3339 time");
    let b = text.as_bytes();
    let num = |from: usize, len: usize| -> Option<i64> {
        let digits = b.get(from..from + len)?;
        digits
            .iter()
            .all(u8::is_ascii_digit)
            .then(|| digits.iter().fold(0, |acc, d| acc * 10 + i64::from(d - b'0')))
    };
    let sep = |at: usize, set: &[u8]| b.get(at).is_some_and(|c| set.contains(c));
    let ok = sep(4, b"-") && sep(7, b"-") && sep(10, b"Tt") && sep(13, b":") && sep(16, b":");
    let (Some(year), Some(mo), Some(day), Some(hour), Some(mi), Some(sec)) =
        (num(0, 4), num(5, 2), num(8, 2), num(11, 2), num(14, 2), num(17, 2))
    else {
        return Err(bad());
    };
    let in_month =
        (1..=12).contains(&mo) && day >= 1 && day <= days(year, mo + 1, 1) - days(year, mo, 1);
    if !ok || !in_month || hour > 23 || mi > 59 || sec > 60 {
        return Err(bad());
    }
    let mut at = 19;
    let mut frac = 0;
    if sep(at, b".") {
        let digits = b[at + 1..].iter().take_while(|c| c.is_ascii_digit()).count();
        if digits == 0 {
            return Err(bad());
        }
        let first3 = num(at + 1, digits.min(3)).ok_or_else(bad)?;
        frac = first3 * [100, 10, 1][digits.min(3) - 1];
        at += 1 + digits;
    }
    let offset = match b.get(at..) {
        Some(b"Z" | b"z") => 0,
        Some([sign @ (b'+' | b'-'), _, _, b':', _, _]) => {
            let (Some(oh), Some(om)) = (num(at + 1, 2), num(at + 4, 2)) else { return Err(bad()) };
            let o = (oh * 60 + om) * 60_000;
            if *sign == b'+' { o } else { -o }
        }
        _ => return Err(bad()),
    };
    let ms = days(year, mo, day) * DAY_MS + ((hour * 60 + mi) * 60 + sec) * 1000 + frac - offset;
    u64::try_from(ms).map(Time).map_err(|_| format!("{text:?} is before 1970"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_utc_with_milliseconds() {
        assert_eq!(format(Time(0)), "1970-01-01T00:00:00.000Z");
        assert_eq!(format(Time(1_789_058_506_982)), "2026-09-10T16:41:46.982Z");
        assert_eq!(format(Time(951_782_400_000)), "2000-02-29T00:00:00.000Z");
    }

    #[test]
    fn reads_what_it_writes() {
        for ms in [0, 1, 999, 951_782_400_000, 1_789_058_506_982, 253_402_300_799_999] {
            assert_eq!(parse(&format(Time(ms))), Ok(Time(ms)), "{ms}");
        }
    }

    #[test]
    fn reads_offsets_and_any_fraction() {
        let t = parse("2026-11-10T16:41:46.982Z").unwrap();
        assert_eq!(parse("2026-11-10T17:41:46.982+01:00"), Ok(t));
        assert_eq!(parse("2026-11-10T15:41:46.982-01:00"), Ok(t));
        assert_eq!(parse("2026-11-10t16:41:46.982123z"), Ok(t));
        assert_eq!(parse("2026-11-10T16:41:46.9Z"), Ok(Time(t.0 - 82)));
        assert_eq!(parse("2026-11-10T16:41:46Z"), Ok(Time(t.0 - 982)));
    }

    #[test]
    fn rejects_what_is_not_a_time() {
        for s in [
            "",
            "2026-11-10",
            "2026-11-10T16:41:46.982",
            "2026-13-10T16:41:46Z",
            "2026-02-29T16:41:46Z",
            "2026-11-10T24:00:00Z",
            "2026-11-10T16:41:46.Z",
            "2026-11-10T16:41:46+0100",
            "1969-12-31T23:59:59Z",
            "2026/11/10T16:41:46Z",
            "2026-11-10T16:41:46.982Zjunk",
        ] {
            assert!(parse(s).is_err(), "{s}");
        }
    }
}
