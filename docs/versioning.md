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
2. Write the version's section in [`CHANGELOG.md`](../CHANGELOG.md) for someone updating from the last public
   release: a bold one-line summary, then `### New`, `### Changed` and `### Fixed`, one bold lead phrase per line
   ("**Return finishes a tool.** …"). Leave out headings that would be empty.
3. For every new feature worth showing, add a card to What's new on the home screen
   (`crates/ui-egui/src/whats_new.rs`): its version, a title, a short text, and a picture in `assets/home/` (our own
   renders or screenshots, 340 × 438, see-through around the model). Newest first.
4. Commit, then tag locally: `git tag -a v0.3.0-alpha -m "ShapeCraft Alpha 0.3.0"`.

## Internal versions and the daily push

Work is versioned as it's finished, but published at most once a day (security fixes excepted).

- Every batch of finished, verified work gets the next version on its own: `Cargo.toml`, a commit and a **local**
  tag. Fixes found while checking a version go into the next one (0.5.1, then 0.5.2), never back into a version
  that's already tagged.
- Most of these versions are never published. The day's one push publishes only the newest, so a GitHub release
  is often several versions past the previous public one. That is normal.
- `CHANGELOG.md` has one section per **public** release. People update from one public version to the next, so the
  versions in between don't get sections of their own: each batch adds its changes to the section of the version
  that will be published next, merged into its New / Changed / Fixed lists (a later change to the same thing
  replaces the earlier line rather than adding to it). Retitle the section whenever a newer version is tagged.
- Push `main` and **only the newest tag**. An older tag pushed on its own would start a release of its own.
- The release notes are that one section.

## Publishing

Nothing is pushed to GitHub, and no GitHub release is made, until the project owner says so for that day.
Pushing the tag publishes it: `.github/workflows/release.yml` builds every download and makes the GitHub release,
whose notes are the version's `CHANGELOG.md` section. Running copies find it within a day and update themselves
([updates.md](updates.md)), so a published version reaches everyone: test it first.
