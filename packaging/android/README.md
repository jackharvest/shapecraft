# Android (command-line tools)

The first step towards ShapeCraft on Android tablets ([#3](https://github.com/jackharvest/shapecraft/issues/3)):
`shapecraft-cli` cross-compiled for `aarch64-linux-android` (and `x86_64-linux-android` for emulators), with
OpenCASCADE built by the Android NDK and linked in statically. The result is one self-contained executable with no
`.so` files beside it. There is no app or touch interface yet.

## What you need

- A Linux or macOS host with Rust, CMake and Ninja.
- Android NDK r28 or newer. r28 aligns code for 16 KB memory pages by default, and Android 15+ devices need that.
- OpenCASCADE 7.9 source (`https://github.com/Open-Cascade-SAS/OCCT/archive/refs/tags/V7_9_3.tar.gz`).
- For the smoke test: `adb` and a device or emulator. An x86_64 Android 11+ emulator image runs the arm64 build
  through its built-in ARM translation.

## Build

```sh
rustup target add aarch64-linux-android
export ANDROID_NDK_HOME=~/android/android-ndk-r28c OCCT_SRC=~/android/OCCT-7_9_3
cargo xtask android-occt      # ~25 min on 4 cores; only the toolkits ShapeCraft links
cargo xtask android-cli       # → target/aarch64-linux-android/release/shapecraft-cli
cargo xtask android-smoke     # runs smoke.jsonl on the adb device and checks the volumes
```

Add `x86_64` after each command for the emulator's own architecture.

- `android-occt` builds OCCT as static libraries for API 26 (Android 8.0) with `c++_static`. It leaves out Draw
  and every optional third-party product (FreeType, OpenGL/GLES, Tcl/Tk, FreeImage, TBB, VTK, …).
- `kernel-occt/build.rs` sees only `.a` archives in `OCCT_LIB_DIR`, so it links them statically and adds `liblog`
  for OCCT's messages. `OCCT_STATIC=1` forces this on other platforms.
- `android-cli` checks that every loadable segment of the result is aligned for 16 KB pages.

## Smoke test

`smoke.jsonl` makes a 60×40×10 mm box, rounds every edge (r 1 mm) and subtracts a r 5 mm through hole. It then
writes STEP and STL, reads the STEP back in, and measures both bodies. The expected volume comes from geometry,
23121.516 mm³, and both measurements must match it to a millionth.

## Trying it on your own device

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
