# Security Policy

## Scope
This repository tracks vulnerabilities across:
- Rust workspace dependencies (`Cargo.lock` at repo root and workspace crates)
- Emulator web dependencies (`device/emulator/web/package-lock.json`, which is the canonical JS security lockfile)
- Android companion dependencies (`companion-apps/android` Gradle dependency graph via GitHub dependency submission + GitHub security advisories/review)
- GitHub Actions dependencies (workflow action references via PR dependency review and repository security tooling)
- Future iOS third-party dependency manifests, which must add repository CI coverage in the same change that introduces them

## Reporting
- For confidential reports, use GitHub private vulnerability reporting if enabled.
- Otherwise, open a security issue with minimal exploit detail and maintainers will coordinate privately.

## Triage SLO
- Critical: patch or documented mitigation plan within 48 hours.
- High: patch within 7 days.
- Medium/Low: batch in routine dependency maintenance.

## CVE Workflow
1. `Security Audit` workflow runs:
   - `Dependency Review` on pull requests (fail on high/critical)
   - `cargo audit` on pull requests and daily schedule
   - `npm audit` on pull requests and daily schedule
   - dependency-manifest guard on pull requests and daily schedule
2. `Android Dependency Submission` workflow runs on pull requests and pushes to `main`:
   - resolves `companion-apps/android` Gradle dependencies
   - uploads/submits the dependency graph so GitHub security features can track Android advisories
3. `CodeQL` workflow runs on pull requests and weekly schedule.
4. Findings are triaged and tracked in issues.

## Baseline and Enforcement
- Audit jobs are enforced by default.
- Optional repository variable `SECURITY_ENFORCE=false` can temporarily switch to non-blocking mode.
- Branch protection should then require these checks:
  - `Dependency Review`
  - `Cargo Audit`
  - `NPM Audit`
  - `Dependency Manifest Coverage`
  - `Android Dependency Graph`
  - `Analyze (javascript-typescript)`
  - `Analyze (rust)`

## Risk Acceptance Process
Use the `CVE Risk Acceptance` issue template and include:
- CVE/advisory ID and affected component.
- Justification and mitigation controls.
- Explicit expiry date for the exception.
- Owner accountable for revisit and closure.
