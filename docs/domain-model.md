# Domain model

Project is the research ownership boundary. All entities carry ProjectId, UTC timestamps, a positive optimistic version, and an archive marker. Every principal identity has a separate UUID newtype. Query and command APIs use those types rather than string identifiers.

Source is a physical/logical locator; Paper is a publication. `paper_sources` links a Paper to one or more sources. Claim references a Paper, optionally one of that Paper's sources, and a location with page/section/paragraph/span/offset/table/figure. Source membership is validated in the application and enforced by foreign keys.

Hypothesis belongs to an optional ResearchQuestion. Its typed statuses are Draft, Proposed, Active, Supported, Rejected, Inconclusive, and Archived. Domain policy validates every status change. Semantic revisions and status changes accept an expected version.

Experiment stores design, objective, methodology, and protocol. ExperimentRun stores a specific execution attempt, its runtime Run, timestamps, input/output artifacts, and metadata. A human explicitly activates an experiment before starting an attempt. Completion atomically updates the attempt, experiment, Run, and Task. Failed experiments can be reactivated for another attempt. A generic Run command cannot finish an active experiment attempt.

Finding references a completed ExperimentRun and stores a structured title, summary, and optional confidence. Artifact records a normalized relative workspace path, SHA256, byte size, kind, and metadata. File bytes stay outside SQLite. The Workspace adapter computes hashes; application registration preserves this metadata.

Relations use typed EntityRef endpoints and RelationKind. Claim/Finding can support, contradict, or qualify a Hypothesis; Experiment tests a Hypothesis; Hypothesis addresses a Question; Paper contains a Claim; ExperimentRun produces Findings/Artifacts. Uses, DerivedFrom, BasedOn, References, RelatedTo, and Supersedes have domain compatibility rules. Cross-project endpoints and self-relations are rejected. Duplicate active relations return the existing identity.

`trace_evidence` returns support/contradiction/qualification groups with the Claim or Finding, publication and sources, execution lineage, relation, and provenance. It performs deterministic data retrieval. Archived records retain their identity and audit history.

Task stores durable intent; Run stores an attempt. Retry creates a new Run for a failed Task. Runtime statuses include Running, WaitingForHuman, Completed, Failed, and Cancelled; parent_run_id supports hierarchical execution. Parent runs cannot finish while children remain active. Gate resolution completes a waiting proposal Run once it has no remaining pending gates or active children.

Gates store a versioned ActionEnvelope, reason/risk, expected versions, affected entities, proposer identity, optional Run, decision actor/time, and applied result. A human command applies directly or decides a gate; agent semantic commands use GatePolicy. Repeated approval returns the saved result, repeated rejection returns the existing decision, and conflicting decisions fail.

Action receipts bind a unique ActionId to the exact envelope and result. Reusing the same ID/payload returns the previous result. Reusing the ID with another payload fails. Hard deletion is absent from the application API; archive preserves evidence and provenance.
