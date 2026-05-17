# Self-Hosted Runners

**macOS** — Apple Silicon Mac. Label: `self-hosted, macOS`. Runs iOS builds and XCTests.

**Linux** — Docker container. Label: `self-hosted, Linux`. Runs Rust, Web, Android, Playwright.

## Setup (Linux)

```bash
cd infra/ci-runner
GITHUB_RUNNER_TOKEN=<token> docker compose up -d
```

Token from https://github.com/pouriaMaleki/navon/settings/actions/runners/new.
Credentials persist in the `runner-config` Docker volume across rebuilds.

## Verify

```bash
gh api repos/pouriaMaleki/navon/actions/runners --jq '.runners[] | {name, status}'
```
