use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

const DEPARTMENT_NAMESPACE: Uuid = Uuid::from_u128(0xa6a9_b6a1_6d91_4e75_a81b_2379_f05b_21d2);

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DepartmentInput {
    pub identity_code: String,
    pub code: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub name: String,
    pub parent_identity_code: Option<String>,
    #[serde(default = "default_true")]
    pub active: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DepartmentTreeItem {
    pub id: Uuid,
    pub identity_code: String,
    pub name: String,
    pub parent_id: Option<Uuid>,
    pub active: bool,
}

pub fn department_id(identity_code: &str) -> Uuid {
    Uuid::new_v5(&DEPARTMENT_NAMESPACE, identity_code.trim().as_bytes())
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum DepartmentError {
    #[error("department identity code, source code, name, and alias values must be non-empty")]
    EmptyField,
    #[error("department identity code is duplicated")]
    DuplicateIdentity,
    #[error("primary or alias department code is assigned more than once")]
    DuplicateCode,
    #[error("department parent identity does not exist")]
    MissingParent,
    #[error("department hierarchy contains a cycle")]
    Cycle,
}

pub fn validate_departments(items: &[DepartmentInput]) -> Result<(), DepartmentError> {
    let mut identities = BTreeSet::new();
    let mut codes = BTreeSet::new();
    for item in items {
        if item.identity_code.trim().is_empty()
            || item.code.trim().is_empty()
            || item.name.trim().is_empty()
            || item.aliases.iter().any(|value| value.trim().is_empty())
        {
            return Err(DepartmentError::EmptyField);
        }
        if !identities.insert(item.identity_code.as_str()) {
            return Err(DepartmentError::DuplicateIdentity);
        }
        if !codes.insert(item.code.as_str())
            || item
                .aliases
                .iter()
                .any(|alias| !codes.insert(alias.as_str()))
        {
            return Err(DepartmentError::DuplicateCode);
        }
    }

    let by_identity = items
        .iter()
        .map(|item| (item.identity_code.as_str(), item))
        .collect::<BTreeMap<_, _>>();
    for item in items {
        if item
            .parent_identity_code
            .as_deref()
            .is_some_and(|parent| !by_identity.contains_key(parent))
        {
            return Err(DepartmentError::MissingParent);
        }
        let mut visited = BTreeSet::new();
        let mut current = Some(item.identity_code.as_str());
        while let Some(identity) = current {
            if !visited.insert(identity) {
                return Err(DepartmentError::Cycle);
            }
            current = by_identity
                .get(identity)
                .and_then(|node| node.parent_identity_code.as_deref());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{DepartmentInput, department_id, validate_departments};

    fn item(identity: &str, code: &str, name: &str, parent: Option<&str>) -> DepartmentInput {
        DepartmentInput {
            identity_code: identity.into(),
            code: code.into(),
            aliases: vec![],
            name: name.into(),
            parent_identity_code: parent.map(str::to_owned),
            active: true,
        }
    }

    #[test]
    fn department_identity_survives_name_and_parent_changes() {
        let original = department_id("ORG-ENG");
        let renamed_and_moved = department_id("ORG-ENG");
        assert_eq!(original, renamed_and_moved);
        let first = vec![
            item("ORG-ROOT", "A", "Group", None),
            item("ORG-ENG", "B", "Engineering", Some("ORG-ROOT")),
        ];
        let second = vec![
            item("ORG-ROOT", "A", "New parent", None),
            item("ORG-ENG", "B", "Platform", Some("ORG-ROOT")),
        ];
        validate_departments(&first).unwrap();
        validate_departments(&second).unwrap();
        assert_eq!(
            department_id(&first[1].identity_code),
            department_id(&second[1].identity_code)
        );
    }

    #[test]
    fn same_name_with_different_identity_codes_stays_distinct() {
        let a = item("ORG-101", "D-101", "Operations", None);
        let b = item("ORG-202", "D-202", "Operations", None);
        validate_departments(&[a.clone(), b.clone()]).unwrap();
        assert_ne!(
            department_id(&a.identity_code),
            department_id(&b.identity_code)
        );
    }

    #[test]
    fn explicit_alias_codes_resolve_to_one_identity() {
        let mut node = item("ORG-ENG", "ENG-NEW", "Engineering", None);
        node.aliases.push("ENG-OLD".into());
        validate_departments(&[node]).unwrap();
        let mut collision = item("ORG-OPS", "OPS", "Operations", None);
        collision.aliases.push("ENG-OLD".into());
        let mut first = item("ORG-ENG", "ENG-NEW", "Engineering", None);
        first.aliases.push("ENG-OLD".into());
        assert!(validate_departments(&[first, collision]).is_err());
    }
}
