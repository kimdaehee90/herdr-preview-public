# Herdr Preview: PPT, PDF and image page navigation

Status: accepted in conversation, with explicit multi-selection of PNG files.

## Intent and agreed behavior

The user means the Preview panel of the installed personal Herdr Preview app,
not macOS Preview. They requested PPT viewing, a current/total page indicator,
and a way to jump directly to a page, with the same controls for PDF and PNG.
They confirmed static PPT slides are sufficient; animation is unnecessary.

## Scope

- Open local `.ppt`, `.pptx`, `.pdf` and `.png` through Files and Preview's file
  picker or path input.
- Show a common previous button, editable 1-based page number, total page count
  and next button in the existing Preview panel. Enter commits the page number.
- First/last page buttons cannot navigate out of range. Reject blank, nonnumeric,
  zero, negative and out-of-range input without changing the displayed page.
- Keep existing zoom, width fit, artifact tabs, space isolation and local reload.
- Treat a single PNG as one page. Explicitly selected PNG files form one image
  sequence in selection order; do not include unselected files from the folder.
- Opening another document begins at page 1. Switching an already open tab keeps
  its page. Reload keeps the current page when possible and clamps if it shrinks.
- Preserve original files and keep conversion/rendered output local.

Not included: editing slides, animations, presenting speaker notes, cloud
conversion, changing Browser behavior, or restarting/updating herdr-core.

## Existing implementation

Source: `~/.local/share/herdr-preview-public`, personal feature `personal-preview`.
The macOS Preview is `src/preview/macos.rs`, using a native WKWebView for artifacts
and a separate persistent Browser mode. `src/preview/server.rs` serves local
files read-only on loopback. Preview tabs are managed by `src/preview/tabs.rs`.
Files routes selected HTML/image/PDF extensions to `preview.open` in
`defaults/mods/files/main.js`. The current toolbar has history, reload and zoom,
but no document pagination or office-file conversion.

The installed personal app and its configuration live separately from the older
Herdr TUI. Keep `dev.herdr.preview`, the current app home and WebKit profile.

## Approaches

1. Convert PPT locally to PDF, then use one paginated document/image view.
   This was recommended in conversation and the user accepted static slides and
   requested PDF/PNG alongside it. It covers legacy PPT as well as PPTX and avoids
   implementing slide layout. Tradeoff: a local PPT converter is required; fonts
   missing on the machine can affect line breaks.
2. Interpret PPTX directly in JavaScript. This avoids a local office converter but
   adds a second rendering engine, incomplete PPTX layout, and no legacy PPT.
   It is a poor fit for viewing existing presentation files accurately.

Proceed with approach 1. For implementation, use LibreOffice's local headless
conversion for PPT/PPTX, with a separate temporary profile per conversion. Its
presence must be detected; installation is a necessary dependency if missing.
Do not automate the user's interactive PowerPoint documents.

## Rendering and integration (builder's proposed choices)

Use macOS PDFKit for PDF page count and rendering. Link the system framework
through the existing Objective-C dependencies, avoiding an additional PDF engine.
Render requested PDF pages to high-resolution PNG in the background and display
through the existing read-only loopback Preview server/WKWebView. This is a static
viewer; PDF text selection/search is outside this change's scope. A single PNG
uses the same page model and controls without PDF conversion.

Put document preparation/rendering in a focused `src/preview/document.rs` module.
Keep document state local to Preview: source identity, page count, current page,
prepared PDF/image list, private output directory and in-flight generation token.
Preparation and conversion never block the UI thread. Show a loading state, do not
leave a prior document presented as the newly opened one, and discard stale work
when switching source/page/tab. Cache a bounded number of rendered pages at a
sensible DPI and pixel limit; large documents must not render every page upfront.
Do not claim completion from a successful subprocess launch: verify nonempty PDF,
page count and successfully decoded/rendered page output.

PPT conversion has a finite timeout, captures a bounded actionable error, and
cleans up only its own subprocess/profile/output. Cancellation must not target
other LibreOffice, PowerPoint or Herdr processes. Keep the generated directory
private, reuse the existing server's no-symlink/no-hard-link checks, and never
serve the user's whole home or add a page-to-terminal bridge.

The native toolbar owns current/total pages and jump input. Existing history
arrows remain history; document previous/next are distinct controls. Existing
fit/zoom operates on the page image. Document controls appear for paginated
artifacts and remain absent for HTML/website previews and Browser.

Update Files' supported extensions and Preview messages. Enable multiple selection
in the Preview file picker. Accept a multi-selection only when all files are PNG;
preserve picker order and exclude hidden/link files. Different folders may be
combined only when the person explicitly selects those files.

Update `defaults/AGENTS.md` and `PREVIEW.md` to explain document controls,
converter requirements and the single-image/sequence semantics. Extend existing
Preview status/view options only if necessary to inspect and test page state;
keep existing consumers compatible. No changes to herdr-core or its shared model.

## Failure behavior

Unreadable/malformed files, encrypted PDFs, unsupported office content, missing
converter, conversion timeout and rendering failure show a clear error in Preview.
Do not silently fall back to page 1, reuse stale output, or modify the original.
A failed page jump keeps the last verified page and exposes the error.

## Validation and rollout

Before product implementation, the user approves this design. Work in an isolated
branch/worktree. Test meaningful behaviors: page-input validation and boundaries,
PDF page count/render dimensions and ordering, PPT/PPTX conversion, cancellation
and stale generation handling, reload clamping, and optional natural PNG ordering.
Use self-contained multipage PDF/PPTX and PNG fixtures with distinct visible labels.
Run the repository's required Rust/docs checks and the isolated macOS Preview and
Browser smoke tests, plus new native page-navigation checks. Test separately from
the active app/profile/core; never send synthetic input to the user's real desktop.

Build with the personal feature and keep update protection intact. The existing
installer requires backup, validated source/binary identity and the personal GUI
to be closed. Prepare and validate the result first. Do not quit/restart the user's
active app or core without explicit authorization; if the app must be closed to
install, hand over that concrete final step only after the build is verified.

## Open item

The user selected explicitly chosen multiple PNGs, not automatic folder grouping.
All scope is grounded in the user's request. The PDFKit renderer, bounded
page cache, LibreOffice converter and native toolbar are builder proposals.
