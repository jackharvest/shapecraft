# Task benchmark

How close ShapeCraft is to the reference, measured instead of estimated. Each task is a workflow a user of the reference does,
driven through the real UI offscreen by `shapecraft-snap` (keys, clicks on model points, typed values, the same
control channel the app serves) with checks on the model it leaves behind.

```sh
cargo xtask bench-tasks                            # writes bench/tasks/*.jsonl (only files that changed)
shapecraft-snap --bench bench/tasks                # runs them all, writes bench/RESULTS.md and results.json
shapecraft-snap --bench bench/tasks --only hist-   # just the History tasks
```

Build `shapecraft-snap` in release with `CARGO_TARGET_DIR=~/.cache/shapecraft-target` (see plan/HANDOFF.md).

## Rules

- **Expected values come from geometry or from observing the reference**, never from reading our own output back. Where
  the reference's behaviour wasn't observed, the task says so in its description and accepts either reading.
- **A task for something we don't have stays in and scores 0.** A command we lack ("unknown command") or a
  parameter a command ignores ("unknown parameter": the benchmark sets `SHAPECRAFT_STRICT_PARAMS=1`, which makes
  the engine refuse keys it never reads) fails the whole task, not just one check.
- **Pick faces and edges the way a user does.** Clicks on model points (`{"click": [x, y, z]}`), or
  `"@face:BODY:x,y,z"` / `"@edge:BODY:x,y,z"` references that find the face or edge at that point when the step runs.
  Raw face and edge numbers change whenever the kernel rebuilds a body, so they only belong on a freshly made box.
- **Scores are usage-weighted.** `categories.json` weighs sketching, modeling and History, where people spend
  their time, above Visualization and Drawings. What a script can't drive (how navigation feels, icon weight) comes
  in as hand-scored entries with the reason written next to them.

## Writing a task

`xtask/src/bench.rs` holds every task as a list of steps (the `shapecraft-snap` scenario format) plus checks:

| Step | Meaning |
|---|---|
| `{"expect": {...}}` | `bodies`, `visible_bodies`, `volume` (or `[value, rel. tol]`), `volumes`, `volume_between`, `area`, `faces`, `bounds`, `sketches`, `regions`, `history`, `history_has`, `failed_steps`, `selection`, `tool`, `banner`, `file` |
| `{"control": …, "returns": {...}}` | check a control call's answer: `{"at": "/pointer", "near": n}`, `"is"`, `"min"`, `"same"` / `"neg"` (another pointer) |
| `{"control": …, "save": {"name": "dr", "at": "/drawing"}}` | keep a returned value; `"$dr"` in a later step puts it in |
| `$TMP` | a scratch folder for files a task writes and reads back |

Mark a task `allow_errors` (`task_with(…, true)`) only when the action itself is meant to be refused.

## Second benchmark: real `.shapr` projects

Projects made by the reference's users are the stricter test: import each one, compare bodies, volumes, names, colours and
the History list with what the reference shows, then edit steps in the middle of the history and check the model rebuilds.
The corpus lives in `plan/shapr-corpus/` (git-ignored; public GitHub repositories, most without a licence, so used
for local interoperability testing only and never committed or redistributed). Sources and per-file results:
`plan/HANDOFF.md`, session 27.

First results (2026-11-01, 8 projects, 12–426 MB, the reference releases from early 2025 to mid 2026):

| Measure | Before this session | Now |
|---|---:|---:|
| Projects that open at all | 0 / 8 | 8 / 8 |
| Live bodies whose geometry converts (mean over projects) | 0% | ≈ 62%, then ≈ 96% (2026-11-01, later) |
| The reference History steps that come across as steps | 0 of ≈ 1,950 | all listed under the Import step, not rebuilt |
| Steps editable after import | 0 | 0 |

The reference itself brings the whole parametric History of an imported project across (hundreds of Movement/Rotation,
Face Offset, Extrusion, Fillet, Boolean, Deletion steps); that is the largest parity gap the corpus found.
