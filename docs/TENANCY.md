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
finding which studio a phone number belongs to at sign-in, turning a session
cookie into a studio context, background jobs that sweep all studios, and
finding which studio an incoming WhatsApp message is for (from the number it
was sent to).
These use the pool directly and run as the database owner, which row-level
security does not restrict. Keep them few, name them clearly, and never pass
their results across studios. The current list is in
`docs/ARCHITECTURE.md` ("Queries that look across studios").

## Columns a studio must not change about itself

`organizations` is the one studio table whose row *is* the studio, and
some of its columns decide things across studios. So `ncrm_app` may update
only named columns of it (`name`, `timezone`, the fee settings,
`bot_mode`), not the whole row.

The one that matters most is `wa_phone_number_id`, the studio's WhatsApp
number. An incoming message is routed to whichever studio holds the number
it was sent to. If a studio could write that column, it could claim another
studio's number and receive their parents' messages. Only the operator sets
it, with `ncrm-admin whatsapp set` (which connects as the database owner),
and a unique index means one number can't belong to two studios.

When adding a column to `organizations`, decide which side it is on. If a
studio should be able to change it from the dashboard, add it to the
`GRANT UPDATE (...)` list in a migration. If not, leave it out and it is
read-only from inside a studio. The test
`only_the_operator_can_give_a_studio_a_number` shows the refusal.

Data that belongs to no studio (sign-in codes, sessions) lives in the
`platform` schema, not `public`. `ncrm_app` has no access to that schema at
all.

## Tests

`crates/ncrm-core/tests/tenant_isolation.rs` proves the rules against a real
Postgres: reads, inserts, updates, deletes, moving rows between studios,
connection reuse, and the audit log being append-only.
`crates/ncrm-core/tests/sign_in.rs` checks that the studio role cannot read
the sign-in tables, and `crates/ncrm-api/tests/sign_in_api.rs` checks
through the HTTP API that a signed-in person only ever sees their current
studio.

Why foreign keys include `org_id`, and why the role switch is
transaction-local: see `docs/NOTES.md`.
