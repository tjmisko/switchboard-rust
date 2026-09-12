//! RFC 3339 timestamps without a date crate.
//!
//! Provided code, not an exercise. Two conversions are all the daemon needs:
//! reading `timestamp` / `ts` fields into a `SystemTime` for `Moment::claimed`
//! and history, and writing a `SystemTime` back out in UTC.
//!
//! Accepts what the corpus contains: `Z` or a `±HH:MM` offset, and zero to
//! nine fractional digits (Go writes up to nine, Claude three).

use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Days since 1970-01-01 for a proleptic Gregorian date. Howard Hinnant's
/// `days_from_civil`.
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Inverse of `days_from_civil`.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

fn digits(s: &str, range: std::ops::Range<usize>) -> Option<u32> {
    let part = s.get(range)?;
    if !part.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    part.parse().ok()
}

/// Parse `2026-09-11T01:35:09.339Z` or `2026-09-11T00:17:20.557938098-07:00`.
pub fn parse_rfc3339(s: &str) -> Option<SystemTime> {
    let b = s.as_bytes();
    if b.len() < 20 || b[4] != b'-' || b[7] != b'-' || !matches!(b[10], b'T' | b't' | b' ') {
        return None;
    }
    if b[13] != b':' || b[16] != b':' {
        return None;
    }
    let year = digits(s, 0..4)? as i64;
    let month = digits(s, 5..7)?;
    let day = digits(s, 8..10)?;
    let hour = digits(s, 11..13)?;
    let minute = digits(s, 14..16)?;
    let second = digits(s, 17..19)?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || minute > 59 {
        return None;
    }
    if second > 60 {
        return None;
    }

    let mut rest = &s[19..];
    let mut nanos: u32 = 0;
    if let Some(frac) = rest.strip_prefix('.') {
        let len = frac.bytes().take_while(u8::is_ascii_digit).count();
        if len == 0 || len > 9 {
            return None;
        }
        let scale = 10u32.pow(9 - len as u32);
        nanos = frac[..len].parse::<u32>().ok()? * scale;
        rest = &frac[len..];
    }

    let offset_secs: i64 = match rest {
        "Z" | "z" => 0,
        _ => {
            let sign = match rest.as_bytes().first()? {
                b'+' => 1,
                b'-' => -1,
                _ => return None,
            };
            if rest.len() != 6 || rest.as_bytes()[3] != b':' {
                return None;
            }
            let oh = digits(rest, 1..3)? as i64;
            let om = digits(rest, 4..6)? as i64;
            sign * (oh * 3600 + om * 60)
        }
    };

    let days = days_from_civil(year, month, day);
    let local = days * 86_400 + hour as i64 * 3600 + minute as i64 * 60 + second as i64;
    let utc = local - offset_secs;
    let secs = u64::try_from(utc).ok()?;
    UNIX_EPOCH.checked_add(Duration::new(secs, nanos))
}

/// `2026-09-11T08:36:29.148Z`: UTC, millisecond precision.
pub fn format_rfc3339(t: SystemTime) -> String {
    let since = t.duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = since.as_secs() as i64;
    let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
    let sod = secs.rem_euclid(86_400);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{:03}Z",
        sod / 3600,
        sod / 60 % 60,
        sod % 60,
        since.subsec_millis()
    )
}

/// Epoch seconds, as `resetsAt` carries them.
pub fn from_epoch_secs(secs: u64) -> Option<SystemTime> {
    UNIX_EPOCH.checked_add(Duration::from_secs(secs))
}

/// Milliseconds since the epoch, for published JSON.
pub fn epoch_millis(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_agree_on_one_instant_when_written_with_different_offsets() {
        let z = parse_rfc3339("2026-09-11T07:07:35.512Z").unwrap();
        let local = parse_rfc3339("2026-09-11T00:07:35.512-07:00").unwrap();
        assert_eq!(z, local);
    }

    #[test]
    fn should_keep_nanoseconds_when_nine_fractional_digits_are_given() {
        let t = parse_rfc3339("2026-09-11T00:17:20.557938098-07:00").unwrap();
        let nanos = t.duration_since(UNIX_EPOCH).unwrap().subsec_nanos();
        assert_eq!(nanos, 557_938_098);
    }

    #[test]
    fn should_round_trip_through_format_at_millisecond_precision() {
        let s = "2026-02-28T23:59:59.999Z";
        assert_eq!(format_rfc3339(parse_rfc3339(s).unwrap()), s);
    }

    #[test]
    fn should_reject_when_the_offset_or_date_is_malformed() {
        for bad in [
            "2026-09-11",
            "2026-13-01T00:00:00Z",
            "2026-09-11T00:00:00",
            "2026-09-11T00:00:00+0700",
            "2026-09-11T00:00:00.Z",
        ] {
            assert_eq!(parse_rfc3339(bad), None, "{bad}");
        }
    }
}
