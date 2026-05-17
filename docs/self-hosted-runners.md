# Self-Hosted Runners

**macOS** — Apple Silicon Mac. Label: `self-hosted, macOS`. Runs iOS builds and XCTests.

**Linux** — Docker container. Label: `self-hosted, Linux`. Runs Rust, Web, Android, Playwright.

## Setup

```bash
cd infra/ci-runner
cp .env.example .env          # then edit .env with your token
docker compose up -d
```

Token from https://github.com/pouriaMaleki/navon/settings/actions/runners/new.
Credentials persist in the `runner-config` Docker volume.

## Verify

```bash
gh api repos/pouriaMaleki/navon/actions/runners --jq '.runners[] | {name, status}'
```
