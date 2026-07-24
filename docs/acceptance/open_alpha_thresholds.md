# Open-alpha performance thresholds

Status: Frozen before the first OA-07 final run  
Scope: Windows, local SSD, repository with 5,000–25,000 tracked files, four concurrent authors

These thresholds define “practical for interactive agent work” for the initial
Windows open alpha. A final run may fail them or prompt an explicit release
decision, but the values must not be weakened after observing results.

| Measurement | Open-alpha threshold |
| --- | ---: |
| Initial ingestion | p95 not applicable; one run must finish within 120 s |
| `repository_status` | p95 <= 750 ms |
| `artifact_list` | p95 <= 1,500 ms |
| `artifact_search` | p95 <= 2,000 ms |
| `artifact_read` | p95 <= 750 ms |
| Artifact mutation | p95 <= 2,000 ms |
| Exact-view resolution | p95 <= 2,000 ms |
| MCP queue delay under four-author burst | p95 <= 2,000 ms; maximum <= 10,000 ms |
| Safe automatic contention retries | no exhausted retry; <= 8 retries for any call |
| First exact-view projection | <= 60 s |
| Cached exact-view projection | <= 5 s and <= 25% of first projection time |
| Projection storage amplification | <= 1.25x logical bytes for one cached exact view |
| Incremental native-state growth | <= 5 MiB for 20 small source operations, two executions, and two checkpoint attempts, excluding the initial content-addressed ingest |
| Full target build/test | must pass; command time is reported separately from queue and projection time |
| Four-author acceptance journey | <= 15 min from completed ingest through validated checkpoint |

The comparison baseline records ordinary local-clone time, target build/test
time, and physical repository bytes. It is evidence, not a release threshold,
because a conventional working-tree run does not provide equivalent durable
concurrency, exact-view, recovery, or provenance semantics.

Failures are not averaged away. Any correctness defect, unbounded queue,
terminal writer-contention error, false cache claim, or full projection per
author fails OA-07 even when aggregate latency remains below these limits.
