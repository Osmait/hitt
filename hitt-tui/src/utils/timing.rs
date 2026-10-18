//! Small time-formatting helpers for the UI.

use chrono::{DateTime, Utc};

/// Compact age of `ts` relative to `now`: `now`, `42s`, `5m`, `3h`, `2d`, `6w`.
/// Timestamps in the future (clock skew) read as `now`.
pub fn relative_time(ts: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let secs = (now - ts).num_seconds();
    match secs {
        s if s < 5 => "now".to_string(),
        s if s < 60 => format!("{s}s"),
        s if s < 3_600 => format!("{}m", s / 60),
        s if s < 86_400 => format!("{}h", s / 3_600),
        s if s < 7 * 86_400 => format!("{}d", s / 86_400),
        s => format!("{}w", s / (7 * 86_400)),
    }
}

/// Short duration label: `123ms` below a second, `1.4s` above.
#[allow(clippy::cast_precision_loss)]
pub fn format_duration_ms(ms: u64) -> String {
    if ms < 1_000 {
        format!("{ms}ms")
    } else {
        format!("{:.1}s", ms as f64 / 1_000.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn relative_time_buckets() {
        let now = Utc::now();
        assert_eq!(relative_time(now, now), "now");
        assert_eq!(relative_time(now + Duration::seconds(30), now), "now");
        assert_eq!(relative_time(now - Duration::seconds(42), now), "42s");
        assert_eq!(relative_time(now - Duration::minutes(5), now), "5m");
        assert_eq!(relative_time(now - Duration::hours(3), now), "3h");
        assert_eq!(relative_time(now - Duration::days(2), now), "2d");
        assert_eq!(relative_time(now - Duration::days(21), now), "3w");
    }

    #[test]
    fn duration_labels() {
        assert_eq!(format_duration_ms(87), "87ms");
        assert_eq!(format_duration_ms(1_500), "1.5s");
    }
}
