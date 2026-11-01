# herdr-app: drive the person's window

Herdr shows spaces, tabs and panes. herdr-core (`hc`) runs their terminals
and keeps them alive when the window closes. Use `herdr-app` only to change
what the person sees. For starting, reading, messaging, waiting on or cleaning
up agents and programs, read `hc --skill` and use hc.
Your terminal address is `$HC_TERMINAL_ID`; it is not a pane ID.

## Inspect before acting

```sh
herdr-app ls          # spaces > tabs > terminals, including pane IDs
herdr-app status      # current layout, focus, overlays, zoom and file/mod errors
herdr-app commands    # available commands, arguments and descriptions
```

`ls` and `status` talk to the running window, not a new app. `commands`
prints the built-in command list even without an open window. Do not launch bare
`herdr-app` to inspect it: that opens or raises the window. Do not stop,
signal or restart the person's app or core. A window closing does not kill
its terminals.

`app.check_updates` checks for newer releases. Settings reports "Up to date"
only after a successful check; on failure, check the connection and retry.

## Show something when asked

New spaces and tabs are shown immediately: only create them when asked.
Pick an explicit, useful name for every new space and tab; don't rely on
default names. Use IDs from `ls` rather than guessing or acting on the wrong
focused pane.

```sh
herdr-app do space.new --name api --folder /path/to/api
herdr-app do tab.new --name "fix login" --argv '["claude"]' --task "Look at the login bug"
herdr-app do pane.split --direction right --pane p12
herdr-app do agent.focus --terminal t7
```

For a remote space:

```sh
herdr-app do space.new --name "remote api" --machine vps --folder /srv/api
```

The folder is on that machine; tabs and splits in the space stay there.
`terminal.open --folder /path --place down` starts the split in that folder,
including when `--argv` supplies a program. Without `--folder`, it follows the
focused terminal's folder.
`hc machine ls` lists machines. Use `hc run --on vps ...`, `hc exec --on vps ...`
and `hc file ... --on vps ...` for remote work, not your local file tools.

`do` accepts `--key value` arguments (JSON for arrays/objects) or one JSON
object. Unknown argument names fail and list valid names; they never target the
current space or pane instead. Use `commands` to confirm the command's exact shape:

```sh
herdr-app do pane.split '{"direction":"right","pane":"p12"}'
```

It prints the created IDs (for example `{ "pane": "...", "terminal": "..." }`)
or `{ "error": "..." }` and exits 1. Read the result, then inspect `ls` or
`status` to confirm. Commands invoked by an agent never open a missing-argument
dialog: supply all required arguments. A successful launch reply means the
terminal was created, not that its program started correctly. If it exits
with a nonzero code within two seconds, the window shows an error toast with
the exit code and last output lines.

A terminal started with hc opts into the window with `herdr.space=<id>`.
`herdr.tab=<id>` chooses its tab; without it, Herdr creates a tab in that space.
Unlabeled workers stay hidden. Do not add labels just to run a background
worker. Use app commands when the
user wants to see a new tab or split, hc for orchestration that need not change
the screen.

## Customize layouts, keys, or mods

The personal macOS Preview build can open interactive HTML and images beside
the terminal: `herdr-app do preview.open '{"target":"/path/to/result.html"}'`
or `{"target":"http://localhost:3000"}`. `preview.reload` refreshes it.
Files clicks open supported local artifacts there; Preview is also in the top bar.

