//! UTC timestamps as RFC 3339 strings (`2026-10-05T09:41:00Z`), without a date crate.

use std::time::{SystemTime, UNIX_EPOCH};

/// The current time.
#[must_use]
pub fn now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    from_unix(i64::try_from(secs).unwrap_or(i64::MAX))
}

/// Seconds since the Unix epoch, as RFC 3339 in UTC.
#[must_use]
pub fn from_unix(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// `2026-10-05T09:41:00Z` → `20261005T094100Z`, for file names.
#[must_use]
pub fn compact(ts: &str) -> String {
    ts.chars().filter(|c| !matches!(c, '-' | ':')).collect()
}

/// The date part of a timestamp.
#[must_use]
pub fn date(ts: &str) -> &str {
    ts.get(..10).unwrap_or(ts)
}

/// Howard Hinnant's `civil_from_days`: days since 1970-01-01 → (year, month, day).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // 1..=12 and 1..=31
    (y, m as u32, d as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_unix_times() {
        assert_eq!(from_unix(0), "1970-01-01T00:00:00Z");
        assert_eq!(from_unix(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(from_unix(1_791_193_260), "2026-10-05T09:41:00Z");
        assert_eq!(compact("2026-10-05T09:41:00Z"), "20261005T094100Z");
        assert_eq!(date("2026-10-05T09:41:00Z"), "2026-10-05");
    }
}
