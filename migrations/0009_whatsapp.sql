-- WhatsApp conversations: one thread per phone number per studio, and
-- every message in it (theirs, the bot's, staff's).
--
-- Follows the checklist in docs/TENANCY.md.

-- ---------------------------------------------------------------------------
-- Each studio has its own WhatsApp number. Meta identifies a number by a
-- "phone number id"; an incoming message says which id it was sent to, and
-- that is how it finds its studio.
--
-- Whoever holds a number's id receives that number's messages, so a studio
-- must not be able to set it for itself (it could claim someone else's).
-- The app role loses UPDATE on the table as a whole and gets it back for
-- named columns only; the number is set by the operator (ncrm-admin).
-- ---------------------------------------------------------------------------
CREATE TYPE bot_mode AS ENUM ('off', 'menu', 'ai');

ALTER TABLE organizations
    ADD COLUMN wa_phone_number_id text UNIQUE
        CHECK (wa_phone_number_id ~ '^[0-9]{5,25}$'),
    -- The number as people see it, for display only.
    ADD COLUMN wa_display_phone text CHECK (length(wa_display_phone) <= 24),
    -- How messages are answered: not at all, with a fixed menu, or by AI
    -- (which falls back to the menu when no AI key is configured).
    ADD COLUMN bot_mode bot_mode NOT NULL DEFAULT 'menu';

REVOKE UPDATE ON organizations FROM ncrm_app;
GRANT UPDATE (name, timezone, fees_from, fee_due_day, bot_mode) ON organizations TO ncrm_app;

-- ---------------------------------------------------------------------------
-- conversations
-- ---------------------------------------------------------------------------
CREATE TABLE conversations (
    id               uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id           uuid NOT NULL REFERENCES organizations (id) ON DELETE CASCADE,
    -- E.164. Matched to `contacts.phone` when shown; not a foreign key, so
    -- a number we have never heard of can still write to the studio.
    phone            text NOT NULL CHECK (phone ~ '^\+[1-9][0-9]{7,14}$'),
    -- The name WhatsApp shows for them. Unverified: display only.
    profile_name     text CHECK (length(profile_name) <= 120),
    last_message_at  timestamptz NOT NULL DEFAULT now(),
    -- WhatsApp lets a business write freely for 24 hours after the person
    -- last wrote; after that only approved templates go through.
    last_inbound_at  timestamptz,
    unread           integer NOT NULL DEFAULT 0 CHECK (unread >= 0),
    -- Someone on the team should look at this.
    needs_staff      boolean NOT NULL DEFAULT false,
    -- Staff have taken over: the bot stays quiet until then.
    bot_paused_until timestamptz,
    created_at       timestamptz NOT NULL DEFAULT now(),
    UNIQUE (org_id, id),
    UNIQUE (org_id, phone)
);

CREATE INDEX conversations_recent_idx ON conversations (org_id, last_message_at DESC);

ALTER TABLE conversations ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON conversations
    FOR ALL TO ncrm_app
    USING (org_id = app_current_org())
    WITH CHECK (org_id = app_current_org());

GRANT SELECT, INSERT, UPDATE ON conversations TO ncrm_app;

-- ---------------------------------------------------------------------------
-- messages
-- ---------------------------------------------------------------------------
CREATE TYPE message_direction AS ENUM ('in', 'out');
CREATE TYPE message_author AS ENUM ('contact', 'bot', 'staff', 'system');
-- in:  received
-- out: queued -> sending -> sent -> delivered -> read, or failed
CREATE TYPE message_status AS ENUM ('received', 'queued', 'sending', 'sent', 'delivered', 'read', 'failed');

CREATE TABLE messages (
    id              uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id          uuid NOT NULL REFERENCES organizations (id) ON DELETE CASCADE,
    conversation_id uuid NOT NULL,
    direction       message_direction NOT NULL,
    author          message_author NOT NULL,
    -- Set when a person on the team wrote it.
    staff_id        uuid,
    body            text NOT NULL CHECK (length(body) BETWEEN 1 AND 4096),
    -- Outgoing template messages (reminders): the approved template's name
    -- and the values for its placeholders. `body` holds the readable text.
    template        text CHECK (length(template) <= 100),
    template_params jsonb,
    status          message_status NOT NULL,
    -- WhatsApp's id for the message. Incoming ones can arrive twice (Meta
    -- retries); the unique index below makes the second a no-op.
    wa_message_id   text CHECK (length(wa_message_id) <= 200),
    error_code      integer,
    attempts        smallint NOT NULL DEFAULT 0,
    -- The wall clock, not now(): now() is the same for every row written
    -- in one transaction, and a thread must keep the order things were said.
    created_at      timestamptz NOT NULL DEFAULT clock_timestamp(),
    sent_at         timestamptz,
    CONSTRAINT messages_shape CHECK (
        (direction = 'in' AND author = 'contact' AND status = 'received' AND staff_id IS NULL)
        OR (direction = 'out' AND author <> 'contact' AND status <> 'received'
            AND ((author = 'staff') = (staff_id IS NOT NULL)))
    ),
    UNIQUE (org_id, id),
    FOREIGN KEY (org_id, conversation_id) REFERENCES conversations (org_id, id) ON DELETE CASCADE,
    FOREIGN KEY (org_id, staff_id) REFERENCES staff (org_id, id)
);

CREATE UNIQUE INDEX messages_wa_id_idx ON messages (org_id, wa_message_id) WHERE wa_message_id IS NOT NULL;
CREATE INDEX messages_thread_idx ON messages (org_id, conversation_id, created_at);
CREATE INDEX messages_queue_idx ON messages (org_id, created_at) WHERE status IN ('queued', 'sending');

ALTER TABLE messages ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON messages
    FOR ALL TO ncrm_app
    USING (org_id = app_current_org())
    WITH CHECK (org_id = app_current_org());

-- What was said can't be rewritten: only the delivery state changes.
GRANT SELECT, INSERT ON messages TO ncrm_app;
GRANT UPDATE (status, wa_message_id, error_code, attempts, sent_at) ON messages TO ncrm_app;
