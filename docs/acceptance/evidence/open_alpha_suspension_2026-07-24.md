# Windows open-alpha suspension

Date: 2026-07-24 (America/Chicago)  
Decision owner: Timothy Cardoza  
Event status: **Suspended pending remediation and review**  
Resolution: **Resolved; approval reinstated after OA-10 passed**

After the initial Windows open-alpha approval, an external application review
showed that automatic secret detection quarantined legitimate authentication,
setup, test, and `.env.example` source files. The resulting native view was
incomplete and could not serve as the authoritative build input.

Timothy Cardoza established the replacement product contract:

- Sunlight does not detect, hide, or block content because it resembles a
  secret. Secret prevention happens outside Sunlight.
- Git-tracked files remain visible under normal Git semantics.
- Git-ignored untracked files are excluded.
- A repository-root `.sunignore` explicitly excludes additional paths from
  Sunlight, including tracked paths.
- `.git/` and `.sunlight/` remain intrinsic implementation exclusions.

Windows open-alpha approval is suspended until this contract is implemented,
covered by source-completeness and preservation tests, independently reviewed,
and incorporated into the acceptance criteria.

Those conditions were subsequently satisfied. The implementation, independent
Sol/high closure review, adversarial coverage, complete Windows test run, and
reinstatement decision are recorded in
[OA-10 source-completeness evidence](oa10_source_completeness_2026-07-24.md).
