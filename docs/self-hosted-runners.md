# Self-Hosted Runners

**macOS** — Apple Silicon Mac. Label: `self-hosted, macOS`. Runs iOS builds and XCTests.

**Linux** — Dev container. Label: `self-hosted, Linux`. Runs Rust, Web, Android, Playwright.

## Setup

Mac: install Xcode, xcodegen. Already configured.

Dev container: add `GITHUB_RUNNER_TOKEN=<token>` to `/work/.env`, rebuild. Token from github's URL navon/settings/actions/runners/new. Credentials survive rebuilds in `~/actions-runner/`.
