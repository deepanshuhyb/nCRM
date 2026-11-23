-- Classes: rooms, batches and their weekly schedule, price plans,
-- enrollments, and class credits for packs and drop-ins.
--
-- Words: the dashboard says "class"; the database says "batch", because a
-- class is also a single session (which arrives with attendance).
--
-- Follows the checklist in docs/TENANCY.md: org_id on every row, row-level
-- security, composite (org_id, id) foreign keys, narrow grants.

-- ---------------------------------------------------------------------------
-- rooms: the spaces inside a branch.
-- ---------------------------------------------------------------------------
CREATE TABLE rooms (
    id         uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id     uuid NOT NULL REFERENCES organizations (id) ON DELETE CASCADE,
    branch_id  uuid NOT NULL,
    name       text NOT NULL CHECK (length(btrim(name)) BETWEEN 1 AND 60),
    -- How many people fit. NULL when nobody has said.
    capacity   integer CHECK (capacity BETWEEN 1 AND 1000),
    active     boolean NOT NULL DEFAULT true,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (org_id, id),
    -- Lets batches reference (org_id, branch_id, room_id), which makes the
    -- database refuse a class whose room is in a different branch.
    UNIQUE (org_id, branch_id, id),
    UNIQUE (org_id, branch_id, name),
    FOREIGN KEY (org_id, branch_id) REFERENCES branches (org_id, id)
);

ALTER TABLE rooms ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON rooms
    FOR ALL TO ncrm_app
    USING (org_id = app_current_org())
    WITH CHECK (org_id = app_current_org());

GRANT SELECT, INSERT, UPDATE ON rooms TO ncrm_app;

