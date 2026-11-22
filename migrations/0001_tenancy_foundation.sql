-- Tenancy foundation.
--
-- Rules every later migration must follow (see docs/TENANCY.md):
--   1. Every table that holds studio data has a NOT NULL org_id.
--   2. Row-level security is enabled on it, with a policy for ncrm_app
--      that compares org_id to app_current_org().
--   3. Child rows reference parents with (org_id, id) composite keys, so a
--      row can never point at another studio's data.
--   4. ncrm_app gets only the privileges the application needs.
--
-- The application opens every studio-scoped transaction with
--   SET LOCAL ROLE ncrm_app; SELECT set_config('app.org_id', '<uuid>', true);
-- so even a query that forgets its WHERE clause cannot cross studios.

-- The restricted role all studio-scoped queries run as. Roles are shared by
-- the whole cluster, so tolerate it already existing.
DO $$
BEGIN
    CREATE ROLE ncrm_app NOLOGIN;
EXCEPTION
    WHEN duplicate_object OR unique_violation THEN NULL;
END $$;

-- The role that runs migrations (and owns the tables) must be able to
-- switch into ncrm_app. Superusers already can.
DO $$
BEGIN
    IF NOT (SELECT rolsuper FROM pg_roles WHERE rolname = current_user) THEN
        EXECUTE format('GRANT ncrm_app TO %I', current_user);
    END IF;
END $$;

GRANT USAGE ON SCHEMA public TO ncrm_app;

-- The studio the current transaction is scoped to, or NULL when none is set.
-- NULL never equals anything, so an unscoped query sees no rows.
CREATE FUNCTION app_current_org() RETURNS uuid
    LANGUAGE sql
    STABLE
AS $$
    SELECT nullif(current_setting('app.org_id', true), '')::uuid
$$;

-- ---------------------------------------------------------------------------
-- organizations: one row per studio business (the tenant).
-- ---------------------------------------------------------------------------
CREATE TABLE organizations (
    id         uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    name       text NOT NULL CHECK (length(btrim(name)) > 0),
    slug       text NOT NULL UNIQUE
               CHECK (slug ~ '^[a-z0-9]([a-z0-9-]{0,62}[a-z0-9])?$'),
    timezone   text NOT NULL DEFAULT 'Asia/Kolkata',
    created_at timestamptz NOT NULL DEFAULT now()
);

ALTER TABLE organizations ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON organizations
    FOR ALL TO ncrm_app
    USING (id = app_current_org())
    WITH CHECK (id = app_current_org());

-- A studio can read and edit itself. Creating and deleting studios happens
-- outside studio scope (signup, support), never as ncrm_app.
GRANT SELECT, UPDATE ON organizations TO ncrm_app;

-- ---------------------------------------------------------------------------
-- branches: physical locations of a studio.
-- ---------------------------------------------------------------------------
CREATE TABLE branches (
    id         uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id     uuid NOT NULL REFERENCES organizations (id) ON DELETE CASCADE,
    name       text NOT NULL CHECK (length(btrim(name)) > 0),
    address    text,
    -- NULL means "use the organization's timezone".
    timezone   text,
    active     boolean NOT NULL DEFAULT true,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (org_id, name),
    UNIQUE (org_id, id)
);

ALTER TABLE branches ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON branches
    FOR ALL TO ncrm_app
    USING (org_id = app_current_org())
    WITH CHECK (org_id = app_current_org());

GRANT SELECT, INSERT, UPDATE, DELETE ON branches TO ncrm_app;

-- ---------------------------------------------------------------------------
-- staff: people who run the studio and sign in to the dashboard.
-- A person who works at two studios has one row in each.
-- ---------------------------------------------------------------------------
CREATE TYPE staff_role AS ENUM ('owner', 'manager', 'front_desk', 'instructor');

CREATE TABLE staff (
    id         uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id     uuid NOT NULL REFERENCES organizations (id) ON DELETE CASCADE,
    -- NULL means the person works across all branches.
    branch_id  uuid,
    -- E.164, for example +919876543210. This is how WhatsApp identifies them.
    phone      text NOT NULL CHECK (phone ~ '^\+[1-9][0-9]{7,14}$'),
    name       text NOT NULL CHECK (length(btrim(name)) > 0),
    role       staff_role NOT NULL,
    active     boolean NOT NULL DEFAULT true,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (org_id, phone),
    UNIQUE (org_id, id),
    FOREIGN KEY (org_id, branch_id) REFERENCES branches (org_id, id)
);

CREATE INDEX staff_phone_idx ON staff (phone);

ALTER TABLE staff ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON staff
    FOR ALL TO ncrm_app
    USING (org_id = app_current_org())
    WITH CHECK (org_id = app_current_org());

GRANT SELECT, INSERT, UPDATE, DELETE ON staff TO ncrm_app;

-- ---------------------------------------------------------------------------
-- audit_log: who changed what. Append-only for the application.
-- ---------------------------------------------------------------------------
CREATE TABLE audit_log (
    id             bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    org_id         uuid NOT NULL REFERENCES organizations (id) ON DELETE CASCADE,
    -- NULL when the system itself made the change (a scheduled job).
    actor_staff_id uuid,
    action         text NOT NULL CHECK (length(btrim(action)) > 0),
    entity         text NOT NULL CHECK (length(btrim(entity)) > 0),
    entity_id      uuid,
    detail         jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at     timestamptz NOT NULL DEFAULT now(),
    FOREIGN KEY (org_id, actor_staff_id) REFERENCES staff (org_id, id)
);

CREATE INDEX audit_log_org_created_idx ON audit_log (org_id, created_at DESC);

ALTER TABLE audit_log ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON audit_log
    FOR ALL TO ncrm_app
    USING (org_id = app_current_org())
    WITH CHECK (org_id = app_current_org());

-- No UPDATE or DELETE: history cannot be rewritten from the application.
GRANT SELECT, INSERT ON audit_log TO ncrm_app;
