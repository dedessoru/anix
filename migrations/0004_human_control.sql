CREATE TABLE human_control (
    project_id TEXT PRIMARY KEY NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    state TEXT NOT NULL CHECK(json_valid(state))
);
