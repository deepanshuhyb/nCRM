-- Fees: what students owe (charges), what they paid (payments), and which
-- payment settled which charge (allocations).
--
-- Follows the checklist in docs/TENANCY.md.
--
-- Money rules the database itself holds, whatever the application does:
--   * amounts are whole paise in a bigint, never floating point;
--   * an amount, once written, can't be changed: the app role may only
--     fill in the three "voided" columns of a charge or payment;
--   * a void can't be undone, and a voided row holds no allocations;
--   * allocations never exceed the payment or the charge they join.

-- ---------------------------------------------------------------------------
-- Studio settings for monthly fees.
-- ---------------------------------------------------------------------------
ALTER TABLE organizations
    -- First month (its first day) for which monthly fees are created
    -- automatically. NULL: the studio hasn't switched this on, so opening
    -- the fees page never creates charges by surprise.
    ADD COLUMN fees_from date
        CHECK (fees_from IS NULL OR extract(day FROM fees_from) = 1),
    -- Monthly fees fall due on this day of the month. Up to 28 so every
    -- month has it.
    ADD COLUMN fee_due_day smallint NOT NULL DEFAULT 5
        CHECK (fee_due_day BETWEEN 1 AND 28);

-- ---------------------------------------------------------------------------
-- charges: money a student owes the studio.
-- ---------------------------------------------------------------------------
CREATE TYPE charge_kind AS ENUM ('monthly', 'pack', 'drop_in', 'other');

CREATE TABLE charges (
    id            uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id        uuid NOT NULL REFERENCES organizations (id) ON DELETE CASCADE,
    student_id    uuid NOT NULL,
    kind          charge_kind NOT NULL,
    -- What it's for, as it appears on the receipt.
    description   text NOT NULL CHECK (length(btrim(description)) BETWEEN 1 AND 120),
    amount_paise  bigint NOT NULL CHECK (amount_paise BETWEEN 1 AND 100000000),
    due_on        date NOT NULL,
    -- Monthly fees: the class membership and the month (its first day).
    enrollment_id uuid,
    period_start  date CHECK (period_start IS NULL OR extract(day FROM period_start) = 1),
    -- Packs and drop-ins: the class credits that were sold.
    credit_lot_id uuid,
    -- NULL when created automatically (monthly fees).
    created_by    uuid,
    created_at    timestamptz NOT NULL DEFAULT now(),
    voided_at     timestamptz,
    voided_by     uuid,
    void_reason   text CHECK (length(btrim(void_reason)) BETWEEN 1 AND 200),
    CONSTRAINT charges_monthly_shape
        CHECK ((kind = 'monthly') = (enrollment_id IS NOT NULL AND period_start IS NOT NULL)),
    CONSTRAINT charges_void_shape
        CHECK ((voided_at IS NULL) = (void_reason IS NULL)),
    UNIQUE (org_id, id),
    -- Lets an allocation prove its charge and payment are the same student's.
    UNIQUE (org_id, student_id, id),
    FOREIGN KEY (org_id, student_id) REFERENCES students (org_id, id),
    FOREIGN KEY (org_id, enrollment_id) REFERENCES enrollments (org_id, id),
    FOREIGN KEY (org_id, credit_lot_id) REFERENCES credit_lots (org_id, id),
    FOREIGN KEY (org_id, created_by) REFERENCES staff (org_id, id),
    FOREIGN KEY (org_id, voided_by) REFERENCES staff (org_id, id)
);

-- One fee per class per month, ever. Voided ones count too: waiving a
-- month's fee must not make the next page load create it again.
CREATE UNIQUE INDEX charges_one_per_month
    ON charges (org_id, enrollment_id, period_start) WHERE enrollment_id IS NOT NULL;
CREATE INDEX charges_student_idx ON charges (org_id, student_id, due_on);
CREATE INDEX charges_open_idx ON charges (org_id, due_on) WHERE voided_at IS NULL;

ALTER TABLE charges ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON charges
    FOR ALL TO ncrm_app
    USING (org_id = app_current_org())
    WITH CHECK (org_id = app_current_org());

GRANT SELECT, INSERT ON charges TO ncrm_app;
-- Column-level: voiding is the only change the app can make to a charge.
GRANT UPDATE (voided_at, voided_by, void_reason) ON charges TO ncrm_app;

