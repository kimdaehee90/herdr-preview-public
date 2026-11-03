# Paginated Artifact Preview Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** View PPT/PPTX, PDF and explicitly selected PNG sequences in Herdr Preview with page counts and direct page navigation.

**Architecture:** Prepare a read-only document snapshot in a private temporary directory, convert PPT through a private LibreOffice profile, and render PDF pages on demand through macOS PDFKit. Native Preview controls select a page; asynchronous generation tokens prevent stale work replacing a newer document. The existing loopback server shows verified page images.

**Tech Stack:** Rust, GPUI, Objective-C/PDFKit, existing Wry, LibreOffice headless.

**Spec:** `docs/superpowers/specs/2026-11-03-document-preview-design.md`

## Global Constraints

- Preserve `personal-preview`, `dev.herdr.preview`, existing app home and WebKit profile.
- Keep all document processing local and original files unchanged.
- PNG collections contain only explicitly selected files, in selection order.
- Never restart the shared herdr-core or quit the user's running GUI during development.
- Reject symlinks, hardlinks and invalid/out-of-range page input; retain the last verified page after a failed jump.
- Render requested pages only, with bounded pixel dimensions/cache and finite conversion timeout.

## Review Focus

- Switching source during PPT conversion must never restore stale output (Task 2 generation check).
- Selected PNGs with identical basenames from different folders must stay distinct (Task 1 snapshot test).
- Closing a tab during work must clean only that job's temporary files/process (Task 1 cancellation test, Task 2 lifecycle review).
- A failed conversion must not leave the old document labeled as the new file (Task 2 native smoke).
- Reloading a shortened PDF must clamp the last page without losing the selected source list (Task 2/3 smoke).

### Task 1: Local document preparation and rendering

**Files:** Create `src/preview/document.rs`; modify `src/preview.rs`, `Cargo.toml`; test in the new module.

**Interfaces:** Produces `Document::prepare(paths: &[PathBuf], cancelled: &AtomicBool) -> Result<Self,String>`, `Document::render(page: usize) -> Result<PathBuf,String>`, `page_count() -> usize`, `root() -> &Path`, and `parse_page(text: &str, total: usize) -> Result<usize,String>`. Document is Send/Sync and owns its private temporary snapshot.

- [x] Write failing tests for page bounds, ordered explicit PNG selections, duplicate basenames, malformed PDF and multipage PDF rendering.
- [x] Run focused tests and confirm failure because the document implementation is absent.
- [x] Implement secure snapshots, PDFKit page count/render, private headless PPT conversion, cancellation/timeout and bounded rendering/cache.
- [x] Run focused tests and real converter fixtures; expect correct counts, nonempty PNG output and original bytes unchanged.
- [x] Commit the document module and tests.

### Task 2: Native Preview controls and multi-file opening

**Files:** Modify `src/preview/macos.rs`, `src/preview/tabs.rs`, `src/preview.rs`, `src/window.rs`, `src/commands.rs`.

**Interfaces:** Consumes Task 1's Document; produces `Preview::open_selection(paths: Vec<PathBuf>, window, cx)` and optional `preview.open` `targets` array plus `preview.view` `page`. Existing `target`, zoom and fit remain supported. Status adds `page`, `pages`, `files` and `document_loading`.

- [x] Add failing tests for supported targets and page selection validation; extend native fixture assertions for opening explicitly selected PNGs.
- [x] Run and confirm the old implementation lacks these behaviors.
- [x] Integrate background preparation/rendering, cancel stale generations, store explicit source lists per Preview tab and reload/clamp the page.
- [x] Add native previous/next buttons, page input and total count; allow multiple files in the picker and reject mixed multi-selections.
- [x] Verify image/PDF paging, invalid input, switching tabs/sources, reload and normal HTML/Browser behavior in an isolated debug app.
- [x] Commit native integration.

### Task 3: File routing, documentation and native validation

**Files:** Modify `defaults/mods/files/main.js`, `defaults/AGENTS.md`, `PREVIEW.md`, `scripts/preview-smoke.py`, generated command docs as required.

**Interfaces:** Files opens PPT/PPTX with the existing `preview.open` command. The file picker is the primary multi-PNG entry point; the CLI's explicit targets array provides equivalent deterministic testing.

- [x] Add regression fixtures for a three-page PDF/PPTX and separately labeled PNGs.
- [x] Enable office file routing and document usage/errors/dependency requirements.
- [x] Run Rust suite, existing JS/Python/docs checks and isolated macOS Preview/Browser smoke tests; expect all required checks green.
- [x] Build the protected personal release, perform a fresh whole-branch review and resolve important findings with regression tests.
- [x] Commit tested changes and prepare the existing validated installer/backups; hand over only any required GUI-close step after the build is ready.
