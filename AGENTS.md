# AGENTS.md — Warp (fork: kilcer/warp)

This is a **fork** of [warpdotdev/warp](https://github.com/warpdotdev/warp). All modifications are our own. See bottom for upstream sync workflow.

Behavioral guidelines to reduce common LLM coding mistakes. Merge with project-specific instructions as needed.

**Tradeoff:** These guidelines bias toward caution over speed. For trivial tasks, use judgment.

## 1. Think Before Coding

**Don't assume. Don't hide confusion. Surface tradeoffs.**

Before implementing:

- State your assumptions explicitly. If uncertain, ask.
- If multiple interpretations exist, present them - don't pick silently.
- If a simpler approach exists, say so. Push back when warranted.
- If something is unclear, stop. Name what's confusing. Ask.

## 2. Simplicity First

**Minimum code that solves the problem. Nothing speculative.**

- No features beyond what was asked.
- No abstractions for single-use code.
- No "flexibility" or "configurability" that wasn't requested.
- No error handling for impossible scenarios.
- If you write 200 lines and it could be 50, rewrite it.

Ask yourself: "Would a senior engineer say this is overcomplicated?" If yes, simplify.

## 3. Surgical Changes

**Touch only what you must. Clean up only your own mess.**

When editing existing code:

- Don't "improve" adjacent code, comments, or formatting.
- Don't refactor things that aren't broken.
- Match existing style, even if you'd do it differently.
- If you notice unrelated dead code, mention it - don't delete it.

When your changes create orphans:

- Remove imports/variables/functions that YOUR changes made unused.
- Don't remove pre-existing dead code unless asked.

The test: Every changed line should trace directly to the user's request.

## 4. Goal-Driven Execution

**Define success criteria. Loop until verified.**

Transform tasks into verifiable goals:

- "Add validation" → "Write tests for invalid inputs, then make them pass"
- "Fix the bug" → "Write a test that reproduces it, then make it pass"
- "Refactor X" → "Ensure tests pass before and after"

For multi-step tasks, state a brief plan:

```
1. [Step] → verify: [check]
2. [Step] → verify: [check]
3. [Step] → verify: [check]
```

Strong success criteria let you loop independently. Weak criteria ("make it work") require constant clarification.

---

**These guidelines are working if:** fewer unnecessary changes in diffs, fewer rewrites due to overcomplication, and clarifying questions come before implementation rather than after mistakes.

## Project Overview

Warp is an agentic development environment / terminal emulator written in Rust. Custom UI framework (WarpUI).

## Build & Dev Commands

| Command                                                                        | Purpose                                      |
| ------------------------------------------------------------------------------ | -------------------------------------------- |
| `./script/bootstrap`                                                           | Platform-specific setup + common skills      |
| `cargo run`                                                                    | Build and run Warp locally                   |
| `cargo run --features with_local_server`                                       | Run with local warp-server                   |
| `./script/presubmit`                                                           | **fmt → clippy → tests** (run before any PR) |
| `cargo fmt -- --check`                                                         | Format check only                            |
| `cargo clippy --workspace --all-targets --all-features --tests -- -D warnings` | Full clippy                                  |
| `cargo nextest run --no-fail-fast --workspace --exclude command-signatures-v2` | Run tests                                    |
| `cargo nextest run -p warp_completer --features v2`                            | Completer tests with v2                      |
| `cargo test --doc`                                                             | Doc tests                                    |

**Toolchain**: Rust 1.92.0 (`rust-toolchain.toml`), components: `rustfmt`, `clippy`

## Project Structure

- `app/` — Main application binary + library (`app/src/lib.rs`)
  - Binary targets: `warp-oss` (default-run, `src/bin/oss.rs`), `warp` (`src/bin/local.rs`), channel-specific binaries
- `crates/` — 64 workspace crates. Key ones:
  - `warpui/`, `warpui_core/` — Custom UI framework (Entity-Component-Handle pattern)
  - `editor/` — Text editing
  - `warp_core/` — Core utilities and platform abstractions
  - `ipc/` — Inter-process communication
  - `graphql/` — GraphQL client
  - `persistence/` — Diesel ORM + SQLite, migrations in `persistence/migrations/`
  - `command/` — Command system
  - `integration/` — Integration tests

## Architecture Patterns

- **Entity-Handle system**: Views reference other views via `ViewHandle<T>`, not direct ownership
- **AppContext/ViewContext/ModelContext**: Context params named `ctx`, always **last** in function signature (exception: closure params go last)
- **TerminalModel locking**: Deadlock-prone — avoid nested `model.lock()` calls. Prefer passing already-locked refs down the stack. Keep lock scope minimal.
- **Feature flags**: Runtime `FeatureFlag::YourFlag.is_enabled()` over `#[cfg(...)]` compile-time gates when possible
- **Exhaustive matching**: No wildcard `_` in match statements unless truly necessary

## Coding Conventions

- Remove unused params entirely (don't prefix with `_`)
- Inline format args: `format!("{var}")` not `format!("{}", var)`
- Never pass `Itertools::format` to logging macros — use `iter.join(", ")` instead
- Test files: `{filename}_tests.rs` or `mod_test.rs` pattern, included as:
  
  ```rust
  #[cfg(test)]
  #[path = "filename_tests.rs"]
  mod tests;
  ```
- Do not remove existing comments when making unrelated changes

## CI & PR Workflow

- CI runs on PRs to `master` and `*_release/*` branches
- **Must pass before PR**: `cargo fmt` + `cargo clippy` + all tests (presubmit)
- CI env: `CARGO_TERM_COLOR=always`, `NEXTEST_PROFILE=ci`, `RUSTFLAGS=-C debuginfo=line-tables-only --cfg=web_sys_unstable_apis`
- Changelog format in PR description: `CHANGELOG-NEW-FEATURE:`, `CHANGELOG-IMPROVEMENT:`, `CHANGELOG-BUG-FIX:`, `CHANGELOG-IMAGE:`

## ⚠️ Fork-Specific: Upstream Sync Workflow

This repo has two remotes configured:

```
origin    git@github.com:kilcer/warp.git    (our fork — push/pull)
upstream  https://github.com/warpdotdev/warp.git  (official — pull only)
```

### Sync from upstream:

```bash
git fetch upstream
git merge upstream/master
# Resolve conflicts if any → git add → git commit
git push origin master
```

### Rules:

- **USE MERGE, NOT REBASE** on `master` (our fork is public with our commits)
- Never push to `upstream` (we don't have write access to official repo)
- Keep our modifications in commits clearly separated from upstream merges
- After conflict resolution, verify with `./script/presubmit`

## Available Project Skills

These skills live in `.agents/skills/` and can be invoked via `skill(name="skill-name")` or loaded in `task(load_skills=["skill-name"])`:

| Skill                   | Purpose                                                                                           |
| ----------------------- | ------------------------------------------------------------------------------------------------- |
| `warp-ui-guidelines`    | **Required reading before any UI work.** Catalog of Warp UI coding guidelines.                    |
| `warp-integration-test` | Write/run/debug integration tests via custom Builder/TestStep framework in `crates/integration/`. |
| `rust-unit-tests`       | Write, improve, and run Rust unit tests in the codebase.                                          |
| `add-feature-flag`      | Add a new `FeatureFlag` variant to gate code changes.                                             |
| `remove-feature-flag`   | Remove a feature flag after rollout stabilization.                                                |
| `promote-feature`       | Promote a feature-flagged feature to Dogfood/Preview/Stable.                                      |
| `create-launch-modal`   | Create one-time launch modals gated by feature flags.                                             |
| `add-telemetry`         | Add telemetry events to track behavior/system events.                                             |
| `review-pr-local`       | Review PRs (local variant of `review-pr`).                                                        |
| `triage-issue-local`    | Triage issues (local variant of `triage-issue`).                                                  |
| `dedupe-issue-local`    | Deduplicate issues (local variant of `dedupe-issue`).                                             |
| `changelog-draft`       | Generate changelog drafts from PRs in a release range.                                            |
| `classify-changelog-pr` | Reference guidance for classifying unmarked PRs for changelog.                                    |

## Testing Notes

- Nextest runner required (`cargo nextest`) — standard `cargo test` won't use parallel test execution
- `command-signatures-v2` excluded from workspace-wide test runs
- Integration tests use custom framework in `crates/integration/`
- Doc tests: `cargo test --doc`
  
  
  
  
  
  
  
