use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::org::{DepartmentInput, validate_departments};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EmployeeInput {
    pub employee_code: String,
    pub display_name: String,
    pub email: Option<String>,
    pub job_code: Option<String>,
    /// The adapter must set this only when the upstream field was present.
    /// `false` is a malformed export; `true` plus null means explicitly unassigned.
    #[serde(default)]
    pub job_code_field_present: bool,
    pub job_title: Option<String>,
    pub active: bool,
    pub department_identity_codes: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HrSnapshot {
    pub snapshot_id: String,
    pub complete: bool,
    pub departments: Vec<DepartmentInput>,
    pub employees: Vec<EmployeeInput>,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum SnapshotError {
    #[error("snapshot is marked incomplete")]
    Incomplete,
    #[error("snapshot identifier is missing")]
    MissingSnapshotId,
    #[error("snapshot has no departments")]
    EmptyDepartments,
    #[error("department records are invalid: {0}")]
    Departments(String),
    #[error("employee code or display name is empty")]
    MissingEmployeeIdentity,
    #[error("employee code is duplicated")]
    DuplicateEmployee,
    #[error("employee job_code field was omitted")]
    MissingJobCodeField,
    #[error("active employee has no department affiliation")]
    MissingAffiliation,
    #[error("employee refers to an unknown or repeated department identity")]
    InvalidAffiliation,
    #[error("employee email is not a valid email address")]
    InvalidEmail,
}

/// Validates all source data before the reconciliation layer starts any writes.
pub fn validate_snapshot(snapshot: &HrSnapshot) -> Result<(), SnapshotError> {
    if !snapshot.complete {
        return Err(SnapshotError::Incomplete);
    }
    if snapshot.snapshot_id.trim().is_empty() {
        return Err(SnapshotError::MissingSnapshotId);
    }
    if snapshot.departments.is_empty() {
        return Err(SnapshotError::EmptyDepartments);
    }
    validate_departments(&snapshot.departments)
        .map_err(|error| SnapshotError::Departments(error.to_string()))?;

    let department_codes = snapshot
        .departments
        .iter()
        .map(|department| department.identity_code.as_str())
        .collect::<BTreeSet<_>>();
    let mut employee_codes = BTreeSet::new();
    for employee in &snapshot.employees {
        if employee.employee_code.trim().is_empty() || employee.display_name.trim().is_empty() {
            return Err(SnapshotError::MissingEmployeeIdentity);
        }
        if !employee_codes.insert(employee.employee_code.as_str()) {
            return Err(SnapshotError::DuplicateEmployee);
        }
        if !employee.job_code_field_present {
            return Err(SnapshotError::MissingJobCodeField);
        }
        if employee.active && employee.department_identity_codes.is_empty() {
            return Err(SnapshotError::MissingAffiliation);
        }
        if employee
            .department_identity_codes
            .iter()
            .any(|department| !department_codes.contains(department.as_str()))
        {
            return Err(SnapshotError::InvalidAffiliation);
        }
        let affiliations = employee
            .department_identity_codes
            .iter()
            .collect::<BTreeSet<_>>();
        if affiliations.len() != employee.department_identity_codes.len() {
            return Err(SnapshotError::InvalidAffiliation);
        }
        if employee.email.as_deref().is_some_and(|email| {
            !email.contains('@') || email.starts_with('@') || email.ends_with('@')
        }) {
            return Err(SnapshotError::InvalidEmail);
        }
    }
    Ok(())
}

/// Hashes a canonicalized view so source ordering does not create new sync runs.
pub fn snapshot_hash(snapshot: &HrSnapshot) -> String {
    let mut canonical = snapshot.clone();
    canonical
        .departments
        .sort_by(|left, right| left.identity_code.cmp(&right.identity_code));
    for department in &mut canonical.departments {
        department.aliases.sort();
    }
    canonical
        .employees
        .sort_by(|left, right| left.employee_code.cmp(&right.employee_code));
    for employee in &mut canonical.employees {
        employee.department_identity_codes.sort();
    }
    let bytes =
        serde_json::to_vec(&canonical).expect("serializing an in-memory HR snapshot is infallible");
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::{EmployeeInput, HrSnapshot, SnapshotError, snapshot_hash, validate_snapshot};
    use crate::org::DepartmentInput;

    fn snapshot() -> HrSnapshot {
        HrSnapshot {
            snapshot_id: "fixture-2026-11".into(),
            complete: true,
            departments: vec![DepartmentInput {
                identity_code: "ROOT".into(),
                code: "R".into(),
                aliases: vec![],
                name: "Root".into(),
                parent_identity_code: None,
                active: true,
            }],
            employees: vec![EmployeeInput {
                employee_code: "EMP-1".into(),
                display_name: "Employee One".into(),
                email: Some("one@example.test".into()),
                job_code: None,
                job_code_field_present: true,
                job_title: None,
                active: true,
                department_identity_codes: vec!["ROOT".into()],
            }],
        }
    }

    #[test]
    fn snapshot_rejects_missing_required_fields_before_reconciliation() {
        let mut malformed = snapshot();
        malformed.employees[0].job_code_field_present = false;
        assert_eq!(
            validate_snapshot(&malformed),
            Err(SnapshotError::MissingJobCodeField)
        );
    }

    #[test]
    fn incomplete_snapshot_cannot_remove_existing_grants() {
        let mut partial = snapshot();
        partial.complete = false;
        assert_eq!(validate_snapshot(&partial), Err(SnapshotError::Incomplete));
    }

    #[test]
    fn identical_records_hash_equally_regardless_of_source_order() {
        let mut first = snapshot();
        let second = first.clone();
        first.snapshot_id = "same-source".into();
        let mut second = second;
        second.snapshot_id = "same-source".into();
        first.departments.push(DepartmentInput {
            identity_code: "CHILD".into(),
            code: "C".into(),
            aliases: vec!["C-OLD".into(), "C-LEGACY".into()],
            name: "Child".into(),
            parent_identity_code: Some("ROOT".into()),
            active: true,
        });
        second.departments.push(first.departments[1].clone());
        first.departments[1].aliases.reverse();
        assert_eq!(snapshot_hash(&first), snapshot_hash(&second));
    }
}
