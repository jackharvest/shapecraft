# Versions and releases

ShapeCraft numbers its builds `MAJOR.MINOR.PATCH`, with `-alpha` on tags while it is in alpha
(`v0.3.0-alpha`). The number lives in one place, `[workspace.package] version` in the root `Cargo.toml`; the
About box, Preferences, crash logs, the MCP server and the Linux bundle's name all read it from there.

| Part | Goes up when | Example |
|---|---|---|
| MAJOR | ShapeCraft leaves alpha, then beta. Stays 0 for a long time. | 0 → 1 |
| MINOR | A batch of new functionality lands: a feature the reference app has that ShapeCraft didn't, or a roadmap item that finally works in the app. One bump per batch (an update), not one per feature. | 0.2.0 → 0.3.0 |
| PATCH | Fixes and polish to things that already exist. | 0.3.0 → 0.3.1 |

A MINOR bump resets PATCH to 0. A batch that mixes features and fixes is a MINOR bump.

## Each version

1. Set the version in `Cargo.toml`.
2. Add the version's section to [`CHANGELOG.md`](../CHANGELOG.md): what's new (one line per feature), then what was
   fixed.
3. For every new feature worth showing, add a card to What's new on the home screen
   (`crates/ui-egui/src/whats_new.rs`): its version, a title, a short text, and a picture in `assets/home/` (our own
   renders or screenshots, 340 × 438, see-through around the model). Newest first.
4. Commit, then tag locally: `git tag -a v0.3.0-alpha -m "ShapeCraft Alpha 0.3.0"`.

## Publishing

Nothing is pushed to GitHub, and no GitHub release is made, until the project owner says so for that version.
Pushing the tag publishes it: `.github/workflows/release.yml` builds every download and makes the GitHub release,
whose notes are the version's `CHANGELOG.md` section. Running copies find it within a day and update themselves
([updates.md](updates.md)), so a published version reaches everyone: test it first.
