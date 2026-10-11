-- Preserve existing lifecycle states; scientific termination has its own typed outcome.
ALTER TABLE work_nodes ADD COLUMN outcome TEXT CHECK(outcome IS NULL OR outcome IN ('completed','inconclusive','blocked'));
