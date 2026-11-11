# Android

ShapeCraft on Android phones and tablets ([#3](https://github.com/jackharvest/shapecraft/issues/3)), in two forms:

- **The app** (`ShapeCraft-<version>-arm64-v8a.apk`): the desktop interface in a `NativeActivity`
  (`apps/shapecraft-android`), with the tablet's touch model and the interface scaled to fit. A test build.
- **`shapecraft-cli`**: the command-line tools as one self-contained executable, for Termux or `adb shell`.

Both carry OpenCASCADE built by the Android NDK and linked in statically, so nothing else has to be installed.

## What you need

- A Linux or macOS host with Rust, CMake and Ninja.
- Android NDK r28 or newer. r28 aligns code for 16 KB memory pages by default, and Android 15+ devices need that.
- OpenCASCADE 7.9 source (`https://github.com/Open-Cascade-SAS/OCCT/archive/refs/tags/V7_9_3.tar.gz`).
- For the app: the Android SDK (`ANDROID_HOME`) with `build-tools;35.0.1` and `platforms;android-35` (any newer
  ones work), and a JDK for `keytool` and `apksigner`.
- For the smoke test: `adb` and a device or emulator. An x86_64 Android 11+ emulator image runs the arm64 build
  through its built-in ARM translation.

## Build

```sh
rustup target add aarch64-linux-android
export ANDROID_NDK_HOME=~/android/android-ndk-r28c OCCT_SRC=~/android/OCCT-7_9_3
cargo xtask android-occt      # ~25 min on 4 cores; only the toolkits ShapeCraft links
cargo xtask android-cli       # → target/aarch64-linux-android/release/shapecraft-cli
cargo xtask android-smoke     # runs smoke.jsonl on the adb device and checks the volumes
export ANDROID_HOME=~/android/sdk
cargo xtask android-apk       # → target/android/ShapeCraft-<version>-arm64-v8a.apk
cargo xtask android-run       # installs it on the adb device, starts it, saves target/android/screen.png
```

Add `x86_64` after each command for the emulator's own architecture.

- `android-occt` builds OCCT as static libraries for API 26 (Android 8.0) with `c++_static`. It leaves out Draw
  and every optional third-party product (FreeType, OpenGL/GLES, Tcl/Tk, FreeImage, TBB, VTK, …).
- `kernel-occt/build.rs` sees only `.a` archives in `OCCT_LIB_DIR`, so it links them statically and adds `liblog`
  for OCCT's messages. `OCCT_STATIC=1` forces this on other platforms.
- `android-cli` checks that every loadable segment of the result is aligned for 16 KB pages.

- `android-apk` builds `libshapecraft.so`, links the manifest (`AndroidManifest.xml`) and icon with `aapt2`, stores
  the library uncompressed on a 16 KB boundary so Android maps it straight from the APK, compiles the Java in
  `packaging/android/java` (javac + d8, so a JDK is needed), and signs it. Without a release key that's the debug
  key (`~/.android/debug.keystore`, made if missing); APKs signed with different keys can't update each other, so
  uninstall first if Android refuses the install.
- Releases: `cargo xtask android-release-key --upload` makes the release key once
  (`~/.android/shapecraft-release.jks` and `.pass`; back both up, a lost key means no later APK can update the
  installed app) and stores it as the repository secrets `ANDROID_KEYSTORE_B64` / `ANDROID_KEYSTORE_PASS`.
  `release.yml` then signs with it (`SHAPECRAFT_APK_KEYSTORE`, `SHAPECRAFT_APK_KEYSTORE_PASS`) and attaches
  `shapecraft-<version>-android-arm64.apk` to the release. Without the secrets the release just has no APK.

## The app

Install the APK (`adb install ShapeCraft-….apk`, or open it on the device and allow installs from that source).

- **Stylus first**, as on iPad: the pen picks, draws and drags handles (it hovers; its barrel
  button is a right-click). It has to be a stylus Android reports as a pen, such as an S Pen or a USI pen;
  rubber-tip styluses count as fingers. Until a pen is used, "Stylus may not be connected" under the project title
  says so when tapped. Fingers only move the view: one finger turns it, two pan and pinch, a double tap zooms
  to a face, a three-finger swipe left or right undoes or redoes; they press buttons but never pick or draw in the
  3D view. Without a pen, a mouse and keyboard are the way to model.
- **Mouse**: all buttons and the wheel work, as on the desktop.
- **Screen**: one point is drawn 1.21 dp, the size of an iPad point, so a tablet shows the interface at the size it
  has on an iPad; small screens shrink it to keep at least 1000 × 640 points. Phones (under 600 dp across the narrow side) run in landscape only, either way round; tablets and opened foldables turn freely. Shortcuts show only while a hardware keyboard is attached.
- **Pointers**: winit 0.30 reports every Android pointer as a plain touch, so `third_party/winit` adds the tool type,
  hover and mouse buttons (see its `PATCH.md`).
- **Keyboard**: the on-screen keyboard opens when a text field has the focus. To type a number into a tool
  (Extrude 10 ↵), a hardware or Bluetooth keyboard works today.
- **Files**: projects are kept in the app's own storage. Open, Import and Export use the system's file picker (Files,
  Drive, Dropbox, USB storage…), and the projects folder shows up in Files and every picker as "ShapeCraft"
  (`packaging/android/java`: `ShapeCraftActivity` adds the picker to the NativeActivity, `ProjectsProvider` is a
  DocumentsProvider). The app and the activity talk through small request and answer files in `files/.mailbox`, so
  the Rust side needs no JNI. The Work Folder and backup folder picks still go to
  `Android/data/com.jackharvest.shapecraft/files`.
- **Graphics**: Vulkan first, then OpenGL ES 3. `adb logcat -s ShapeCraft` shows which one started, and any error.

## Smoke test

`smoke.jsonl` makes a 60×40×10 mm box, rounds every edge (r 1 mm) and subtracts a r 5 mm through hole. It then
writes STEP and STL, reads the STEP back in, and measures both bodies. The expected volume comes from geometry,
23121.516 mm³, and both measurements must match it to a millionth.

## Trying the command-line tools on your own device

Copy `shapecraft-cli` and `smoke.jsonl` to the device, for example into Termux's home folder. Replace `OUT` in
`smoke.jsonl` with a folder you can write to, then run:

```sh
chmod +x shapecraft-cli
./shapecraft-cli run smoke.jsonl
./shapecraft-cli demo out      # another check: a filleted plate with four holes, STEP + STL in out/
```

Both `measure:` lines should report `"volume_mm3":23121.516…`.

`./shapecraft-cli mcp` serves the full command set over MCP on stdio, so scripts and assistants can model on the
device.
