# ShapeCraft – instructions for agents

ShapeCraft is a clean-room, open-source, local-first direct-modeling CAD app in Rust. "The reference" below means the
commercial direct modeler whose behaviour we observe (black-box only) and whose `.shapr` files we read and write.
It runs on Linux, macOS and Windows. No accounts, no cloud, no seats, no limits.

## Start every session here
1. Read `plan/STATUS.md` (current milestone, next task).
2. The behavior reference is `plan/shapr3d/` (`01-observed-ui.md` = measured observations of the running app,
   `04-menu-tree-*.txt` = menu trees with shortcuts). `plan/` is gitignored (local only).
3. Library decisions: `docs/adr/`.
4. Lessons from the ArtCraft apps: `plan/RESEARCH.md`.

## Golden rule: it must feel like the reference
The GUI and UX feeling extremely close to the reference is the most important feature, and it outranks feature breadth.
- Every UI change is checked against the reference: screenshot ours (`ui.screenshot` on the control channel),
  compare with `plan/shapr3d/screenshots/`, and fix the differences in layout, spacing, color, motion and flow.
- Match the interaction model, not just the look: contextual toolbar, live handles, type-a-number-anytime,
  camera animation to sketch planes, hint banner, selection colors, shortcuts (`04-menu-tree-workspace.txt`).
- If an observation is missing, go observe it (drive the reference with `plan/tools/`) before guessing.
- **Side by side in the built app.** Building through the control channel or MCP skips what a person sees, so
  every larger parity pass also includes hand-built comparisons: the release app and the reference in two windows of the
  same size, the same part made in both with the same real clicks and keys, both captured at every step (what
  stays visible, labels and pills on handles, the left column, the bottom row, the status bar, the camera). Read
  state from ours with `ui.state` / `inspect` to tell a wrong picture from a wrong model. Check a difference
  against the reference before fixing it: an old note in the code can be wrong, and so can a first impression.
- Leave out what we don't do: teams, Upgrade/edu banners, Share, Published Versions, Spaces, cloud sync,
  tutorial videos. Local-first stand-ins are noted in `01-observed-ui.md`.

## Clean room (non-negotiable)
- The reference may be **observed** black-box: run it, use it with scratch projects, screenshot it, read its menu
  names through accessibility. Never read, disassemble or copy anything inside the reference's app bundle beyond file and
  framework names. Never commit screenshots of it or files produced by it.
- Don't touch the owner's existing projects in the reference. Use scratch projects only.
- **No assets from the reference, ever**: icons, images, fonts, materials, HDRIs, sounds, help text. Icons are drawn in code.
  Any file asset needs a row in `ATTRIBUTION.md` (author, source, license).
- File formats come from public specs (STEP via OCCT, STL, 3MF, OBJ, glTF). Parasolid XT is read and written from
  the vendor's published "Parasolid XT Format Reference" (crates/xt; no Parasolid code). `.shapr` is learned only
  from files we made ourselves in the reference, for interoperability (crates/shapr); its samples and notes stay in the
  gitignored `plan/` (plan/shapr-format.md) and are never committed.

## Architecture rules
- **Everything is a command.** Every user-visible action is a registered command (id, label, menu path, shortcut,
  JSON params, run). Menus, toolbar, command search, scripts, CLI, the control channel and MCP all reach the same
  registry. Programmatic calls never open dialogs.
- **The kernel is replaceable.** All geometry goes through the `shapecraft-kernel` traits. OCCT lives only in
  `kernel-occt` (C++ shim + dynamic link). Nothing else may include OCCT headers or types.
- **Layering:** geom → kernel → doc → engine → ui-egui / mcp → apps. Nothing below the UI depends on egui/wgpu.
- **Agent control from day one:** `--control PORT` JSON-lines channel and an MCP server (headless or bridged to the
  running app).

## Never crash
- No `unwrap`/`expect`/`panic!`/`todo!`/`unreachable!` in non-test code. Errors are `Result` with `?`.
- `unsafe` only inside `kernel-occt`'s FFI boundary. Every OCCT call is wrapped in C++ `try/catch` (including
  `Standard_Failure`) and returns an error code, so C++ exceptions never cross into Rust.
- Command execution runs under `catch_unwind`, and a failure restores the document.
- Input-derived numbers (files, MCP params, typed values) are hostile: validate, check for NaN, cap sizes.

## Files
- Never write over an imported file. Export always goes to a new path the user picked.

## Git
- The repository is public. Commit after each piece of work that builds and passes tests; push and release only
  when the maintainer says so.
- Finished batches get their own version and local tag; only the day's one push reaches GitHub
  ([docs/versioning.md](docs/versioning.md) "Internal versions and the daily push"). The internal checklist,
  including the visual check of every change in the reference and in our built app, is `plan/PROCESS.md`.
- Public text (commits, docs, About box) names no AI tools and no personal names. Commit as the repository's
  configured identity, with no co-author trailers.