-- ---------------------------------------------------------------------------
-- payments: money the studio received. Each has a receipt number that
-- counts up per studio with no gaps.
-- ---------------------------------------------------------------------------
CREATE TYPE payment_method AS ENUM ('cash', 'upi', 'card', 'bank_transfer', 'cheque', 'other');

CREATE TABLE payments (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id       uuid NOT NULL REFERENCES organizations (id) ON DELETE CASCADE,
    student_id   uuid NOT NULL,
    receipt_no   bigint NOT NULL CHECK (receipt_no > 0),
    amount_paise bigint NOT NULL CHECK (amount_paise BETWEEN 1 AND 100000000),
    method       payment_method NOT NULL,
    -- UPI reference, cheque number and the like. Never card numbers.
    reference    text CHECK (length(reference) <= 80),
    note         text CHECK (length(note) <= 200),
    -- The day the money came in, which can be earlier than created_at.
    received_on  date NOT NULL,
    received_by  uuid NOT NULL,
    created_at   timestamptz NOT NULL DEFAULT now(),
    voided_at    timestamptz,
    voided_by    uuid,
    void_reason  text CHECK (length(btrim(void_reason)) BETWEEN 1 AND 200),
    CONSTRAINT payments_void_shape
        CHECK ((voided_at IS NULL) = (void_reason IS NULL)),
    UNIQUE (org_id, id),
    UNIQUE (org_id, student_id, id),
    UNIQUE (org_id, receipt_no),
    FOREIGN KEY (org_id, student_id) REFERENCES students (org_id, id),
    FOREIGN KEY (org_id, received_by) REFERENCES staff (org_id, id),
    FOREIGN KEY (org_id, voided_by) REFERENCES staff (org_id, id)
);

CREATE INDEX payments_student_idx ON payments (org_id, student_id, received_on);
CREATE INDEX payments_day_idx ON payments (org_id, received_on);

ALTER TABLE payments ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON payments
    FOR ALL TO ncrm_app
    USING (org_id = app_current_org())
    WITH CHECK (org_id = app_current_org());

GRANT SELECT, INSERT ON payments TO ncrm_app;
GRANT UPDATE (voided_at, voided_by, void_reason) ON payments TO ncrm_app;

-- The last receipt number given out in each studio. Taking the next one
-- updates this row inside the payment's transaction: the row lock makes
-- two payments at once take turns, and a payment that rolls back gives
-- its number back, so there are no gaps.
CREATE TABLE receipt_counters (
    org_id  uuid PRIMARY KEY REFERENCES organizations (id) ON DELETE CASCADE,
    last_no bigint NOT NULL CHECK (last_no > 0)
);

ALTER TABLE receipt_counters ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON receipt_counters
    FOR ALL TO ncrm_app
    USING (org_id = app_current_org())
    WITH CHECK (org_id = app_current_org());

GRANT SELECT, INSERT, UPDATE ON receipt_counters TO ncrm_app;

-- ---------------------------------------------------------------------------
-- payment_allocations: "this much of that payment went to that charge".
-- A payment bigger than what's owed keeps the rest unallocated (an
-- advance) until a later charge takes it.
-- ---------------------------------------------------------------------------
CREATE TABLE payment_allocations (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id       uuid NOT NULL REFERENCES organizations (id) ON DELETE CASCADE,
    student_id   uuid NOT NULL,
    payment_id   uuid NOT NULL,
    charge_id    uuid NOT NULL,
    amount_paise bigint NOT NULL CHECK (amount_paise > 0),
    created_at   timestamptz NOT NULL DEFAULT now(),
    UNIQUE (org_id, id),
    -- Both keys carry student_id, so money can't be moved between students.
    FOREIGN KEY (org_id, student_id, payment_id) REFERENCES payments (org_id, student_id, id),
    FOREIGN KEY (org_id, student_id, charge_id) REFERENCES charges (org_id, student_id, id)
);

CREATE INDEX payment_allocations_payment_idx ON payment_allocations (org_id, payment_id);
CREATE INDEX payment_allocations_charge_idx ON payment_allocations (org_id, charge_id);

