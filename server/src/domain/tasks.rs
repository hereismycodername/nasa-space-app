use super::measurements::ValidationError;
use chrono::NaiveDate;

#[derive(Debug)]
pub(crate) struct Task {
    pub id: String,
    pub title: String,
    pub completed_at: Option<String>,
}

/// Accept only canonical calendar dates; timestamps and relaxed padding are invalid.
pub(crate) fn parse_date(
    value: Option<&str>,
    field: &'static str,
) -> Result<NaiveDate, ValidationError> {
    let invalid = || ValidationError {
        field,
        message: "Expected a calendar date in YYYY-MM-DD format",
    };
    let value = value.ok_or_else(invalid)?;
    let date = NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| invalid())?;
    if value.len() != 10 || date.to_string() != value {
        return Err(invalid());
    }
    Ok(date)
}
