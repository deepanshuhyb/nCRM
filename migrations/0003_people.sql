-- Students and the people we contact about them.
--
-- A *contact* is anyone with a phone number we message: an adult student
-- themselves, or a parent or guardian. One contact can be linked to several
-- students (a parent with two children at the studio), which is how the
-- WhatsApp bot will ask "which child?" later.
--
-- Follows the checklist in docs/TENANCY.md: org_id on every row, row-level
-- security, composite (org_id, id) foreign keys, narrow grants.

CREATE TYPE student_status AS ENUM ('active', 'paused', 'left');
CREATE TYPE contact_relation AS ENUM ('self', 'mother', 'father', 'guardian', 'other');

-- ---------------------------------------------------------------------------
-- students
-- ---------------------------------------------------------------------------
CREATE TABLE students (
    id            uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id        uuid NOT NULL REFERENCES organizations (id) ON DELETE CASCADE,
    -- The branch they mostly attend. NULL when not set.
    branch_id     uuid,
    name          text NOT NULL CHECK (length(btrim(name)) BETWEEN 1 AND 120),
    -- Optional. Kept only because classes are often grouped by age. Future
    -- dates are refused in the application (a CHECK can't compare with
    -- today, because CHECK expressions must not depend on the clock).
    date_of_birth date CHECK (date_of_birth > DATE '1900-01-01'),
    status        student_status NOT NULL DEFAULT 'active',
    notes         text CHECK (length(notes) <= 2000),
    joined_on     date NOT NULL DEFAULT CURRENT_DATE,
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now(),
    UNIQUE (org_id, id),
    FOREIGN KEY (org_id, branch_id) REFERENCES branches (org_id, id)
);

CREATE INDEX students_org_name_idx ON students (org_id, lower(name));
CREATE INDEX students_org_status_idx ON students (org_id, status);

ALTER TABLE students ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON students
    FOR ALL TO ncrm_app
    USING (org_id = app_current_org())
    WITH CHECK (org_id = app_current_org());

-- No DELETE: students are marked 'left', so history (attendance, fees)
-- keeps pointing at a real row. Erasure on request is a separate,
-- deliberate job (see docs/PLAN.md).
GRANT SELECT, INSERT, UPDATE ON students TO ncrm_app;

-- ---------------------------------------------------------------------------
-- contacts
-- ---------------------------------------------------------------------------
CREATE TABLE contacts (
    id         uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id     uuid NOT NULL REFERENCES organizations (id) ON DELETE CASCADE,
    name       text NOT NULL CHECK (length(btrim(name)) BETWEEN 1 AND 120),
    -- E.164. One contact per number per studio: that's what lets a parent of
    -- two students be one person.
    phone      text NOT NULL CHECK (phone ~ '^\+[1-9][0-9]{7,14}$'),
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (org_id, phone),
    UNIQUE (org_id, id)
);

ALTER TABLE contacts ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON contacts
    FOR ALL TO ncrm_app
    USING (org_id = app_current_org())
    WITH CHECK (org_id = app_current_org());

GRANT SELECT, INSERT, UPDATE ON contacts TO ncrm_app;

-- ---------------------------------------------------------------------------
-- student_contacts: who to message about which student.
-- ---------------------------------------------------------------------------
CREATE TABLE student_contacts (
    org_id     uuid NOT NULL REFERENCES organizations (id) ON DELETE CASCADE,
    student_id uuid NOT NULL,
    contact_id uuid NOT NULL,
    relation   contact_relation NOT NULL,
    is_primary boolean NOT NULL DEFAULT false,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (org_id, student_id, contact_id),
    FOREIGN KEY (org_id, student_id) REFERENCES students (org_id, id) ON DELETE CASCADE,
    FOREIGN KEY (org_id, contact_id) REFERENCES contacts (org_id, id) ON DELETE CASCADE
);

-- At most one primary contact per student.
CREATE UNIQUE INDEX student_contacts_one_primary
    ON student_contacts (org_id, student_id) WHERE is_primary;
-- "Which students is this number about?" (the WhatsApp lookup).
CREATE INDEX student_contacts_contact_idx ON student_contacts (org_id, contact_id);

ALTER TABLE student_contacts ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON student_contacts
    FOR ALL TO ncrm_app
    USING (org_id = app_current_org())
    WITH CHECK (org_id = app_current_org());

GRANT SELECT, INSERT, UPDATE, DELETE ON student_contacts TO ncrm_app;