ALTER TABLE payment_allocations ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON payment_allocations
    FOR ALL TO ncrm_app
    USING (org_id = app_current_org())
    WITH CHECK (org_id = app_current_org());

-- DELETE: a void frees the money so it can settle something else. No UPDATE.
GRANT SELECT, INSERT, DELETE ON payment_allocations TO ncrm_app;

-- An allocation may not take more than is left of its payment, nor pay
-- more than is left of its charge, and neither may be voided.
--
-- Two allocations for the same student at once take turns on a
-- per-student advisory lock (the same key `fees.rs` takes before it reads
-- balances); in READ COMMITTED each statement below then sees what the
-- first one committed.
CREATE FUNCTION payment_allocations_stay_in_bounds() RETURNS trigger
    LANGUAGE plpgsql
AS $$
DECLARE
    pay_amount    bigint;
    pay_voided    timestamptz;
    charge_amount bigint;
    charge_voided timestamptz;
    pay_used      bigint;
    charge_paid   bigint;
BEGIN
    PERFORM pg_advisory_xact_lock(
        hashtextextended('ncrm:fees:' || NEW.student_id::text || ':' || NEW.org_id::text, 0));

    SELECT amount_paise, voided_at INTO pay_amount, pay_voided
      FROM payments WHERE org_id = NEW.org_id AND id = NEW.payment_id;
    SELECT amount_paise, voided_at INTO charge_amount, charge_voided
      FROM charges WHERE org_id = NEW.org_id AND id = NEW.charge_id;
    IF pay_amount IS NULL OR charge_amount IS NULL THEN
        RETURN NEW;  -- the foreign keys report what's missing
    END IF;

    IF pay_voided IS NOT NULL OR charge_voided IS NOT NULL THEN
        RAISE EXCEPTION 'allocation to a voided payment or charge'
            USING ERRCODE = 'check_violation', CONSTRAINT = 'payment_allocations_voided';
    END IF;

    SELECT coalesce(sum(amount_paise), 0) INTO pay_used
      FROM payment_allocations WHERE org_id = NEW.org_id AND payment_id = NEW.payment_id;
    SELECT coalesce(sum(amount_paise), 0) INTO charge_paid
      FROM payment_allocations WHERE org_id = NEW.org_id AND charge_id = NEW.charge_id;

    IF pay_used + NEW.amount_paise > pay_amount
       OR charge_paid + NEW.amount_paise > charge_amount THEN
        RAISE EXCEPTION 'allocation of % exceeds payment (% of %) or charge (% of %)',
                NEW.amount_paise, pay_used, pay_amount, charge_paid, charge_amount
            USING ERRCODE = 'check_violation', CONSTRAINT = 'payment_allocations_bounds';
    END IF;
    RETURN NEW;
END
$$;

CREATE TRIGGER payment_allocations_stay_in_bounds
    BEFORE INSERT ON payment_allocations
    FOR EACH ROW EXECUTE FUNCTION payment_allocations_stay_in_bounds();

-- Voiding: once voided, always voided; and the row lets go of its
-- allocations in the same statement, so "voided rows hold no money" is
-- true by construction, not by the application remembering.
CREATE FUNCTION fees_void_rules() RETURNS trigger
    LANGUAGE plpgsql
AS $$
BEGIN
    IF OLD.voided_at IS NOT NULL THEN
        RAISE EXCEPTION 'voided rows in % cannot be changed', TG_TABLE_NAME
            USING ERRCODE = 'check_violation', CONSTRAINT = 'fees_void_is_final';
    END IF;
    IF NEW.voided_at IS NOT NULL THEN
        IF TG_TABLE_NAME = 'payments' THEN
            DELETE FROM payment_allocations WHERE org_id = NEW.org_id AND payment_id = NEW.id;
        ELSE
            DELETE FROM payment_allocations WHERE org_id = NEW.org_id AND charge_id = NEW.id;
        END IF;
    END IF;
    RETURN NEW;
END
$$;

CREATE TRIGGER charges_void_rules
    BEFORE UPDATE ON charges
    FOR EACH ROW EXECUTE FUNCTION fees_void_rules();

CREATE TRIGGER payments_void_rules
    BEFORE UPDATE ON payments
    FOR EACH ROW EXECUTE FUNCTION fees_void_rules();
