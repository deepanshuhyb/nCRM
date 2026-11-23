-- Sign-in: one-time codes and sessions.
--
-- These tables live in their own schema, `platform`, because they exist
-- before anyone is inside a studio: a phone number asks for a code before we
-- know which studio (or studios) it belongs to. The studio-scoped role
-- ncrm_app gets no access to this schema at all; only the database owner,
-- which the API uses for sign-in, can read or write it.
--
-- See docs/ARCHITECTURE.md ("Signing in") and docs/NOTES.md.

CREATE SCHEMA platform;
REVOKE ALL ON SCHEMA platform FROM PUBLIC;

-- ---------------------------------------------------------------------------
-- login_codes: one row per code sent (or per code request, see below).
--
-- A row is written for every request, even when the phone belongs to no
-- studio and nothing is sent. That keeps rate limits and timing the same for
-- known and unknown numbers, so the endpoint cannot be used to discover
-- which numbers are staff.
-- ---------------------------------------------------------------------------
CREATE TABLE platform.login_codes (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    phone       text NOT NULL CHECK (phone ~ '^\+[1-9][0-9]{7,14}$'),
    -- HMAC-SHA256 of the code, keyed with the server's AUTH_SECRET. A plain
    -- hash of a 6-digit code could be reversed by trying all million codes.
    code_hash   bytea NOT NULL,
    created_at  timestamptz NOT NULL DEFAULT now(),
    expires_at  timestamptz NOT NULL,
    attempts    integer NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    consumed_at timestamptz
);

CREATE INDEX login_codes_phone_created_idx
    ON platform.login_codes (phone, created_at DESC);

ALTER TABLE platform.login_codes ENABLE ROW LEVEL SECURITY;
-- No policies on purpose: any role other than the owner sees nothing.

-- ---------------------------------------------------------------------------
-- sessions: a signed-in browser or device.
--
-- The browser holds a random 256-bit token in a cookie; only its SHA-256 is
-- stored, so a database leak does not hand out working sessions.
--
-- org_id and staff_id are NULL until the person picks a studio. A phone that
-- works at one studio gets it picked automatically.
-- ---------------------------------------------------------------------------
CREATE TABLE platform.sessions (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    token_hash   bytea NOT NULL UNIQUE,
    phone        text NOT NULL CHECK (phone ~ '^\+[1-9][0-9]{7,14}$'),
    org_id       uuid,
    staff_id     uuid,
    created_at   timestamptz NOT NULL DEFAULT now(),
    last_seen_at timestamptz NOT NULL DEFAULT now(),
    expires_at   timestamptz NOT NULL,
    revoked_at   timestamptz,
    CHECK ((org_id IS NULL) = (staff_id IS NULL)),
    FOREIGN KEY (org_id) REFERENCES organizations (id) ON DELETE CASCADE,
    FOREIGN KEY (org_id, staff_id) REFERENCES staff (org_id, id) ON DELETE CASCADE
);

CREATE INDEX sessions_phone_idx ON platform.sessions (phone);

ALTER TABLE platform.sessions ENABLE ROW LEVEL SECURITY;
-- No policies on purpose, as above.
