-- A discount on a fee: "₹500 off, sibling". Until now the only way was
-- to cancel the fee and add a cheaper one, which lost the link to the
-- month and to anything already paid towards it.
--
-- A discount lowers the fee's `amount_paise` in place and records how
-- much came off and why. Everything that works out what is owed
-- (amount less what has been paid towards it) keeps working unchanged,
-- because the amount it reads is already the discounted one.

ALTER TABLE charges
    -- Everything taken off so far. The price before any discount is
    -- always amount_paise + discount_paise.
    ADD COLUMN discount_paise  bigint NOT NULL DEFAULT 0 CHECK (discount_paise >= 0),
    ADD COLUMN discount_reason text CHECK (length(discount_reason) <= 200),
    ADD COLUMN discounted_by   uuid,
    ADD COLUMN discounted_at   timestamptz,
    ADD CONSTRAINT charges_discount_has_reason
        CHECK ((discount_paise = 0) = (discount_reason IS NULL)),
    ADD FOREIGN KEY (org_id, discounted_by) REFERENCES staff (org_id, id);

-- Until now the app could change nothing about a charge except cancel
-- it. It may now also lower the amount, within the rules below.
GRANT UPDATE (amount_paise, discount_paise, discount_reason, discounted_by, discounted_at)
    ON charges TO ncrm_app;

-- The rules live in the database, not only in the application, because
-- the grant above would otherwise let any code path rewrite an amount:
--   * the price before discounts never changes (what comes off the
--     amount goes onto the discount, and nothing else);
--   * a discount only lowers a fee, it can't be undone or raise one;
--   * a fee can't be discounted to less than has already been paid
--     towards it.
CREATE FUNCTION charges_discount_rules() RETURNS trigger
    LANGUAGE plpgsql
AS $$
DECLARE
    paid bigint;
BEGIN
    IF NEW.amount_paise = OLD.amount_paise AND NEW.discount_paise = OLD.discount_paise THEN
        RETURN NEW;
    END IF;
    IF NEW.amount_paise + NEW.discount_paise <> OLD.amount_paise + OLD.discount_paise THEN
        RAISE EXCEPTION 'a discount cannot change what the fee was before it'
            USING ERRCODE = 'check_violation', CONSTRAINT = 'charges_original_is_fixed';
    END IF;
    IF NEW.amount_paise >= OLD.amount_paise THEN
        RAISE EXCEPTION 'a discount can only lower a fee'
            USING ERRCODE = 'check_violation', CONSTRAINT = 'charges_discount_only_lowers';
    END IF;
    SELECT coalesce(sum(amount_paise), 0) INTO paid
      FROM payment_allocations
     WHERE org_id = NEW.org_id AND charge_id = NEW.id;
    IF NEW.amount_paise < paid THEN
        RAISE EXCEPTION 'a fee cannot be discounted below what has been paid towards it'
            USING ERRCODE = 'check_violation', CONSTRAINT = 'charges_discount_covers_paid';
    END IF;
    RETURN NEW;
END
$$;

CREATE TRIGGER charges_discount_rules
    BEFORE UPDATE ON charges
    FOR EACH ROW EXECUTE FUNCTION charges_discount_rules();
