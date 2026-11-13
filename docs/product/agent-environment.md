# Runner as an environment for agents

> Direction set by Jason on 2026-09-23, reconciled with shipped behavior on 2026-10-22. The agent-first framing and mission vocabulary are reflected in [`vision.md`](vision.md); this note retains the rationale and the remaining delegation steps.

## The shift

Until now the person has driven Runner: they build a crew, start a mission, and the agents take part in it. From here the main caller of Runner is an agent. A Claude Code session starts a mission, spawns a Codex worker into it, waits for the result and stops the worker, without the person clicking anything. Runner becomes the place where agents from different providers spawn, find and coordinate with each other, and the app is the environment they live in.

Most of the groundwork is already done. The 0.11.0 CLI made agents first-class callers: one binary, the caller identified by a roster handle, the coordinator verbs `spawn`, `ps`, `wait`, `stop` and `done` reserved, and MCP removed so there is one way in. This doc names the direction so later decisions can be checked against it.

## What stays: the app is where the person watches

Agents being the main callers does not make the app a thin viewer. The windows, tabs, panes and the real terminal stay core to the product. Every session an agent spawns is a real PTY that the person can open in a pane, read, type into and take over, exactly like one they started themselves. Humans and agents share the same sessions; they just get to them differently (the person through the UI, agents through the CLI).

This is the second ceiling in [`vision.md`](./vision.md) §1 applied: the more agents that spawn without the person, the more the person relies on the terminal layout and tab management to follow them. Work on the terminal and on tabs keeps its priority.

## Why Runner and not the vendors

Claude Code's subagents spawn Claude and Codex's spawn Codex. No vendor has a reason to let its agent hand work to a competitor's as an equal. Being neutral about providers is the one position the vendors will not take, and it is Runner's position already ([`vision.md`](./vision.md) §2).

## What the environment provides

| Need | How Runner meets it | State |
| --- | --- | --- |
| A place to work together | The mission as a container that owns its roster, seeded from a crew or started empty | Shipped through 0.16.2 in [#562](https://github.com/yicheng47/runner/issues/562) |
| Identity | Roster handles; outside agents take a seat | 648 decisions shipped; seats in [#916](https://github.com/yicheng47/runner/issues/916) |
| Coordination | The mission bus: signals, messages, the inbox | Shipped |
| Handing off directly | A chat uses the CLI to start an empty mission and add a worker; the return path to that chat needs a design | Shipped in [#562](https://github.com/yicheng47/runner/issues/562) (0.16.2); the return path in [#748](https://github.com/yicheng47/runner/issues/748) is on hold after Runner typing updates into a normal chat was rejected on 2026-10-17; [#704](https://github.com/yicheng47/runner/issues/704)'s `runner session send` closed unbuilt on 2026-10-11 |
| Living past the app | Sessions continue in runnerd when the app quits ([`vision.md`](./vision.md) §4.2) | [#645](https://github.com/yicheng47/runner/issues/645), shipped in 0.13 |
| Planned limits | `MISSION_SPAWN_CAP` (8) and one level of spawning; permissions granted per role or slot | With the verbs in [#916](https://github.com/yicheng47/runner/issues/916) |
| Reaching the person | Every member asks the person directly with `runner ask` / `ask_human`; desktop notifications | Shipped; [#701](https://github.com/yicheng47/runner/issues/701); `ask_lead` retired in [#562](https://github.com/yicheng47/runner/issues/562) Phase 3 |
| Keeping track | Plan usage is shipped; the token ledger is unscheduled; the desktop Active view is separate | [#706](https://github.com/yicheng47/runner/issues/706), [#630](https://github.com/yicheng47/runner/issues/630), [#887](https://github.com/yicheng47/runner/issues/887) |

The proposed cap and one-level rule belong to #916; they are not a claim that the current CLI implements the reserved coordinator verbs. Today the person controls membership, including when an authorized outside agent operates the CLI for them.

## What it does not provide

The non-goals in [`vision.md`](./vision.md) §6 stand. Runner does not create worktrees, edit files, perform git writes or sandbox agents. Read-only file and diff review is planned in [#634](https://github.com/yicheng47/runner/issues/634). The agent brings its own tools, and a crew that needs its own checkout makes one from its brief. Runner provides the place, the identities, the channels, the limits and the way to reach the person, and nothing more.

## The test

The direction holds if this works end to end without the person touching anything:

1. A Claude Code chat, open in a tab, starts an empty mission or takes a seat in one.
2. It spawns a Codex worker with a task.
3. The worker's session appears in the app, where the person can watch it but is not interrupted.
4. The chat waits, reads the worker's report from its inbox, and stops the worker or closes the mission.

Steps 1 and 2 work today from outside the mission: #562 shipped empty missions and add-role, so the chat starts a mission and adds a worker with its task. Seats, `spawn`, `wait` and `done` are [#916](https://github.com/yicheng47/runner/issues/916), and [#748](https://github.com/yicheng47/runner/issues/748) brings the report back into the chat for step 4. Step 3 was settled in #562's Phase 3: the mission appears in the sidebar without taking focus.

## Open questions

1. **Where does a session an agent spawned show up?** In a new tab, in the mission's tab as a pane, or kept in the sidebar until the person opens it. It must not take focus from what the person is doing. Settled 2026-10-15 in #562's Phase 3: the mission appears in the sidebar and does not take focus, as a mission started from the CLI does today; only the app's own Start mission dialog opens the mission it starts.
2. **Does a direct chat get a way to spawn workers without a mission?** No, settled 2026-10-11 when #704 closed into #562: direct chats stay off the bus, and a chat that needs workers starts a mission. Revisit only if that step turns out to be friction in real use.

## Roadmap implications

The [roadmap](../roadmap.md) owns release scheduling. The session host and #562's mission delegation have shipped; reliable delivery (#907) shipped in 0.16.5, and delegation's remaining follow-through is #748 and per-role or per-slot verbs (#916). File review follows. Historical milestone numbers from the original September direction are superseded by the roadmap.
