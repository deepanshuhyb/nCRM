-- When a student left, so the dashboard can show joined and left per month.
--
-- Until now "left" was only a status, with no date. `updated_at` moves on
-- every edit, so it can't answer "who left in October". The app sets
-- left_on when the status becomes 'left' and clears it when they come
-- back; the CHECK keeps the two in step whatever the app does.

ALTER TABLE students ADD COLUMN left_on date;

-- Students who had already left: the best date we have is their last edit.
UPDATE students
   SET left_on = greatest(studio_date(org_id, updated_at), joined_on)
 WHERE status = 'left';

ALTER TABLE students
    ADD CONSTRAINT students_left_on_matches_status
        CHECK ((status = 'left') = (left_on IS NOT NULL)),
    ADD CONSTRAINT students_left_after_joining
        CHECK (left_on IS NULL OR left_on >= joined_on);

CREATE INDEX students_left_on_idx ON students (org_id, left_on) WHERE left_on IS NOT NULL;
CREATE INDEX students_joined_on_idx ON students (org_id, joined_on);
