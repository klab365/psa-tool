//! Forgiving date parsing shared by CLI arguments and interactive prompts.
//!
//! In addition to ISO dates it accepts common German spellings (for example
//! `gestern`), weekdays of the current week (`Montag`) and relative offsets
//! (`+2`, `-1w`).

use chrono::{Datelike, Duration, Local, NaiveDate, Weekday};

/// Returns the Monday-to-Sunday range containing the given date (or today).
pub fn week(date: Option<&str>) -> Result<(String, String), String> {
    let day = match date {
        Some(input) => parse_date(input)?,
        None => Local::now().date_naive(),
    };
    let start = day - Duration::days(day.weekday().num_days_from_monday() as i64);
    Ok((start.to_string(), (start + Duration::days(6)).to_string()))
}

/// Parses a user supplied date. Empty input means "today".
pub fn parse_date(input: &str) -> Result<NaiveDate, String> {
    parse_date_from(input, Local::now().date_naive())
}

/// Deterministic variant with an explicit reference day.
pub fn parse_date_from(input: &str, today: NaiveDate) -> Result<NaiveDate, String> {
    let raw = input.trim();
    let normalized = raw.to_lowercase();

    if normalized.is_empty() || matches!(normalized.as_str(), "heute" | "today" | "now") {
        return Ok(today);
    }

    for format in ["%Y-%m-%d", "%d.%m.%Y", "%d.%m.%y"] {
        if let Ok(date) = NaiveDate::parse_from_str(raw, format) {
            return Ok(date);
        }
    }

    if let Some(date) = parse_day_month(&normalized, today) {
        return Ok(date);
    }

    match normalized.as_str() {
        "gestern" | "yesterday" => return Ok(today - Duration::days(1)),
        "vorgestern" => return Ok(today - Duration::days(2)),
        "morgen" | "tomorrow" => return Ok(today + Duration::days(1)),
        "übermorgen" | "uebermorgen" => return Ok(today + Duration::days(2)),
        _ => {}
    }

    if let Some(days) = parse_offset(&normalized) {
        return Ok(today + Duration::days(days));
    }

    if let Some(weekday) = parse_weekday(&normalized) {
        let monday = today - Duration::days(today.weekday().num_days_from_monday() as i64);
        return Ok(monday + Duration::days(weekday.num_days_from_monday() as i64));
    }

    Err(format!(
        "Ungültiges Datum '{raw}' (erwartet z. B. 2026-09-01, 01.09., gestern, Montag, +2)."
    ))
}

/// Parses `DD.MM.` / `DD.MM` using the year of `today`.
fn parse_day_month(input: &str, today: NaiveDate) -> Option<NaiveDate> {
    let (day, month) = input.trim_end_matches('.').split_once('.')?;
    let day: u32 = day.trim().parse().ok()?;
    let month: u32 = month.trim().parse().ok()?;
    NaiveDate::from_ymd_opt(today.year(), month, day)
}

/// Parses signed day or week offsets such as `+2`, `-3d` or `+1w`.
fn parse_offset(input: &str) -> Option<i64> {
    let (sign, rest) = if let Some(rest) = input.strip_prefix('+') {
        (1, rest)
    } else {
        let rest = input.strip_prefix('-')?;
        (-1, rest)
    };
    let (number, unit) = if let Some(number) = rest.strip_suffix('w') {
        (number, 7)
    } else if let Some(number) = rest.strip_suffix('d') {
        (number, 1)
    } else {
        (rest, 1)
    };
    Some(sign * number.parse::<i64>().ok()? * unit)
}

fn parse_weekday(input: &str) -> Option<Weekday> {
    let key: String = input.chars().filter(|c| c.is_alphabetic()).collect();
    match key.as_str() {
        "mo" | "mon" | "montag" | "monday" => Some(Weekday::Mon),
        "di" | "die" | "dien" | "dienstag" | "tuesday" | "tue" | "tues" => Some(Weekday::Tue),
        "mi" | "mit" | "mittwoch" | "wednesday" | "wed" => Some(Weekday::Wed),
        "do" | "don" | "donnerstag" | "thursday" | "thu" | "thur" | "thurs" => Some(Weekday::Thu),
        "fr" | "fre" | "freitag" | "friday" | "fri" => Some(Weekday::Fri),
        "sa" | "sam" | "samstag" | "saturday" | "sat" => Some(Weekday::Sat),
        "so" | "son" | "sonntag" | "sunday" | "sun" => Some(Weekday::Sun),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 9, 3).unwrap() // Thursday
    }

    #[test]
    fn parses_iso_and_german_dates() {
        assert_eq!(
            parse_date_from("2026-09-01", today()).unwrap(),
            NaiveDate::from_ymd_opt(2026, 9, 1).unwrap()
        );
        assert_eq!(
            parse_date_from("01.09.", today()).unwrap(),
            NaiveDate::from_ymd_opt(2026, 9, 1).unwrap()
        );
        assert_eq!(
            parse_date_from("01.09.2025", today()).unwrap(),
            NaiveDate::from_ymd_opt(2025, 9, 1).unwrap()
        );
    }

    #[test]
    fn parses_relative_dates_and_weekdays() {
        assert_eq!(parse_date_from("", today()).unwrap(), today());
        assert_eq!(
            parse_date_from("gestern", today()).unwrap(),
            NaiveDate::from_ymd_opt(2026, 9, 2).unwrap()
        );
        assert_eq!(
            parse_date_from("+2", today()).unwrap(),
            NaiveDate::from_ymd_opt(2026, 9, 5).unwrap()
        );
        assert_eq!(
            parse_date_from("-1w", today()).unwrap(),
            NaiveDate::from_ymd_opt(2026, 8, 27).unwrap()
        );
        assert_eq!(
            parse_date_from("Montag", today()).unwrap(),
            NaiveDate::from_ymd_opt(2026, 8, 31).unwrap()
        );
    }

    #[test]
    fn rejects_unknown_input() {
        assert!(parse_date_from("keine ahnung", today()).is_err());
    }
}
