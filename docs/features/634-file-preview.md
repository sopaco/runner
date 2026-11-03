# 634 — File preview

> Tracking issue: [#634](https://github.com/yicheng47/runner/issues/634). Scheduling and priority live on the issue and in the [roadmap](../roadmap.md). Platforms: macOS and Windows.
> Status: draft spec. Filed 2026-09-17, closed as not planned on 2026-09-22 under the "not an agent development environment" non-goal, and reopened by Jason on 2026-10-08. On 2026-11-03 it was narrowed to the file preview: the diff, commits and review to the agent moved to [#1141](https://github.com/yicheng47/runner/issues/1141), which builds on this.
> Design: `runner/desktop/specs/634-file-preview.pen` in `yeelinks_design`. Boards: `mZ6zP` (the panel with file tabs, the tree, Open in and tab overflow), `n3y8HZ` (Markdown), `s5WSq0` (file icons) and `v5dda` (where the tree starts, decided 2026-11-03).
> Related: [#458](https://github.com/yicheng47/runner/issues/458) (⌘-click file links, `file_links.rs`), [#1072](https://github.com/yicheng47/runner/issues/1072) (side-panel views), [#1141](https://github.com/yicheng47/runner/issues/1141) (the git changes view), [#403](../product/vision.md#6-non-goals-long-view) (worktree creation, still dropped).

## Motivation

Reading what an agent changed is the most common reason to leave Runner. #458 made cited paths ⌘-clickable, which covers jumping to one file but not seeing the project and what moved in it. Runner does not need to become an editor: a read-only view of the project with its changes marked lets the human judge the work without leaving the cockpit, and editing stays in the editor the user already picked.

## Scope

### Files view and its root

The Files view is a view of the side panel: beside Info in a chat's panel, and beside Sessions and Info in a mission's. Where the tree starts depends on the session (Jason, 2026-11-03):

| Session | The Files view opens on |
|---|---|
| A chat or terminal outside a project | Nothing: it asks which folder to show |
| A chat or terminal in a project | Its own checkout, from the top: the worktree its working directory is in, or the main checkout |
| A mission | Its own checkout, from the top, found the same way from the mission's working directory |
| Any session whose root was changed | The checkout or folder chosen, remembered for that chat or mission |

- **Empty start.** Outside a project a chat's folder is only the default working directory, so the view shows "Choose a folder to show" with a path field (typed, or Browse through the system picker), the user's projects and recently chosen folders. Picking one shows its tree.
- **One checkout at a time.** The tree is rooted in a single checkout, found with `git rev-parse --show-toplevel` from the working directory, so a mission in `.worktrees/feat-634-file-preview` opens with that worktree's files at the top level and its own git status. A working directory below the checkout's top opens expanded and tagged `cwd`. In the main checkout, `.worktrees/` is an ordinary gitignored folder, dimmed like `target/`.
- **The worktree picker** (after Zed's, decided 2026-11-03). The tree header's root names the checkout, with a branch icon for a linked worktree, and opens a picker: a field reading "Select a worktree or type a path…", every worktree of the repository from `git worktree list --porcelain` with its branch and short commit, the main checkout first; then recent folders, and the projects for a chat outside a project; then Choose folder…, which opens the system picker. Typing filters the list, and Enter on a path roots the tree there. Zed's "create a worktree" rows are left out: creating worktrees stays out of scope (#403).
- **Re-rooting from the tree.** Right-clicking a folder offers Show files from here, Copy path and Reveal in Finder (Show in Explorer on Windows).
- The header's changed count covers the checkout shown.
- Outside a git repository the tree is plain, with no status, and the picker lists no worktrees.

### Tree with git status

- Directories load lazily. `.git` is hidden; gitignored entries are dimmed, not hidden.
- Each changed file carries a coloured status letter from `git status --porcelain=v2 -z --untracked-files=all`: modified, added, renamed and untracked. A folder shows a dot when anything beneath it changed. Deleted files are left out until #1141 can show them as a diff.
- Live refresh from a `notify` watcher on the worktree plus the git index and HEAD, debounced.
- File icons come from open-source themes in Zed's icon-theme format: Catppuccin (default, following light and dark), Material and Minimal, chosen in Settings → Appearance → File icons.

### Read-only file viewer

- Each file opens as a tab above the file. A single click in the tree opens an italic preview tab that the next single click replaces; a double-click on the file or its tab keeps it; a file already open switches to its tab. The active tab shows ×, the others on hover; closing the last tab returns to the Files view. A changed file's tab carries its status letter.
- Opening a file widens the panel: the file takes the inner side and the tree moves to the panel's outer edge. The tree toggle hides or shows the tree, and the panel grows by the tree's width rather than squeezing the file.
- When the tabs outgrow the strip it scrolls sideways with faded edges, and a list button with the count lists every open file with its folder. ⌃Tab and ⌃⇧Tab (Ctrl+Tab and Ctrl+Shift+Tab on Windows) move between file tabs and ⌘W (Ctrl+Shift+W) closes the active one.
- Code shows line numbers and is syntax-highlighted in the terminal theme's colours, following Settings → Terminal. Binary and oversized files show a placeholder with the size. Rendering is virtualized, so a multi-thousand-line file does not stall the frame.
- Markdown opens rendered (headings, lists, code blocks, links, local images); a `</>` button switches to the highlighted source. Links open in the browser; a relative link to a project file opens it in a new tab.
- An **Open in** control shows the app the file opens in; clicking it opens the active file there at the line in view. Its chevron lists the installed editors, then the system default app and Reveal in Finder. Picking an editor is the same setting as Settings → General → Open file links in, so the control and ⌘-click on a path agree. ⌘-click on a line opens the file at that line.

### Still to design

- A terminal tab has no side panel today ([chats and windows](../guide/guides/chats-and-windows.md)); the Files view for a terminal needs that panel drawn.
- Edge states: a binary or oversized file, a file deleted while it is open, a root that no longer exists.

## Non-goals

The diff, commits and review to the agent (#1141). Editing files; create, rename, delete or move in the tree; stage, commit or push; project-wide content search; worktree creation (#403 stays closed).

## Risks

- **Large repos.** The tree walk and `git status` on a monorepo must stay off the UI thread. A build writing tens of thousands of files into an ignored `target/` must not trigger a status rerun per event, and neither must a sibling worktree's build under `.worktrees/`.
- **Git as a subprocess.** Shell out to `git` resolved through the login-shell PATH (`shell_path`, as agent spawns do) rather than adding libgit2. Windows needs `git.exe` on PATH. The picker's list is one `git worktree list` run when it opens.
- **Shared checkouts.** A session without its own worktree shares the checkout, so the status mixes the human's edits and other sessions' changes; the tree must not imply the changes are the session's.
- **New dependencies.** Syntax highlighting and Markdown rendering add the workspace's first highlighter and parser; bundled icon themes keep their licences (Catppuccin Icons MIT, Material Icon Theme MIT).
- **New rendering work.** GPUI has no tree or code view to reuse; both are new element work on a virtualized list.

## Verification

- Unit tests for the porcelain v2 parser (renames, untracked, unmerged, paths with spaces), the `git worktree list --porcelain` parser (detached and locked worktrees, paths with spaces), the folder rollup, and root resolution: outside a project, in a project, a subfolder working directory, a worktree, and a non-git folder.
- A fixture repo with modified, added, renamed, untracked, ignored and binary files: tree letters match `git status`.
- A fixture repo with a worktree under `.worktrees/`: a session there opens on the worktree with letters matching `git status` run inside it; the picker lists the main checkout and the worktree with their branches; picking the main checkout shows `.worktrees/` dimmed and without the worktree's letters.
- An agent edits a file while the panel is open: the row and the open file update within the debounce window without a manual refresh.
- A chosen root survives switching tabs and restarting the app, per chat and per mission.
- Large-repo smoke: the Runner repo with `target/` built opens the panel without a visible stall, and a `cargo build` does not churn status.
- macOS and Windows.

## Guide pages

[Chats and windows](../guide/guides/chats-and-windows.md) (the side panel's Files view and its root), [Missions](../guide/concepts/missions.md) (the mission side panel), [Appearance](../guide/guides/appearance.md) (File icons) and [Keyboard shortcuts](../guide/reference/keyboard-shortcuts.md) (file tabs).
