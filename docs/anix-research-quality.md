# Research quality evaluation protocol

Execution completion, safety/recovery and scientific quality are separate measurements. The current synthetic long-horizon loop does not validate final scientific synthesis and therefore cannot establish production scientific accuracy. Historical completion totals are not transferable to the corrected explicit-termination harness.

## Dataset and artifact requirements

Freeze each input corpus and SHA-256, question, scope, exclusions, expected evidence, contradictions and uncertainty. Include evidence-rich, contradictory, duplicate, sparse, theoretical counterexample, computational experiment and interrupted-work cases. Use genuinely held-out questions and independent human annotation. Synthetic pack `tests/research-packs/catalog.json` is a contract fixture, not an independent research benchmark.

Save the final synthesis as an immutable Artifact, its source references and exact supporting ranges/quotes, Claim/Relation/Provenance records, model profile, ContextSnapshots and the complete visible action trace. A read/search count does not substitute for these outputs. Offloaded content must be consumed through verified ranges when needed.

## Separate scoring

| Check | Required evidence | Failure |
|---|---|---|
| Citation validity | Every cited Source/Paper exists in the project, was accessible and content was inspected | Invented, excluded or unrelated reference |
| Claim -> Evidence | Exact content supports each factual statement; quote/range and provenance are preserved | Snippet treated as proof; unsupported extrapolation |
| Contradictions | Conflicting measurements/populations/time windows are identified and scoped | Contradictory evidence omitted without explanation |
| Unsupported findings | Findings link to completed experiment attempts and actual results | Execution success mistaken for scientific truth |
| Final synthesis | Answers the actual question, distinguishes facts/inference and offers an inspectable output | Generic completion message or missing argument |
| Uncertainty | Sparse evidence and failed experiments produce explicit bounded uncertainty | Missing evidence represented as a positive outcome |
| Strategy flexibility | Literature-free proof, existing-source analysis, experiment and replan are tested as applicable | Forced universal Search -> Read -> Hypothesis pipeline |

Record workflow completion separately from scientifically acceptable conclusions and explicit inconclusive outcomes. Audit safety, integrity and recovery for every run regardless of quality score. A qualified independent reviewer should adjudicate disputed scientific results; another model's confidence is not a deterministic validator.

The 200-run campaign is still gated by known-seed and mixed readiness. This document prepares the later quality evaluation; it does not claim that evaluation has been executed.
