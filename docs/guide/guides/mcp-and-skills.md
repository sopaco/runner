# MCP servers and skills

Use Settings to inspect the tools and instructions available to your agents. Runner reads each runtime's own configuration, and supported switches change that configuration for new sessions inside and outside Runner.

## MCP servers

1. Open **Settings → MCP** and select an agent.
2. Search for a server and inspect its command or URL, transport and registration.
3. Toggle its registration for the selected runtime, or open its entry to edit the native JSON or TOML.
4. Launch a new session to pick up the change.

Runner writes only the named server entry. Enabling a server for an agent can copy an existing supported definition from another agent; disabling unregisters it from the selected config. If a transport cannot be copied, add it with that agent's tooling. Use the agent's own tooling to create a new server or rename one. A same-named server may have different definitions in different agents, so inspect the selected agent's entry before editing.

| Runtime | Default user MCP configuration |
| --- | --- |
| Codex | `~/.codex/config.toml` |
| Claude Code | `~/.claude.json` |
| Antigravity CLI | `~/.gemini/config/mcp_config.json` |
| GitHub Copilot CLI | `~/.copilot/mcp-config.json` |
| TRAE CLI | `~/.trae/traecli.toml` |
| Grok | `~/.grok/config.toml` |
| OpenCode v2 | `~/.config/opencode/opencode.jsonc` |

For Claude Code, the native sign-in uses `~/.claude.json`, with its skill catalog and switches in `~/.claude/skills/` and `~/.claude/settings.json`.

These are defaults; runtime environment settings can change some paths. `~` means your home directory on either platform. Use the path displayed by Runner for the actual file. Cursor and pi do not have MCP management in Runner.

## Skills

Open **Settings → Skills**, choose an agent, and select a skill to read it. Hover a row to edit the underlying skill document. **Refresh** reloads the catalog after changes made elsewhere.

Global on/off switches are available for three runtimes:

| Runtime | What the switch changes |
| --- | --- |
| Codex | A `[[skills.config]]` entry in `~/.codex/config.toml` |
| Claude Code | `skillOverrides` in `~/.claude/settings.json` |
| GitHub Copilot CLI | `disabledSkills` in `~/.copilot/settings.json` |

Other supported runtimes show a catalog and allow reading and editing skill files, but Runner does not provide a global on/off switch for them. Editing a skill changes the shared file itself, which may also be read by other agents.

The catalog is not an exhaustive list of every instruction the agent loads. For example, Codex system and plugin skills, Claude Code bundled and project skills, and Copilot project and plugin skills are not listed. The selected runtime's caption explains its skill roots and exclusions.

## Runner's own skill

**Settings → General → Command line → Runner skill for agents** controls installation of Runner's command guide for detected agents. Mission coordination uses the bundled `runner` CLI; you do not need to register Runner as an MCP server to start a mission.

See [Drive Runner from agents](drive-runner-from-agents.md) for that workflow, and [Troubleshooting](troubleshooting.md) if a change does not appear in a running session.
