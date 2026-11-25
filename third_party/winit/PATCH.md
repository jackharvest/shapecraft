# winit 0.30.13, patched

The release from crates.io (MIT or Apache-2.0, see LICENSE), with one addition for Android: winit 0.30 turns every
Android pointer into a plain `WindowEvent::Touch`, dropping which tool made it (finger, stylus, mouse, eraser) and
ignoring stylus hover, mouse buttons and the scroll wheel. ShapeCraft draws with the pen and navigates with fingers,
so it needs them.

The changes, both marked "ShapeCraft patch":
- `src/platform/android.rs`: a `pointers` module, a queue of `Record`s the app takes each frame.
- `src/platform_impl/android/mod.rs`: `record_pointers`, called first for every `MotionEvent`.

Nothing else differs from the release but what is listed below. Drop this folder and the
`[patch.crates-io]` entry in the workspace `Cargo.toml` once eframe uses a winit that reports pointer kinds (0.31).

iOS (same patch, later): the `pointers` module moved to `src/platform/pointers.rs`, shared by Android and iOS
(`src/platform/ios.rs` exposes it too), and `src/platform_impl/ios/view.rs` records every `UITouch` with its type
(Pencil, finger, indirect pointer) in `handle_touches`. Also marked "ShapeCraft patch".
- `src/platform_impl/ios/view_controller.rs`: `preferredVerticalBarBehavior` (iOS 27.1) returns Disabled unless the
  app asks for the vertical bar with `platform::ios::set_vertical_bar(true)` (a static in
  `platform_impl/ios/mod.rs`; the app then calls `setNeedsUpdateOfVerticalBarConfiguration` itself). Disabled, the
  iPhone Duo shows its status bar as a row at the top; Automatic, as a column down the side (the projects screen).
- `src/platform_impl/ios/window.rs`: `inner_size` is the whole view rather than the safe area. eframe sizes the
  render surface from it while the Metal layer covers the screen, so the safe-area size stretched the picture
  vertically (1.21x on the iPhone Duo). The app keeps clear of the safe area itself.

macOS (later, #81): `src/platform_impl/macos/view.rs` `scroll_wheel` reports a precise scroll that has neither a
gesture phase nor a momentum phase as a `LineDelta` (50 points a line). That is a mouse wheel with a smooth-scrolling
driver; trackpads and the Magic Mouse always send a phase. Without it the wheel looked like a trackpad and orbited.

Android (later, #78): `record_pointers` works out which button a press or release changed from the buttons held
before and after (`pointers::changed_button`) instead of calling `MotionEvent::action_button()`. That call links
`AMotionEvent_getActionButton`, which only Android 13 (API 33) has, and the app says it runs from Android 8: the
library then failed to load on anything older. `cargo xtask android-apk` now checks every system symbol the library
needs exists at the oldest API it supports.

Android (later, #95): those records alone never drew a frame. Hover, mouse buttons and scrolling send no window event
and the app takes the records as it draws, so a pen hovering over a still view lit nothing. `handle_input_event` now
sets the redraw flag for them.

Wayland (later, #97): `src/platform_impl/linux/wayland/types/tablet.rs` binds `zwp_tablet_manager_v2` and records pens
(hover, tip, barrel buttons, eraser) as `pointers::Record`s; winit 0.30 has no tablets on Wayland. The module is exposed
by `src/platform/wayland.rs`. With `pointers::record_touches(true)`, `seat/touch/mod.rs` records fingers too instead of
sending `WindowEvent::Touch`, so an app can tell them from the mouse. `record_pointers` asks for a redraw, since the app
takes the records as it draws. Touched for it: `state.rs`, `seat/mod.rs`, `types/mod.rs`.

Wayland, KDE's global menu (#97): `types/kwin_appmenu.rs` tells the compositor where the app's menus are
(`org_kde_kwin_appmenu`) for every new window, once the app said so with `platform::wayland::set_app_menu`. Touched for
it: `state.rs`, `window/mod.rs`, `types/mod.rs`.
