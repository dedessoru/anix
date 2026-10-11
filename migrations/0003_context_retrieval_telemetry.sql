ALTER TABLE context_snapshots
    ADD COLUMN retrieval_query TEXT NOT NULL DEFAULT '';
ALTER TABLE context_snapshots
    ADD COLUMN retrieval_candidate_count INTEGER NOT NULL DEFAULT 0
        CHECK(retrieval_candidate_count >= 0);
ALTER TABLE context_snapshots
    ADD COLUMN retrieval_selected TEXT NOT NULL DEFAULT '[]'
        CHECK(json_valid(retrieval_selected));
