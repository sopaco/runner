# Mission Brief: Remove remote access, the hosted account, and Claude accounts

Approved by Jason on 2026-11-03 after a full codebase survey. This brief is the
authorization for the `Pair coding crew` on branch
`remove/remote-access-and-accounts` (worktree
`.worktrees/remove-remote-access-and-accounts`, PR against `origin/main`).

## Authorized scope

The crew may commit, push, and open one pull request against `main`, drive CI
green on both platforms, and run the selected live checks in the development
app under the standing [Test Scope](../../tests/README.md) authorization. No
nightly, no release, no merge. Jason reviews and merges.

Jason's decisions for this mission:

1. **Remove all three**: the relay remote-access subsystem, the hosted relay
   account it signs into, and the Claude (Anthropic) accounts subsystem.
2. **No credential cleanup migration.** Leftover `remote.json`,
   `remote-trust.json`, and OS keychain items on already-released machines stay
   where they are. Do not write startup cleanup code.
3. The roadmap's iOS companion milestone (built on the relay) is moot after
   this lands; adjust roadmap text honestly, invent nothing new.

## What stays (do not remove)

- `reqwest` — shared by usage, `runtimes/opencode/models.rs`, and others.
- `runner-core/src/companion/{messages,overview,status,actions}.rs` data-model
  types — the local app UI (`front_page/model.rs`, `project_field.rs`,
  `ops/node.rs`) uses them as its shared vocabulary. Only the pairing/crypto
  layer (`companion/channel/`) and the daemon companion server go away.
- The SQLite `_app_state` table (the `claude_accounts` key simply stops being
  written).
- `RUNNER_HANDLE` itself — session spawn, CLI env and install still set and
  read it. Only `runner remote`'s use of it disappears.
- `docs/features/archive/` and `docs/tests/archive/` records (archive policy).
- Claude Code must keep working with the user's native login after the
  accounts subsystem is gone.

## Commit plan

Five focused commits, each building and passing its crate tests:

1. `ui: remove remote access client surfaces` — runner-app only (surfaces,
   store, notifications, assets, i18n).
2. `cli: remove remote subcommand` — runner-cli only.
3. `remote: remove relay subsystem from core and daemon` — runner-core
   `src/remote/` + `companion/channel/`, protocol entries, runner-daemon
   `src/remote/` + `src/companion/`, terminal-attach branches, remote-only
   dependencies.
4. `accounts: remove the Claude accounts subsystem` — daemon
   `claude_accounts.rs` + `sharing.rs`, protocol types, app settings page,
   runtime env injection, MCP/ops env fallback.
5. `docs: remove remote access and Claude account documentation` — guide
   pages, tech deep dive, regression suite rows, READMEs, vision, arch,
   roadmap; regenerate `docs/guide/reference/cli.md` with `make guide-cli`.

i18n key removals belong to the code commits that drop their surfaces (1 and
4), not the docs commit.

## Change points (survey evidence)

### runner-app (commit 1)

Delete whole:

- `src/surfaces/remote_client/` (all, ~14.5k lines incl. `tests.rs`)
- `src/surfaces/settings/remote.rs` + `src/surfaces/settings/remote/account.rs`
- `src/surfaces/mission_workspace/remote.rs` + `remote_tests.rs`
- assets `cloud.svg`, `cloud-off.svg` (`src/assets.rs`)

Surgical:

- `front_page/client.rs`: collapse `FrontPageClient` to local-only. Remove
  `FrontPageTarget::Remote`, `RemoteAccess`, and every `is_remote()` branch in
  sidebar (`rows,state,activation,menus,drag,model`), `start_chat/launcher.rs`,
  `command_palette.rs` (`remote_palette_items`),
  `mission_workspace/{mod,state,actions,...}`. This is the largest hand-edit;
  expect 30–40 touched files.
- `app_store.rs`: drop `remote_clients` entity + subscriptions; the
  `claude_accounts/changed` event arm goes in commit 4.
- `settings/page/mod.rs`: remove `SettingsPane::Account`/`Remote` and the
  `CLOUD_PANES` group.
- `root/mod.rs`: unmount `remote_client`.
- `notifications.rs`: pairing notification + its tests.
- `app_settings.rs` (~:552): drop the legacy `localOnly`/`remoteAccessEnabled`
  ignore-and-clear block.
- i18n `en.toml`/`zh-hans.toml`: remove `remote.*` (118), `client.*` (79),
  `account.*` (17) keys in both languages.

### runner-cli (commit 2)

- Delete `src/remote.rs`; remove `Command::Remote` routing (`command.rs`
  :47-48, `validate_remote` :169) and clap defs (`command/cli.rs` :27-30).
- Tests: `tests/remote.rs` deleted; `tests/golden.rs` 8 remote help goldens;
  `tests/roundtrip.rs` RUNNER_HANDLE remote entries.

### runner-core + runner-daemon (commit 3)

Delete whole:

- `runner-core/src/remote/` (mod, client, identity, origin, status,
  `fixtures/` — 14 goldens)
- `runner-core/src/companion/channel/` (cpace, handshake, records, vectors)
- `runner-daemon/src/remote/` (mod, machine, transport, channels, client/,
  store, persistence, trust, tests)
- `runner-daemon/src/companion/` (mod, core, actions, feed, tests)

Surgical:

- `protocol/api.rs`: remove the ~19 `remote_*` daemon-API entries.
- `protocol/socket.rs`: `attach_remote` (:632) and remote deadline budgets
  (:32-52, tests :1061); `protocol/client.rs` `remote/status` + `remote/trust`
  event names (:27); `protocol/managed.rs` `Transport::attach_remote` (:295).
