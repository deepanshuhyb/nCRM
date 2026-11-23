//! What each staff role is allowed to do.
//!
//! Row-level security decides *which studio's* data a request can touch.
//! This module decides *what a person may do* inside their own studio.
//! Handlers ask `role.can(Capability::X)` before acting; the matrix below is
//! the single place the rules live.

use serde::Serialize;
use uuid::Uuid;

use crate::staff::StaffRole;
use crate::{Error, Result};

/// The staff member doing something. Passed into functions that apply
/// rules beyond the capability matrix (who may change whom) and that record
/// who did what in the audit log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Actor {
    pub staff_id: Uuid,
    pub role: StaffRole,
}

impl Actor {
    /// Refuse unless the actor's role allows `cap`. Handlers check this too;
    /// checking again in the core means a forgotten handler check can't
    /// open a hole.
    pub fn require(&self, cap: Capability) -> Result<()> {
        if self.role.can(cap) {
            Ok(())
        } else {
            Err(Error::Forbidden)
        }
    }
}

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
    /// Add and edit students and their contacts; put them in classes.
    ManageStudents,
    /// Set up rooms, classes, schedules and prices; give or take back free
    /// class credits. The shape of the business, so not front desk.
    ManageClasses,
    /// Sell class packs, add charges, record payments, issue receipts.
    ManageFees,
    /// Cancel a charge or a recorded payment (waivers, refunds, mistakes).
    /// Kept away from the front desk: whoever takes the cash shouldn't be
    /// able to make the record of it disappear.
    AdjustFees,
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
    pub const ALL: [Capability; 11] = [
        Capability::ViewStudio,
        Capability::ManageBranches,
        Capability::ManageStaff,
        Capability::ManageStudents,
        Capability::ManageClasses,
        Capability::ManageFees,
        Capability::AdjustFees,
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
        // Front desk sells packs but can't change prices or give classes away.
        assert!(!FrontDesk.can(ManageClasses));
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
    fn managers_shape_classes_and_prices() {
        assert!(Manager.can(ManageClasses));
        assert!(Owner.can(ManageClasses));
        assert!(!Instructor.can(ManageClasses));
    }

    #[test]
    fn everyone_can_view_the_studio() {
        for role in [Owner, Manager, FrontDesk, Instructor] {
            assert!(role.can(ViewStudio), "{role:?}");
        }
    }
}
