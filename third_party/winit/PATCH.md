# winit 0.30.13, patched

The release from crates.io (MIT or Apache-2.0, see LICENSE), with one addition for Android: winit 0.30 turns every
Android pointer into a plain `WindowEvent::Touch`, dropping which tool made it (finger, stylus, mouse, eraser) and
ignoring stylus hover, mouse buttons and the scroll wheel. ShapeCraft draws with the pen and navigates with fingers,
so it needs them.

The changes, both marked "ShapeCraft patch":
- `src/platform/android.rs`: a `pointers` module, a queue of `Record`s the app takes each frame.
- `src/platform_impl/android/mod.rs`: `record_pointers`, called first for every `MotionEvent`.

Nothing else differs from the release, and nothing changes on other platforms. Drop this folder and the
`[patch.crates-io]` entry in the workspace `Cargo.toml` once eframe uses a winit that reports pointer kinds (0.31).