-- ---------------------------------------------------------------------------
-- batches: a class that meets every week ("Hip-hop Beginners, Mon and Wed
-- 6 pm").
-- ---------------------------------------------------------------------------
CREATE TABLE batches (
    id            uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id        uuid NOT NULL REFERENCES organizations (id) ON DELETE CASCADE,
    branch_id     uuid NOT NULL,
    room_id       uuid,
    -- Who usually teaches it. Any active team member can teach.
    instructor_id uuid,
    name          text NOT NULL CHECK (length(btrim(name)) BETWEEN 1 AND 80),
    -- Free text such as "Hip-hop" or "Bharatanatyam". Optional.
    style         text CHECK (length(btrim(style)) BETWEEN 1 AND 40),
    -- Most students enrolled at once. NULL means no limit.
    capacity      integer CHECK (capacity BETWEEN 1 AND 500),
    starts_on     date NOT NULL,
    -- Last day the class runs. NULL means it keeps running.
    ends_on       date,
    active        boolean NOT NULL DEFAULT true,
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now(),
    CHECK (ends_on IS NULL OR ends_on >= starts_on),
    UNIQUE (org_id, id),
    FOREIGN KEY (org_id, branch_id) REFERENCES branches (org_id, id),
    -- MATCH SIMPLE (the default) skips this check when room_id is NULL.
    FOREIGN KEY (org_id, branch_id, room_id) REFERENCES rooms (org_id, branch_id, id),
    FOREIGN KEY (org_id, instructor_id) REFERENCES staff (org_id, id)
);

CREATE INDEX batches_org_branch_idx ON batches (org_id, branch_id) WHERE active;
CREATE INDEX batches_org_instructor_idx ON batches (org_id, instructor_id) WHERE active;

ALTER TABLE batches ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON batches
    FOR ALL TO ncrm_app
    USING (org_id = app_current_org())
    WITH CHECK (org_id = app_current_org());

-- No DELETE: a class is archived (active = false) so enrollments and,
-- later, attendance keep pointing at it.
GRANT SELECT, INSERT, UPDATE ON batches TO ncrm_app;

-- ---------------------------------------------------------------------------
-- batch_slots: when a batch meets each week, in the branch's local time.
-- ---------------------------------------------------------------------------
CREATE TABLE batch_slots (
    id        uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id    uuid NOT NULL REFERENCES organizations (id) ON DELETE CASCADE,
    batch_id  uuid NOT NULL,
    -- ISO weekday: 1 = Monday ... 7 = Sunday.
    weekday   smallint NOT NULL CHECK (weekday BETWEEN 1 AND 7),
    -- Wall-clock times, no time zone: "6 pm at this branch" stays 6 pm when
    -- clocks change. Classes don't cross midnight.
    starts_at time NOT NULL,
    ends_at   time NOT NULL,
    CHECK (ends_at > starts_at),
    CHECK (ends_at - starts_at <= interval '6 hours'),
    UNIQUE (org_id, id),
    FOREIGN KEY (org_id, batch_id) REFERENCES batches (org_id, id) ON DELETE CASCADE
);

CREATE INDEX batch_slots_batch_idx ON batch_slots (org_id, batch_id);
CREATE INDEX batch_slots_day_idx ON batch_slots (org_id, weekday);

ALTER TABLE batch_slots ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON batch_slots
    FOR ALL TO ncrm_app
    USING (org_id = app_current_org())
    WITH CHECK (org_id = app_current_org());

-- A schedule edit replaces a batch's slots, so DELETE is allowed here.
GRANT SELECT, INSERT, UPDATE, DELETE ON batch_slots TO ncrm_app;

-- ---------------------------------------------------------------------------
-- plans: what the studio charges.
-- ---------------------------------------------------------------------------
CREATE TYPE plan_kind AS ENUM ('monthly', 'pack', 'drop_in');

CREATE TABLE plans (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id      uuid NOT NULL REFERENCES organizations (id) ON DELETE CASCADE,
    name        text NOT NULL CHECK (length(btrim(name)) BETWEEN 1 AND 80),
    kind        plan_kind NOT NULL,
    -- Money is whole paise in a bigint: never floating point.
    price_paise bigint NOT NULL CHECK (price_paise BETWEEN 0 AND 100000000),
    -- Pack and drop-in only: how many classes, valid for how many days.
    classes     integer,
    valid_days  integer,
    active      boolean NOT NULL DEFAULT true,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    -- Every branch starts with an IS NOT NULL test on purpose: a CHECK that
    -- evaluates to NULL counts as passed, so "classes BETWEEN 2 AND 500"
    -- alone would let a pack with no class count through.
    CONSTRAINT plans_shape CHECK (
        (kind = 'monthly' AND classes IS NULL AND valid_days IS NULL)
        OR (kind = 'pack'
            AND classes IS NOT NULL AND classes BETWEEN 2 AND 500
            AND valid_days IS NOT NULL AND valid_days BETWEEN 1 AND 730)
        OR (kind = 'drop_in'
            AND classes IS NOT NULL AND classes = 1
            AND valid_days IS NOT NULL AND valid_days BETWEEN 1 AND 365)
    ),
    UNIQUE (org_id, id),
    UNIQUE (org_id, name)
);

ALTER TABLE plans ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON plans
    FOR ALL TO ncrm_app
    USING (org_id = app_current_org())
    WITH CHECK (org_id = app_current_org());

-- No DELETE: retired plans are made inactive; enrollments and credit packs
-- keep pointing at them.
GRANT SELECT, INSERT, UPDATE ON plans TO ncrm_app;

-- ---------------------------------------------------------------------------
-- enrollments: a student is a regular member of a batch.
-- ---------------------------------------------------------------------------
CREATE TABLE enrollments (
    id         uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id     uuid NOT NULL REFERENCES organizations (id) ON DELETE CASCADE,
    student_id uuid NOT NULL,
    batch_id   uuid NOT NULL,
    -- The monthly plan they pay on. NULL means they pay with class credits.
    plan_id    uuid,
    -- First day in the class.
    starts_on  date NOT NULL,
    -- The day they left: from this day on they're no longer in the class.
    -- NULL while they're still in it. Equal to starts_on when they left
    -- before ever starting.
    ended_on   date,
    created_by uuid,
    created_at timestamptz NOT NULL DEFAULT now(),
    CHECK (ended_on IS NULL OR ended_on >= starts_on),
    UNIQUE (org_id, id),
    FOREIGN KEY (org_id, student_id) REFERENCES students (org_id, id),
    FOREIGN KEY (org_id, batch_id) REFERENCES batches (org_id, id),
    FOREIGN KEY (org_id, plan_id) REFERENCES plans (org_id, id),
    FOREIGN KEY (org_id, created_by) REFERENCES staff (org_id, id)
);

-- Only one open enrollment per student per batch.
CREATE UNIQUE INDEX enrollments_one_open
    ON enrollments (org_id, student_id, batch_id) WHERE ended_on IS NULL;
-- Roster and capacity counts.
CREATE INDEX enrollments_open_batch_idx
    ON enrollments (org_id, batch_id) WHERE ended_on IS NULL;

ALTER TABLE enrollments ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON enrollments
    FOR ALL TO ncrm_app
    USING (org_id = app_current_org())
    WITH CHECK (org_id = app_current_org());

-- No DELETE: leaving a class sets ended_on, which keeps the history.
GRANT SELECT, INSERT, UPDATE ON enrollments TO ncrm_app;

-- ---------------------------------------------------------------------------
-- Class credits: a ledger that is only ever appended to.
--
-- credit_lots:  credits given to a student at once (a pack sold, a drop-in,
--               free classes from the studio), with an expiry.
-- credit_moves: every later change to a lot (a class used, a use undone,
--               credits taken back). A lot's remaining credits are
--               lot.classes + sum(moves.change).
-- ---------------------------------------------------------------------------
CREATE TABLE credit_lots (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id      uuid NOT NULL REFERENCES organizations (id) ON DELETE CASCADE,
    student_id  uuid NOT NULL,
    -- The pack or drop-in plan sold. NULL for free classes given by staff.
    plan_id     uuid,
    classes     integer NOT NULL CHECK (classes BETWEEN 1 AND 500),
    -- Price at the time, copied so later price changes don't rewrite history.
    price_paise bigint NOT NULL DEFAULT 0 CHECK (price_paise BETWEEN 0 AND 100000000),
    -- Last day the credits can be used. NULL means they don't expire.
    expires_on  date,
    note        text CHECK (length(note) <= 200),
    created_by  uuid NOT NULL,
    created_at  timestamptz NOT NULL DEFAULT now(),
    UNIQUE (org_id, id),
    FOREIGN KEY (org_id, student_id) REFERENCES students (org_id, id),
    FOREIGN KEY (org_id, plan_id) REFERENCES plans (org_id, id),
    FOREIGN KEY (org_id, created_by) REFERENCES staff (org_id, id)
);

CREATE INDEX credit_lots_student_idx ON credit_lots (org_id, student_id);

ALTER TABLE credit_lots ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON credit_lots
    FOR ALL TO ncrm_app
    USING (org_id = app_current_org())
    WITH CHECK (org_id = app_current_org());

-- Append-only: no UPDATE, no DELETE. Corrections are new moves.
GRANT SELECT, INSERT ON credit_lots TO ncrm_app;

CREATE TYPE credit_reason AS ENUM (
    'used',      -- a class attended (with attendance)
    'returned',  -- a use undone (attendance corrected)
    'removed'    -- taken back by staff, with a note
);

CREATE TABLE credit_moves (
    id         uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id     uuid NOT NULL REFERENCES organizations (id) ON DELETE CASCADE,
    lot_id     uuid NOT NULL,
    change     integer NOT NULL CHECK (change <> 0 AND change BETWEEN -500 AND 500),
    reason     credit_reason NOT NULL,
    note       text CHECK (length(note) <= 200),
    created_by uuid NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    CHECK ((reason = 'returned') = (change > 0)),
    UNIQUE (org_id, id),
    FOREIGN KEY (org_id, lot_id) REFERENCES credit_lots (org_id, id),
    FOREIGN KEY (org_id, created_by) REFERENCES staff (org_id, id)
);

CREATE INDEX credit_moves_lot_idx ON credit_moves (org_id, lot_id);

ALTER TABLE credit_moves ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON credit_moves
    FOR ALL TO ncrm_app
    USING (org_id = app_current_org())
    WITH CHECK (org_id = app_current_org());

GRANT SELECT, INSERT ON credit_moves TO ncrm_app;

-- The database itself keeps every lot between 0 and its original size, so
-- no code path (a bug, a race between two front-desk screens, a future
-- import) can spend a credit twice or invent one.
--
-- Two inserts for the same lot at the same moment would each see the
-- other's row missing and both pass. The advisory lock makes the second
-- wait until the first commits; in READ COMMITTED each statement in this
-- function then takes a fresh snapshot, so the sum below includes it.
-- (A row lock with SELECT ... FOR UPDATE would need UPDATE rights on
-- credit_lots, which the app role deliberately doesn't have.)
CREATE FUNCTION credit_moves_keep_balance() RETURNS trigger
    LANGUAGE plpgsql
AS $$
DECLARE
    granted integer;
    moved   integer;
BEGIN
    PERFORM pg_advisory_xact_lock(hashtextextended('ncrm:credit_lot:' || NEW.lot_id::text, 0));

    SELECT classes INTO granted
      FROM credit_lots WHERE org_id = NEW.org_id AND id = NEW.lot_id;
    IF granted IS NULL THEN
        RETURN NEW;  -- the foreign key reports the missing lot
    END IF;

    SELECT coalesce(sum(change), 0) INTO moved
      FROM credit_moves WHERE org_id = NEW.org_id AND lot_id = NEW.lot_id;

    IF granted + moved + NEW.change < 0 OR granted + moved + NEW.change > granted THEN
        RAISE EXCEPTION 'credit lot % would hold % of % classes',
                NEW.lot_id, granted + moved + NEW.change, granted
            USING ERRCODE = 'check_violation', CONSTRAINT = 'credit_moves_balance';
    END IF;
    RETURN NEW;
END
$$;

CREATE TRIGGER credit_moves_keep_balance
    BEFORE INSERT ON credit_moves
    FOR EACH ROW EXECUTE FUNCTION credit_moves_keep_balance();
