use std::collections::BTreeSet;

use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum GrantSource {
    Manual,
    ExternalHrRole,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Grant {
    pub user_id: Uuid,
    pub department_id: Uuid,
    pub source: GrantSource,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Reconciliation {
    pub grant: Vec<Grant>,
    pub revoke: Vec<Grant>,
}

/// Reconciles only HR-derived grants. Manual rows have separate identities and are never mutated here.
pub fn reconcile_automatic_grants(
    existing: &[Grant],
    desired_auto: &BTreeSet<(Uuid, Uuid)>,
) -> Reconciliation {
    let current_auto = existing
        .iter()
        .filter(|grant| grant.source == GrantSource::ExternalHrRole)
        .map(|grant| (grant.user_id, grant.department_id))
        .collect::<BTreeSet<_>>();
    let grant = desired_auto
        .difference(&current_auto)
        .map(|(user_id, department_id)| Grant {
            user_id: *user_id,
            department_id: *department_id,
            source: GrantSource::ExternalHrRole,
        })
        .collect();
    let revoke = current_auto
        .difference(desired_auto)
        .map(|(user_id, department_id)| Grant {
            user_id: *user_id,
            department_id: *department_id,
            source: GrantSource::ExternalHrRole,
        })
        .collect();
    Reconciliation { grant, revoke }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use uuid::Uuid;

    use super::{Grant, GrantSource, reconcile_automatic_grants};

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    #[test]
    fn automatic_sync_preserves_manual_grant() {
        let manual = Grant {
            user_id: id(1),
            department_id: id(9),
            source: GrantSource::Manual,
        };
        let stale_auto = Grant {
            user_id: id(1),
            department_id: id(9),
            source: GrantSource::ExternalHrRole,
        };
        let desired = BTreeSet::new();
        let plan = reconcile_automatic_grants(&[manual.clone(), stale_auto], &desired);
        assert!(plan.grant.is_empty());
        assert_eq!(
            plan.revoke,
            vec![Grant {
                source: GrantSource::ExternalHrRole,
                ..manual
            }]
        );
    }

    #[test]
    fn promotion_revokes_only_stale_automatic_scope() {
        let old = Grant {
            user_id: id(1),
            department_id: id(2),
            source: GrantSource::ExternalHrRole,
        };
        let manual = Grant {
            user_id: id(1),
            department_id: id(2),
            source: GrantSource::Manual,
        };
        let desired = BTreeSet::new();
        let plan = reconcile_automatic_grants(&[old.clone(), manual], &desired);
        assert_eq!(plan.revoke, vec![old]);
    }
}
