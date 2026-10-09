//! Timestamps are stored as whole minutes since the Unix epoch in a `u32` (valid until the
//! year 10135). Minute precision is plenty for "how old is this" questions and halves the
//! space of a 64-bit timestamp.

/// Seconds between 1601-01-01 (Windows FILETIME epoch) and 1970-01-01.
const FILETIME_UNIX_DIFF_SECS: i64 = 11_644_473_600;

/// Converts Unix seconds to stored minutes. Pre-1970 and unknown times map to 0 ("unknown").
pub fn minutes_from_unix_secs(secs: i64) -> u32 {
    if secs <= 0 {
        0
    } else {
        (secs / 60).min(u32::MAX as i64) as u32
    }
}

/// Converts a Windows FILETIME (100 ns ticks since 1601) to stored minutes.
pub fn minutes_from_filetime(ticks: i64) -> u32 {
    minutes_from_unix_secs(ticks / 10_000_000 - FILETIME_UNIX_DIFF_SECS)
}

/// Converts a `SystemTime` to stored minutes.
pub fn minutes_from_system_time(t: std::time::SystemTime) -> u32 {
    match t.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => minutes_from_unix_secs(d.as_secs().min(i64::MAX as u64) as i64),
        Err(_) => 0,
    }
}

/// Formats stored minutes as an ISO-8601 UTC timestamp, e.g. `2026-10-09T20:51Z`.
/// Returns an empty string for 0 (unknown).
pub fn format_iso8601(minutes: u32) -> String {
    if minutes == 0 {
        return String::new();
    }
    let total = minutes as i64;
    let (days, rem) = (total.div_euclid(1440), total.rem_euclid(1440));
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}Z", rem / 60, rem % 60)
}

/// Days since 1970-01-01 to (year, month, day), proleptic Gregorian calendar
/// (Howard Hinnant's public-domain algorithm).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_known_dates() {
        assert_eq!(format_iso8601(0), "");
        assert_eq!(format_iso8601(1), "1970-01-01T00:01Z");
        // 2026-10-09T20:51:00Z = 1791579060 s
        assert_eq!(
            format_iso8601(minutes_from_unix_secs(1_791_579_060)),
            "2026-10-09T20:51Z"
        );
        // 2000-02-29 (leap day) 12:00Z = 951825600 s
        assert_eq!(
            format_iso8601(minutes_from_unix_secs(951_825_600)),
            "2000-02-29T12:00Z"
        );
    }

    #[test]
    fn filetime_conversion() {
        // FILETIME for 1970-01-01T00:01Z
        let ticks = (FILETIME_UNIX_DIFF_SECS + 60) * 10_000_000;
        assert_eq!(minutes_from_filetime(ticks), 1);
        assert_eq!(minutes_from_filetime(0), 0);
    }
}
