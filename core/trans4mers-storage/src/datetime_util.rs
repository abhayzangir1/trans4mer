use chrono::{DateTime, NaiveDateTime, Utc};

/// Safely parses datetime strings originating from SQLite or external sources.
/// Handles:
/// 1. RFC 3339 format ("2026-09-05T11:10:29Z", "2026-09-05T13:53:56.598973500+00:00")
/// 2. Standard SQLite CURRENT_TIMESTAMP format ("2026-09-05 11:10:29")
/// 3. Subsecond SQLite format ("2026-09-05 11:10:29.123")
/// 4. ISO without timezone ("2026-09-05T11:10:29")
/// 5. Falls back to Utc::now() instead of ever panicking.
pub fn parse_db_datetime(s: &str) -> DateTime<Utc> {
    let trimmed = s.trim();
    if let Ok(dt) = DateTime::parse_from_rfc3339(trimmed) {
        return dt.with_timezone(&Utc);
    }
    if let Ok(naive) = NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%d %H:%M:%S") {
        return DateTime::from_naive_utc_and_offset(naive, Utc);
    }
    if let Ok(naive) = NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%d %H:%M:%S%.f") {
        return DateTime::from_naive_utc_and_offset(naive, Utc);
    }
    if let Ok(naive) = NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%dT%H:%M:%S") {
        return DateTime::from_naive_utc_and_offset(naive, Utc);
    }
    if let Ok(naive) = NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%dT%H:%M:%S%.f") {
        return DateTime::from_naive_utc_and_offset(naive, Utc);
    }
    Utc::now()
}

pub fn parse_db_datetime_opt(s: Option<&str>) -> Option<DateTime<Utc>> {
    s.map(parse_db_datetime)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_formats() {
        // RFC 3339
        let dt1 = parse_db_datetime("2026-09-05T11:10:29Z");
        assert_eq!(dt1.to_rfc3339(), "2026-09-05T11:10:29+00:00");

        // SQLite CURRENT_TIMESTAMP
        let dt2 = parse_db_datetime("2026-09-05 11:10:29");
        assert_eq!(dt2.to_rfc3339(), "2026-09-05T11:10:29+00:00");

        // Subsecond SQLite
        let dt3 = parse_db_datetime("2026-09-05 11:10:29.123");
        assert_eq!(dt3.timestamp(), 1788606629);

        // ISO without timezone
        let dt4 = parse_db_datetime("2026-09-05T11:10:29");
        assert_eq!(dt4.to_rfc3339(), "2026-09-05T11:10:29+00:00");

        // Corrupted / unexpected string never panics
        let _dt5 = parse_db_datetime("invalid-date-format");
    }
}