- `daemon/server.rs`: `core.remote` spawn wiring (:193-202), companion core
  injection (:198-202), terminal attach/input/resize `remote:{computer}:{session}`
  branching (:864-881, :973-992), remote_follow/enroll owner lifecycle
  (:572-599), shutdown (:293), embedded remote tests (:1145-1660).
- `daemon/mod.rs` :52-86: the same InProcessTransport branches.
- `lib.rs` :50: `AppCore.remote` field; `boot.rs` :100 init.
- `ops/node.rs` :623: `companion::Controller` provenance — simplify.
- `runner-core/Cargo.toml`: drop `snow`, `curve25519-dalek`, `hostname`, `url`.
- `runner-daemon/Cargo.toml`: drop `tokio-tungstenite`, `security-framework`,
  `windows-sys` `Win32_Security_Credentials` feature; check whether `rand`
  still has users after this commit.

### Claude accounts (commit 4)

Delete whole:

- `runner-daemon/src/claude_accounts.rs` (1467) + `claude_accounts/sharing.rs`
  (1545)
- `runner-core/src/protocol/claude_accounts.rs` (171)
- `runner-daemon/src/runtimes/claude_code/onboarding.rs` (157)
- `runner-app/src/surfaces/settings/claude_accounts.rs` (~1000)

Surgical:

- `protocol/api.rs` :169-171: `claude_account_usage`, `claude_accounts`,
  `claude_account_action` entries; :179-183 the four `mcp_*` ops lose the
  `claude_accounts::sharing::config_env` overlay and read config with the
  runtime shell env only — decide whether the `_with_env` helpers still earn
  their keep or fold back.
- `ops/session.rs`, `ops/skills.rs`: remove account-based sharing/env.
- `runtimes/claude_code/mod.rs` :164-231: per-account folder env injection —
  Claude Code must still launch with the native `~/.claude` config.
- `runner-app`: `app_store.rs` (`claude_accounts` state + `claude_accounts/changed`
  event + usage map), `settings/page/mod.rs`, `settings/agents/{mod,render}.rs`,
  `panes/{header,pane,mod}.rs` account pickers, `chat/mod.rs` + `archived.rs`
  `claude_account` fields, `app_shell/usage.rs`, `root/{mod,init}.rs`,
  `command_palette.rs`, `test_support.rs` (`AccountService::without_authentication`),
  and the tests in `sidebar/tests.rs`, `roles/tests.rs`.
- i18n: `claude`-related keys, both languages.
- CLI: `tests/golden.rs` claude goldens if any.

### Docs (commit 5)

Delete:

- `docs/guide/guides/remote-access.md` + `docs/guide/zh-CN/guides/remote-access.md`
- `docs/guide/guides/claude-accounts.md` + zh-CN mirror
- `docs/tech/remote-control.md`
- `docs/tests/regression/remote.md` + its row in the suite README

Update:

- `README.md` + `README.zh-CN.md` in the same PR: feature blurb (:87) and any
  other remote/account mentions; untranslated-paragraph rule with
  `<!-- TODO zh-CN -->` applies.
- `docs/guide/README.md` navigation rows for the deleted pages.
- `docs/product/vision.md` :111, :135, :141 — remote roadmap lines and
  non-goals that reference the relay.
- `docs/arch/arch.md` (~:950) and any other arch references to remote access.
- `docs/roadmap.md`: remove/adjust rows that assume the relay (iOS companion,
  remote backlog fixes); record nothing speculative.
- Regenerate `docs/guide/reference/cli.md` via `make guide-cli` (remote help
  text leaves the CLI).

## Validation

Automated gates: `make verify` locally, then CI green on macOS and Windows.
Prefer the smallest sufficient check per commit: commit 1 → `runner-app` tests
+ workspace clippy; commits 2–4 → the touched crates' tests + workspace
clippy; commit 5 → `make guide-cli` and doc link checks CI already runs.

Selected live checks (per Test Scope; the change alters what a person sees and
operates, so live checks are required. This crew has no qa slot, so Jason runs
them on the merged candidate; the PR carries `needs-live-test` until then):

1. App launches against the development daemon; no Settings entries for
   Remote access, Account, or Claude accounts; sidebar has no computer picker
   and no account avatar.
2. Create a chat, send a message, open the terminal drawer — local behavior
   unchanged.
3. A Claude Code session starts and responds using the machine's native
   Claude login (no account folder env).
4. MCP catalog opens and lists servers (config env fallback works).
5. Usage pane renders without a Claude-account section.

Jason runs these on the candidate after the clean code review, in the
development app, in fresh `/tmp` folders; the crew does not run the app.

Record which checks ran in the PR description when `needs-live-test` comes off.

## PR shape

- Labels: `core-review` (workflow adds it), `risk-high` (broad removal across
  the client protocol, daemon lifecycle and PTY boundary), `needs-live-test`
  until Jason passes the checks above.
- Clean history on top of `main`, merge-commit PR, fixes folded into their
  commits (`git commit --fixup` + autosquash, or `--amend` for the last one),
  pushed with `--force-with-lease`. No tool-specific co-author trailers.
- Stop after CI is green and the PR is open. The crew does not merge and does
  not run live checks.

## Environment note

The root checkout `/Users/bjsttlp485/Workspace/SAW/tmp/runner` is on branch
`self-use` with uncommitted changes (`.terrain/`, `script/bundle-mac`). Do not
touch the root checkout: all work happens in
`.worktrees/remove-remote-access-and-accounts`.
