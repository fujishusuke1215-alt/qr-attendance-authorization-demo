use chrono::{DateTime, NaiveDate, Utc};
use chrono_tz::Asia::Tokyo;
use serde::Serialize;
use uuid::Uuid;

use crate::qr::QrKind;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AttendanceRecord {
    pub id: Uuid,
    pub user_id: Uuid,
    pub display_name: String,
    pub email: Option<String>,
    pub department_name: Option<String>,
    pub kind: QrKind,
    pub work_date: NaiveDate,
    pub occurred_at: DateTime<Utc>,
    pub qr_revision: i64,
}

pub fn work_date_in_tokyo(now: DateTime<Utc>) -> NaiveDate {
    now.with_timezone(&Tokyo).date_naive()
}

#[cfg(test)]
mod tests {
    use super::work_date_in_tokyo;
    use chrono::{DateTime, Utc};

    #[test]
    fn attendance_day_uses_tokyo_midnight_boundary() {
        let just_before = DateTime::parse_from_rfc3339("2026-11-14T14:59:59Z")
            .unwrap()
            .with_timezone(&Utc);
        let at_midnight = DateTime::parse_from_rfc3339("2026-11-14T15:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(work_date_in_tokyo(just_before).to_string(), "2026-11-14");
        assert_eq!(work_date_in_tokyo(at_midnight).to_string(), "2026-11-15");
    }
}
