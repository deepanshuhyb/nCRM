-- Fee reminders and broadcasts: the two ways a studio writes first on
-- WhatsApp. Both go out as approved templates (WhatsApp delivers free
-- text only within 24 hours of the person's last message), through the
-- same `messages` outbox as everything else.

-- ---------------------------------------------------------------------------
-- Fee reminders: on unless a studio turns them off.
-- ---------------------------------------------------------------------------
ALTER TABLE organizations ADD COLUMN fee_reminders boolean NOT NULL DEFAULT true;
-- 0009 made `organizations` updatable column by column. A studio may
-- change this one itself.
GRANT UPDATE (fee_reminders) ON organizations TO ncrm_app;

-- Which reminders have been made, so each is made once: for a charge
-- there is at most one "3 days before", one "on the day", one "3 days
-- late" and one "7 days late". The row is written in the same
-- transaction as the message, so a worker that runs twice, or two
-- workers at once, can't send the same reminder twice.
CREATE TABLE fee_reminders_sent (
    id         uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id     uuid NOT NULL REFERENCES organizations (id) ON DELETE CASCADE,
    charge_id  uuid NOT NULL,
    -- Days from the due date: -3, 0, 3 or 7.
    step       smallint NOT NULL CHECK (step IN (-3, 0, 3, 7)),
    -- The message that carried it. NULL when there was nobody to tell
    -- (the student has no main contact): recorded anyway, so we don't
    -- look again every five minutes.
    message_id uuid,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (org_id, id),
    UNIQUE (org_id, charge_id, step),
    FOREIGN KEY (org_id, charge_id) REFERENCES charges (org_id, id) ON DELETE CASCADE,
    FOREIGN KEY (org_id, message_id) REFERENCES messages (org_id, id) ON DELETE SET NULL (message_id)
);

ALTER TABLE fee_reminders_sent ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON fee_reminders_sent
    FOR ALL TO ncrm_app
    USING (org_id = app_current_org())
    WITH CHECK (org_id = app_current_org());

-- A record of what was done: added to, never edited.
GRANT SELECT, INSERT ON fee_reminders_sent TO ncrm_app;

-- ---------------------------------------------------------------------------
-- Broadcasts: one message to many people, written by staff.
-- ---------------------------------------------------------------------------
CREATE TABLE broadcasts (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id         uuid NOT NULL REFERENCES organizations (id) ON DELETE CASCADE,
    -- One paragraph: WhatsApp refuses a template value with a line break
    -- or a tab in it.
    body           text NOT NULL CHECK (length(btrim(body)) BETWEEN 1 AND 600 AND body !~ '[\n\r\t]'),
    -- Who it was for, as chosen ({"kind":"batch","batch_id":"…"}), and
    -- the same in words for the history ("Hip-hop Beginners").
    audience       jsonb NOT NULL,
    audience_label text NOT NULL CHECK (length(audience_label) BETWEEN 1 AND 120),
    -- How many people it was queued for.
    recipients     integer NOT NULL CHECK (recipients BETWEEN 1 AND 1000),
    created_by     uuid NOT NULL,
    created_at     timestamptz NOT NULL DEFAULT now(),
    UNIQUE (org_id, id),
    FOREIGN KEY (org_id, created_by) REFERENCES staff (org_id, id)
);

CREATE INDEX broadcasts_recent_idx ON broadcasts (org_id, created_at DESC);

ALTER TABLE broadcasts ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON broadcasts
    FOR ALL TO ncrm_app
    USING (org_id = app_current_org())
    WITH CHECK (org_id = app_current_org());

-- Sent is sent: a broadcast can't be reworded or removed afterwards.
GRANT SELECT, INSERT ON broadcasts TO ncrm_app;

-- Each recipient's copy is an ordinary message in their conversation,
-- pointing back at the broadcast so its deliveries can be counted.
ALTER TABLE messages
    ADD COLUMN broadcast_id uuid,
    ADD FOREIGN KEY (org_id, broadcast_id) REFERENCES broadcasts (org_id, id);

CREATE INDEX messages_broadcast_idx ON messages (org_id, broadcast_id) WHERE broadcast_id IS NOT NULL;
