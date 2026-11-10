# Contributing

Thanks for helping. ShapeCraft aims to feel exactly like the app it follows, so the bar for UI changes is "it
looks and behaves the same side by side"; the bar for code is "a newcomer finds it where they'd look for it".

Every pull request is read and checked before it is merged, and accepted work is credited by your GitHub username
in the [CHANGELOG](CHANGELOG.md).

## Issues: what to expect

1. A first reply within about a day, with labels: what kind it is (`bug`, `enhancement`, `question`) and where it
   stands (`needs-triage`, then `accepted`, `needs-info` or `wontfix`).
2. Bugs get a test that reproduces them before the fix, so they stay fixed. Big feature requests are split into
   small steps; the issue covers the first step and the rest get their own linked issues.
3. When the change ships, we comment with the version it is in and credit you in the CHANGELOG.

Start with [`docs/architecture.md`](docs/architecture.md): the crates, the command registry, and where each kind
of change goes. Project rules are in [`AGENTS.md`](AGENTS.md) (clean room, never crash, layering).

## Build and check

See the README for OpenCASCADE. With Nix, `nix develop` (or `direnv allow`) gives the toolchain, OCCT and the window
libraries instead. Then:

```sh
cargo build
cargo test                     # all crates; OCCT tests run in parallel and very rarely flake once
cargo fmt --all                # rustfmt.toml: 140 columns, dense
cargo clippy --workspace       # unwrap/expect/panic are denied outside tests
```

UI changes: render the real UI offscreen and look at it.

```sh
cargo run -p shapecraft-snap -- SCENARIO.jsonl OUT_DIR
```

A scenario is one JSON step per line (`size`, `control` with any control-channel method, `hover`, `click`,
`right_click`, `drag`, `key`, `text`, `wait`, `shot`); see the doc comment in `apps/shapecraft-snap`. Rendering is
deterministic, so for refactors render before and after and require identical pixels.

## Conventions

- One idea per line; comments say *why* or *what was observed*, not what the code says.
- Files hold one concern and open with a `//!` line saying what lives there. Split a file when it grows past a
  screenful of unrelated things, not by line count alone.
- Colours: never a raw `Color32` for UI; use `Tokens::…()` or `theme::c(0xDARK)` so the Light theme works.
- Icons are drawn in code in `icons.rs`. No image, font or sound assets from other apps; any asset needs a row in
  `ATTRIBUTION.md`.
- Errors are `Result` with `?`; input from files, the network or the user is validated.
- Commits are small, build, and pass tests.
