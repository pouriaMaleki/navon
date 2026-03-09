# Security Policy

## Scope
This repository tracks vulnerabilities across:
- Rust workspace dependencies (`Cargo.lock` at repo root and workspace crates)
- Emulator web dependencies (`emulator/web/package-lock.json`)
- GitHub Actions dependencies (workflow action references via Dependabot + PR dependency review)

## Reporting
- For confidential reports, use GitHub private vulnerability reporting if enabled.
- Otherwise, open a security issue with minimal exploit detail and maintainers will coordinate privately.

## Triage SLO
- Critical: patch or documented mitigation plan within 48 hours.
- High: patch within 7 days.
- Medium/Low: batch in routine dependency maintenance.

## CVE Workflow
1. Dependabot opens security and version update PRs.
2. `Security Audit` workflow runs:
   - `Dependency Review` on pull requests (fail on high/critical)
   - `cargo audit` on pull requests and daily schedule
   - `npm audit` on pull requests and daily schedule
3. `CodeQL` workflow runs on pull requests and weekly schedule.
4. Findings are triaged and tracked in issues.

## Baseline and Enforcement
- Audit jobs are enforced by default.
- Optional repository variable `SECURITY_ENFORCE=false` can temporarily switch to non-blocking mode.
- Branch protection should then require these checks:
  - `Dependency Review`
  - `Cargo Audit`
  - `NPM Audit`
  - `Analyze (javascript-typescript)`
  - `Analyze (rust)`

## Risk Acceptance Process
Use the `CVE Risk Acceptance` issue template and include:
- CVE/advisory ID and affected component.
- Justification and mitigation controls.
- Explicit expiry date for the exception.
- Owner accountable for revisit and closure.
