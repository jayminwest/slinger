# slinger

One window onto your computers: think with a pen on a canvas, sling anything to an agent, and get things back instead of chat.

## Principles

Read `docs/PHILOSOPHY.md` before you change any design. In short:

- Deep modules with simple interfaces. Hide each design decision in one place.
- The domain logic lives in the core library. Front ends only translate input and output.
- Define errors out of existence. The errors that remain have a stable code and a hint.
- Text output by default, `--json` for machines. Say nothing extra on success. Fail loudly.
- Ratchets only tighten. Tests verify behavior, not calls to mocks.

## Layout

- `crates/core/`: the core library (`slinger-core`). It has no terminal or process IO and no front-end dependencies.
- `crates/cli/`: the CLI (`slinger` binary). This is a thin layer over the daemon's control socket: it does not link the core. The output contract is in `docs/cli.md`.
- `plugin/`: the Claude Code plugin: the skill that tells an agent when to use slinger (`plugin/skills/slinger/SKILL.md`) and the one that makes a session take a workspace's slings (`plugin/skills/slinger-listen/SKILL.md`). `.claude-plugin/marketplace.json` lists it. The CLI ships the same skills (`slinger skill install`) and the guide it points to (`slinger guide`, from `crates/cli/src/guide.md`).
- `crates/control/`: the control protocol (`slinger-control`): the requests, the replies, the wire format and the socket client. The contract is in `docs/service.md`.
- `crates/daemon/`: the daemon (`slingerd` binary). It owns the socket, the signals and startup, and serves the core. It holds no domain logic.
- `crates/app/`: the macOS app (`slinger-app` binary). This is a thin layer over the daemon's control socket. Only `crates/app/src/platform/` holds `unsafe`, objc2 or AppKit code. `scripts/bundle.sh` builds the `.app`.
- `xtask/`: the quality gates (`cargo xtask check-all`). The budgets and the layer rules (`budgets/layer-rules.json`) are in `budgets/`.
- `ARCHITECTURE.md`: the layers, the boundaries and how to add things. Update it in the same commit as any change to a contract.

## Commands

```bash
cargo xtask check-all      # every gate: the verify command (pre-commit and CI run it too)
cargo xtask check-all --bail
cargo xtask check:size     # one gate; `cargo xtask --list` lists them
cargo test --workspace
```

`CHECK_ALL_VERBOSE=1` streams the full gate output. On failure the runner prints each failing gate's error lines and how to re-run that gate.

`scripts/e2e-sling.sh` sends three slings to the real Claude Code on a scratch daemon and checks the Spike C pass line (SPEC section 14). It is not part of `check-all`: it needs a logged-in Claude Code, spends tokens (`--model`, default `haiku`) and adds transcripts to Claude Code's session store. Run it by hand after a change to the session, the sling or the agent verbs.

## Conventions

- Run verify before every commit. Warnings fail.
- Lints are in the root `Cargo.toml`. Make exceptions with `#[expect(lint, reason = "...")]`, never a bare `allow`.
- Use typed errors with `thiserror` in the core and `anyhow` only at the binary edge.
- A layer boundary is a rule in `budgets/layer-rules.json`. Add the rule in the same commit as the boundary.
- A new verb changes four places, in this order: the core, `crates/control`, the daemon's `answer`, then each client. See "Adding a verb" in `docs/service.md`.
- A change to how agents use slinger (a new common task, a renamed verb or flag) changes `crates/cli/src/guide.md` in the same commit. The tests in `crates/cli/src/tests/guide.rs` fail on a command or flag that is gone.
- AppKit runs on the main thread only. Keep decisions in the pure modules (`scene`, `layout`) and out of `platform`.
- Keep files under 500 lines. Budgets live in `budgets/` and only tighten.
- Stop an app or daemon you started by its pid. `pkill -f target/debug/slinger-app` also kills the app that another worktree's tests are running (slinger-db4a8b).
- Every TODO/FIXME/HACK/XXX cites a tracker id (`slinger-xxxx`), an issue (`#N`) or a URL on the same line.
- Commits use conventional messages: `type(scope): summary (slinger-xxxx)`.
- CHANGELOG entries go under `## [Unreleased]`, written for users.
- Release: follow `.agents/skills/release/SKILL.md`.

<!-- seeds:start -->
## Issue Tracking (Seeds)
<!-- seeds-onboard:v0.5.15 -->
<!-- seeds-onboard-schema:7 -->

This project uses [Seeds](https://github.com/jayminwest/seeds) v0.5.15 for git-native issue tracking.

**At the start of every session**, run:
```
sd prime
```

This injects session context: rules, command reference, and workflows. Pass `--format json|compact|markdown|plain|ids` on any command for agent-friendly output.

**Quick reference:**
- `sd ready` — Find unblocked work
- `sd search <query>` — Full-text search across titles + descriptions
- `sd create --title "..." --type task --priority 2` — Create issue
- `sd update <id> --status in_progress` — Claim work
- `sd close <id>` — Complete work
- `sd dep add <id> <depends-on>` — Add dependency between issues
- `sd sync` — Sync with git (run before pushing)

### Planning
Use `sd plan` when work is large or ambiguous enough that an LLM benefits from structured decomposition. Submit spawns one child seed per step; `step.blocks` uses forward semantics (step i with `blocks: [j]` means step i blocks step j, and step j gets step i's id in its `blockedBy`).

- `sd plan templates` — List built-ins (`feature`, `bug`, `refactor`) plus custom templates
- `sd plan prompt <seed-id>` — Emit a structured prompt the LLM fills in
- `sd plan submit <seed-id> --plan <file>` — Validate + spawn child seeds
- `sd plan show <pl-id>` — View sections, children, sub-plans
- `sd plan edit <id> [--name | --section <name> <text> | --step <i> --title/--priority/--type]` — In-place field edits; bumps revision
- `sd plan outcome <pl-id> --result success|partial|failure` — Record outcome (storage-only)
- `sd plan review <pl-id> --by <name>` — Record reviewer (informational)

### Before You Finish
1. Close completed issues: `sd close <id>`
2. File issues for remaining work: `sd create --title "..."`
3. Sync and push: `sd sync && git push`
<!-- seeds:end -->
