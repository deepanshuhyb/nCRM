//! What each staff role is allowed to do.
//!
//! Row-level security decides *which studio's* data a request can touch.
//! This module decides *what a person may do* inside their own studio.
//! Handlers ask `role.can(Capability::X)` before acting; the matrix below is
//! the single place the rules live.

use serde::Serialize;

use crate::staff::StaffRole;

/// Things a signed-in staff member might try to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    /// See branches, classes and the dashboard.
    ViewStudio,
    /// Add, rename or close branches.
    ManageBranches,
    /// Add, edit or deactivate staff.
    ManageStaff,
    /// Enroll and edit students and families.
    ManageStudents,
    /// Record payments, issue receipts, change fee plans.
    ManageFees,
    /// Mark attendance for any class.
    MarkAnyAttendance,
    /// Mark attendance for classes the person teaches.
    MarkOwnAttendance,
    /// See revenue, collection and other money reports.
    ViewFinancials,
    /// Change the studio's subscription and message wallet.
    ManageBilling,
}

impl Capability {
    pub const ALL: [Capability; 9] = [
        Capability::ViewStudio,
        Capability::ManageBranches,
        Capability::ManageStaff,
        Capability::ManageStudents,
        Capability::ManageFees,
        Capability::MarkAnyAttendance,
        Capability::MarkOwnAttendance,
        Capability::ViewFinancials,
        Capability::ManageBilling,
    ];
}

impl StaffRole {
    /// Whether this role may do `cap`.
    pub fn can(self, cap: Capability) -> bool {
        use Capability::*;
        use StaffRole::*;
        match self {
            Owner => true,
            Manager => !matches!(cap, ManageBilling),
            FrontDesk => matches!(
                cap,
                ViewStudio | ManageStudents | ManageFees | MarkAnyAttendance | MarkOwnAttendance
            ),
            Instructor => matches!(cap, ViewStudio | MarkOwnAttendance),
        }
    }

    /// Every capability this role has, for showing or hiding parts of the UI.
    pub fn capabilities(self) -> Vec<Capability> {
        Capability::ALL
            .into_iter()
            .filter(|c| self.can(*c))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::Capability::*;
    use crate::staff::StaffRole::*;

    #[test]
    fn owners_can_do_everything() {
        assert_eq!(Owner.capabilities().len(), super::Capability::ALL.len());
    }

    #[test]
    fn only_owners_touch_billing() {
        assert!(Owner.can(ManageBilling));
        for role in [Manager, FrontDesk, Instructor] {
            assert!(!role.can(ManageBilling), "{role:?}");
        }
    }

    #[test]
    fn front_desk_runs_the_day_but_not_the_business() {
        assert!(FrontDesk.can(ManageStudents));
        assert!(FrontDesk.can(ManageFees));
        assert!(FrontDesk.can(MarkAnyAttendance));
        assert!(!FrontDesk.can(ManageBranches));
        assert!(!FrontDesk.can(ManageStaff));
        assert!(!FrontDesk.can(ViewFinancials));
    }

    #[test]
    fn instructors_see_and_mark_only_their_classes() {
        assert_eq!(
            Instructor.capabilities(),
            vec![ViewStudio, MarkOwnAttendance]
        );
    }

    #[test]
    fn everyone_can_view_the_studio() {
        for role in [Owner, Manager, FrontDesk, Instructor] {
            assert!(role.can(ViewStudio), "{role:?}");
        }
    }
}
