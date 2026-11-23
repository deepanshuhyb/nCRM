-- Attendance: dated sessions of the weekly classes, who came, and whether
-- the teacher did.
--
-- Follows the checklist in docs/TENANCY.md.

-- ---------------------------------------------------------------------------
-- sessions: one class meeting on one date ("Hip-hop Beginners, Mon 23 Nov,
-- 18:00"). Created on demand from batch_slots, for today and the past only,
-- so a schedule change never leaves stale future sessions behind.
-- ---------------------------------------------------------------------------
CREATE TYPE session_status AS ENUM ('scheduled', 'cancelled');

CREATE TABLE sessions (
    id                uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id            uuid NOT NULL REFERENCES organizations (id) ON DELETE CASCADE,
    batch_id          uuid NOT NULL,
    on_date           date NOT NULL,
    starts_at         time NOT NULL,
    ends_at           time NOT NULL,
    status            session_status NOT NULL DEFAULT 'scheduled',
    cancel_reason     text CHECK (length(cancel_reason) <= 200),
    -- Who teaches this meeting. Starts as the class's usual teacher; a
    -- substitute can be set for one day without touching the class.
    instructor_id     uuid,
    -- Teacher attendance. NULL until someone marks it.
    teacher_present   boolean,
    teacher_marked_by uuid,
    teacher_marked_at timestamptz,
    created_at        timestamptz NOT NULL DEFAULT now(),
    CHECK (ends_at > starts_at),
    UNIQUE (org_id, id),
    -- Makes "create if missing" safe to run any number of times.
    UNIQUE (org_id, batch_id, on_date, starts_at),
    FOREIGN KEY (org_id, batch_id) REFERENCES batches (org_id, id),
    FOREIGN KEY (org_id, instructor_id) REFERENCES staff (org_id, id),
    FOREIGN KEY (org_id, teacher_marked_by) REFERENCES staff (org_id, id)
);

CREATE INDEX sessions_day_idx ON sessions (org_id, on_date);

ALTER TABLE sessions ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON sessions
    FOR ALL TO ncrm_app
    USING (org_id = app_current_org())
    WITH CHECK (org_id = app_current_org());

-- DELETE only for untouched sessions that a schedule change made wrong
-- (see ncrm_core::batches::update); marked sessions are never deleted.
GRANT SELECT, INSERT, UPDATE, DELETE ON sessions TO ncrm_app;

-- ---------------------------------------------------------------------------
-- attendance: one student at one session.
-- ---------------------------------------------------------------------------
CREATE TYPE attendance_status AS ENUM ('present', 'late', 'absent', 'excused');

CREATE TABLE attendance (
    id            uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id        uuid NOT NULL REFERENCES organizations (id) ON DELETE CASCADE,
    session_id    uuid NOT NULL,
    student_id    uuid NOT NULL,
    status        attendance_status NOT NULL,
    -- Came to a class they aren't a regular member of.
    drop_in       boolean NOT NULL DEFAULT false,
    -- The credit lot a class credit was taken from, when coming used one.
    -- Changing the mark to absent gives it back to the same lot.
    credit_lot_id uuid,
    -- Came on credits but had none left: the studio is owed a class.
    owes_credit   boolean NOT NULL DEFAULT false,
    marked_by     uuid NOT NULL,
    marked_at     timestamptz NOT NULL DEFAULT now(),
    CHECK (NOT (owes_credit AND credit_lot_id IS NOT NULL)),
    UNIQUE (org_id, id),
    UNIQUE (org_id, session_id, student_id),
    FOREIGN KEY (org_id, session_id) REFERENCES sessions (org_id, id) ON DELETE CASCADE,
    FOREIGN KEY (org_id, student_id) REFERENCES students (org_id, id),
    FOREIGN KEY (org_id, credit_lot_id) REFERENCES credit_lots (org_id, id),
    FOREIGN KEY (org_id, marked_by) REFERENCES staff (org_id, id)
);

CREATE INDEX attendance_student_idx ON attendance (org_id, student_id);
CREATE INDEX attendance_owed_idx ON attendance (org_id, student_id) WHERE owes_credit;

ALTER TABLE attendance ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON attendance
    FOR ALL TO ncrm_app
    USING (org_id = app_current_org())
    WITH CHECK (org_id = app_current_org());

-- DELETE: removing a drop-in added by mistake, and clearing a cancelled
-- class. Credits are given back first, in the ledger.
GRANT SELECT, INSERT, UPDATE, DELETE ON attendance TO ncrm_app;
