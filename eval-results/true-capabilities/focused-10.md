# Focused true-capability regression — 2026-10-06

Fresh local Test Lead runs, with context limit 8,192. Older reports from earlier campaigns are not included.

## Branching

| Seed | Status | Child Branches | Completed Work per child | Reconciled | Restart/recovery | Integrity |
|---:|---|---:|---|---|---|---|
| 810 | passed | 2 | 3, 3 | True | 1/True | True |
| 811 | passed | 2 | 3, 3 | True | 1/True | True |
| 812 | passed | 2 | 3, 3 | True | 1/True | True |
| 813 | passed | 2 | 3, 3 | True | 1/True | True |
| 814 | passed | 2 | 3, 3 | True | 1/True | True |

Capability: **
5
/5**. Each run persisted two real child Branches, three completed WorkNodes per child, reconciled results, and passed restart and integrity audits.

## Long context

Corpus: **46,954 estimated tokens**; context limit: **8,192**. Each run exercised three retrievals, one offloaded source, and a successful range read against the same Artifact.

| Seed | Status | Retrievals | Offloaded reads | Range reads | Snapshots | Max prompt | Restart/recovery | Integrity |
|---:|---|---:|---:|---:|---:|---:|---|---|
| 820 | passed | 3 | 1 | 1 | 17 | 3143 | 1/True | True |
| 821 | passed | 3 | 1 | 1 | 16 | 3126 | 1/True | True |
| 822 | passed | 3 | 1 | 1 | 16 | 3119 | 1/True | True |
| 823 | passed | 3 | 1 | 1 | 16 | 3125 | 1/True | True |
| 824 | passed | 3 | 1 | 1 | 16 | 3130 | 1/True | True |

Capability: **
5
/5**. All five runs completed with recovery and Core integrity passing, no audit violations, no duplicate consequential operations, and no prompt above the configured context limit.

## Aggregate

- Branching: 
5
/5.
- Long-context: 
5
/5.
- Integrity: 
10
/10.
- Recovery: 
10
/10.
- The 200-run campaign was not started.
