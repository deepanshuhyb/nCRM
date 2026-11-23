-- Join dates in the studio's own calendar, and backdating.
--
-- Until now students.joined_on defaulted to CURRENT_DATE, which is the
-- database server's date (UTC). Between midnight and 5:30 am in India
-- that's still yesterday. The application now always sets joined_on
-- itself, from the studio's date or a date staff typed in (backdating).

-- The date a moment fell on, where the studio is. Used to show whether a
-- date was backdated: a join date earlier than the day it was entered.
-- Runs as the caller; under ncrm_app, row-level security on organizations
-- means it only answers for the current studio (NULL for any other).
CREATE FUNCTION studio_date(org uuid, at timestamptz) RETURNS date
    LANGUAGE sql
    STABLE
AS $$
    SELECT (at AT TIME ZONE timezone)::date FROM organizations WHERE id = org
$$;

-- Repair join dates written by the old default. joined_on couldn't be set
-- by anyone before this migration, so every existing value is the
-- default: the UTC date of created_at. Put it in the studio's calendar.
UPDATE students s
   SET joined_on = (s.created_at AT TIME ZONE o.timezone)::date
  FROM organizations o
 WHERE o.id = s.org_id
   AND s.joined_on = (s.created_at AT TIME ZONE 'UTC')::date;

-- No default any more: a missing join date is a bug, not "today in UTC".
ALTER TABLE students ALTER COLUMN joined_on DROP DEFAULT;

-- Backdating has a floor, so a typo (2O15, 1015) can't produce nonsense.
ALTER TABLE students
    ADD CONSTRAINT students_joined_on_check CHECK (joined_on >= DATE '2000-01-01');
