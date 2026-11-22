# How studios are kept apart

Every studio (an "organization") shares one database. The database itself
stops one studio from seeing another's data, so a bug in application code
cannot leak it.

## How it works

1. Every table with studio data has a `org_id` column that is never null.
2. Row-level security is on for every table. The policy lets the `ncrm_app`
   role touch a row only when `org_id = app_current_org()`.
3. `app_current_org()` reads the `app.org_id` setting. When it is not set the
   function returns NULL, which matches nothing, so the default is "see
   nothing".
4. The application opens studio work with `TenantTx::begin(pool, org_id)`. That
   starts a transaction, switches to the `ncrm_app` role and sets
   `app.org_id`, both for that transaction only.
5. Functions that read or write studio data take `&mut TenantTx`. They cannot
   be called with a plain connection.

## Rules for new tables

A migration that adds a table must:

- add `org_id uuid NOT NULL REFERENCES organizations (id) ON DELETE CASCADE`;
- add `UNIQUE (org_id, id)` and reference parents as `(org_id, parent_id)`,
  so a row cannot point at another studio's row;
- `ALTER TABLE ... ENABLE ROW LEVEL SECURITY`;
- create the `tenant_isolation` policy for `ncrm_app`;
- grant `ncrm_app` only the privileges the application needs.

The test `every_table_is_protected_by_row_level_security` fails if a table
misses the first, third or fourth rule.

## Work outside a studio's scope

A few things cannot run inside one studio: creating a studio at signup,
finding which studio a phone number belongs to at login, and background jobs
that sweep all studios. These use the pool directly and run as the database
owner, which row-level security does not restrict. Keep them few, name them
clearly, and never pass their results across studios.

## Tests

`crates/ncrm-core/tests/tenant_isolation.rs` proves the rules against a real
Postgres: reads, inserts, updates, deletes, moving rows between studios,
connection reuse, and the audit log being append-only.
