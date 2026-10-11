# Final seed 664 action timeline

Report: `.runtime/lead-runtime-cleanup-seeds/branching-664/eval-results/long-horizon/runs/20261009T142816099/branching-000664/report.json`. Trace Artifact: `8896a084-7cdf-404c-be16-ddd414b63c38`.

| Turn | Action | Handles | Execution |
|---:|---|---|---|
| 1 | `create_branch` {"name": "create_branch"} | none | completed |
| 2 | `create_work_node` {"objective": "Investigate branching", "required_tool": "read_source", "title": "Investigate branching"} | none | completed |
| 3 | `search_sources` {"query": "mechanism explain outcome Y"} | none | completed |
| 4 | `read_source` {"handle": "S1"} | S1, S2 | completed |
| 5 | `read_source` {"handle": "S2"} | S2 | completed |
| 6 | `complete_work_node` {"work_node_id": "061d01ba-7ca5-4dcb-a1b3-3d19dc5ce40c"} | none | completed |
| 7 | `create_work_node` {"objective": "Investigate branching", "required_tool": "read_source", "title": "Local paper 1"} | none | completed |
| 8 | `search_sources` {"query": "mechanism explain outcome Y"} | none | completed |
| 9 | `reject_retrieved_sources` {"reason": "Read source content to evaluate its relevance to the question"} | S1, S2 | completed |
| 10 | `search_sources` {"query": "mechanism explain outcome Y"} | none | completed |
| 11 | `reject_retrieved_sources` {"reason": "Read source content to evaluate its relevance to the question"} | S1, S2 | completed |
| 12 | `search_sources` {"query": "mechanism explain outcome Y"} | none | completed |
| 13 | `reject_retrieved_sources` {"reason": "Read source content to evaluate its relevance to the question"} | S1, S2 | completed |
| 14 | `search_sources` {"query": "mechanism explain outcome Y"} | none | completed |
| 15 | `read_source` {"handle": "S1"} | S1, S2 | completed |
| 16 | `read_source` {"handle": "S2"} | S2 | completed |
| 17 | `complete_work_node` {"work_node_id": "09a350b8-80f7-45bf-b7c1-627f7e0256e5"} | none | completed |
| 18 | `create_work_node` {"objective": "Investigate branching", "required_tool": "read_source", "title": "Local paper 1"} | none | completed |
| 19 | `search_sources` {"query": "mechanism explain outcome Y"} | none | completed |
| 20 | `reject_retrieved_sources` {"reason": "Reviewing retrieved sources to determine if they address the objective of investigating branching mechanisms for outcome Y."} | S1, S2 | completed |
| 21 | `search_sources` {"query": "mechanism explain outcome Y"} | none | completed |
| 22 | `reject_retrieved_sources` {"reason": "Reviewing retrieved sources to determine if they address the objective of investigating branching mechanisms for outcome Y."} | S1, S2 | completed |
| 23 | `search_sources` {"query": "mechanism explain outcome Y"} | none | completed |
| 24 | `read_source` {"handle": "S1"} | S1, S2 | completed |
| 25 | `reject_retrieved_sources` {"reason": "Reviewing retrieved sources to determine if they address the objective of investigating branching mechanisms for outcome Y."} | S2 | completed |
| 26 | `complete_work_node` {"work_node_id": "f22f1521-c6c2-4fd6-b627-5632abf2936a"} | none | completed |
| 27 | `create_work_node` {"objective": "Investigate branching", "required_tool": "read_source", "title": "Local paper 1"} | none | completed |
| 28 | `search_sources` {"query": "mechanism explain outcome Y"} | none | completed |
| 29 | `reject_retrieved_sources` {"reason": "Read source S1 to inspect its content and determine its relevance to the question."} | S1, S2 | completed |
| 30 | `search_sources` {"query": "mechanism explain outcome Y"} | none | completed |
| 31 | `reject_retrieved_sources` {"reason": "Read source S1 to inspect its content and determine its relevance to the question."} | S1, S2 | completed |
| 32 | `search_sources` {"query": "mechanism explain outcome Y"} | none | completed |
| 33 | `read_source` {"handle": "S1"} | S1, S2 | completed |
| 34 | `reject_retrieved_sources` {"reason": "Read source S2 to inspect its content and determine its relevance to the question."} | S2 | completed |
| 35 | `complete_work_node` {"work_node_id": "ef7e5147-9d09-4a73-80ed-efb0755539f2"} | none | completed |
| 36 | `create_work_node` {"objective": "Investigate branching", "required_tool": "read_source", "title": "Investigate branching"} | none | completed |
| 37 | `search_sources` {"query": "mechanism explain outcome Y"} | none | completed |
| 38 | `reject_retrieved_sources` {"reason": "Reviewing candidates to determine if they address the objective of investigating branching."} | S1, S2 | completed |
| 39 | `search_sources` {"query": "mechanism explain outcome Y"} | none | completed |
| 40 | `reject_retrieved_sources` {"reason": "Reviewing candidates to determine if they address the objective of investigating branching."} | S1, S2 | completed |

Search is advertised before read handles exist; pending reads use actual bounded handles. Completed child source observations reach the parent. All decisions executed without invalid references, tool failures or structured repairs. The Lead repeated evidence tasks and source rejection; no FinishResearch decision occurred before the unchanged 40-call budget. This run fails the completion gate.

Parent decision turn / source-content note count: 7: 2, 18: 4, 27: 5, 36: 6.

Scenario seed varies the synthetic pack; llama.cpp receives no deterministic generation seed. Reruns are not bitwise inference replays.