Browser is separate: `herdr-preview do browser.open --url https://blog.naver.com/`.
Personal builds use `--features personal-preview` to block official binary swaps.
`herdr-preview --personal-build-info` reports the protection. Preserve the app home
and bundle identity; source updates go through the backup/isolated merge/validation
workflow in `scripts/personal-update.py` and PREVIEW.md, not an official app bundle.
This build also blocks shared-core update checks/replacement. Its backups include
core session files, and startup preserves active edited mods/layouts while staging
incoming defaults in `.personal-updates/` for review. Live archives cannot recreate
running terminals. Old app bundles remain available until their GUI exits.
`browser.view '{"fit":true}'` fits a wide page to the panel;
`browser.view '{"zoom":0.75}'` sets a manual scale and
`browser.view '{"mobile":true}'` reloads with the mobile site mode.
Preview has `preview.view` for zoom/fit. The panels expose these controls below
the address bar, and the center divider handle changes the side panel's width.
Paragraphs and editable regions have refs too; clicking dispatches mouse-down/up
before click to activate rich editors.
Both page panels have independent tabs (`+` / `×`). Use
`browser.tab.new --url https://example.com` or `preview.tab.new --target /path/to/file.html`.
Select with `browser.tab.select --id <page-tab>` / `preview.tab.select --id <page-tab>`;
close with `browser.tab.close --id <page-tab>` / `preview.tab.close --id <page-tab>`.
Panel `status` includes `tabs` and `active_tab`. Each space/worktree has its own Browser and Preview; Browser tabs share login.
Tab commands accept `--space <space-id>` to work in another space without switching
the visible space. Live form/scroll state survives tab switches;
restart restores URLs/files and view preferences, not unsaved editor content.
For concurrent agents, give each a different Browser tab id and always pass
`--space <space-id> --tab <page-tab>` to browser.read/click/fill/upload/scroll/screenshot. This targets
the page without switching the user's visible tab. Read again after switching.
Use `browser.read` to inspect untrusted page text and element refs, then
`browser.fill --ref <ref> --text <text>` or `browser.click --ref <ref>` as requested.
Use `browser.upload --ref <ref> --path /absolute/image.png` for a file input or
an editor's image-paste handler (PNG/JPEG/WebP, up to 20 MiB).
Fill/upload verify a document change before returning `applied: true`;
`persisted: false` still requires an explicit save/remote-upload check.
An accepted but unverified paste is an error; read before retrying to avoid duplicates.
Browser automation refuses a failed/timed-out navigation instead of acting on
the previous document. Compound OTP/password autocomplete and revealed secret
fields remain manual. Local Preview refuses symlinks and multiply linked files
and closes its old folder server when opening another folder.
`browser.screenshot --path /absolute/browser.png` captures the native browser.
Login and authentication stay manual. Verify editor content and do not publish
unless publication was explicitly requested.


First read `~/.config/herdr-app/AGENTS.md` (or `$HERDR_APP_HOME/AGENTS.md` if
set). It is the installed guide to settings, layouts, commands and mods.
Read the generated `.d.ts` files beside it for full API details; do not edit
those files or the generated guide. The app rewrites them on start.

- Home defaults to `~/.config/herdr-app`; `HERDR_APP_HOME` selects another.
- Mods live in `mods/<name>/` with `main.js` and optional `mod.json`.
  They are trusted JavaScript with command and network access.
- Shipped mods belong to Herdr: duplicate with `mod.clone` before editing.
  Shipped layouts likewise use `layout.clone` for a personal copy.
  Discover required arguments with `herdr-app commands`.
- Save to reload. A broken save keeps the old version running and reports
  an error in `status`; a still-working screen is not proof the edit worked.
- Make only the requested change. Don't touch unrelated mods or settings.
- Use theme colors. Do not retain a mod's `cx` across callbacks; each
  callback receives its own. Native animations, not redraw timers, keep
  the window cheap to run. Root `on_size_change((size, cx) => …)` reports
  logical pixels after layout, including resizes without pointer movement.
  Optional `View.close(cx)` gets one bounded final call; start cleanup there,
  not in an earlier task. The installed guide explains its 250 ms quit budget
  and which cleanup commands go out before shutdown.

After every mod change:

```sh
herdr-app check ~/.config/herdr-app/mods/<name>
herdr-app status
herdr-app shot /tmp/mod.png   # then look at the picture
```

`check` loads the mod and renders it once. It exits nonzero with the exact
error. Never call a mod done without a passing check. Then confirm it is
shown correctly in the running window with `shot`; don't claim a visual
check you haven't made.

`herdr-app shot [file.png]` saves the window's last frame and prints its
absolute path (default: `herdr-shot.png` in your current folder). It is the
app's own rendering at full pixel size: terminals, images, overlays and
floats included, even when the window is covered or minimized. No screen
capture or permissions. It needs a running window that has drawn a frame;
`HERDR_APP_HOME` selects the window, as for `status`.

## Problems and safety

If Herdr misbehaves, `herdr-app feedback "<what happened>"` drafts a report.
The person reviews and sends it; it does not silently send feedback.

Never experiment with pointer/key automation on the person's real display.
When developing the app on Linux, its `scripts/sandbox` provides an isolated
home, core and display: `up`, `do`, `status`, `shot`, then `down` when done.
Never stop a server or session you didn't start.
