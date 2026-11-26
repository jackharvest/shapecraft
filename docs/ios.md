# ShapeCraft on iPhone (in testing)

ShapeCraft runs as a native app on iPhone. On the **iPhone Duo** it's the full modeler: the same OpenCASCADE kernel
and the same interface as on a computer, laid out for the phone opened up. **Other iPhones** are a viewer: projects
and files open to turn, look at in any display mode, screenshot and export; modeling waits for a computer or a Duo.

It needs iOS or iPadOS 15.0 or newer. There's no App Store or TestFlight build. Put it on your own phone one of two ways:

## The easy way: sideload the .ipa

1. Download `shapecraft-<version>-ios-arm64.ipa` from the [latest release](https://github.com/jackharvest/shapecraft/releases/latest).
2. Open it with the sideloading tool you like ([AltStore](https://altstore.io), [Sideloadly](https://sideloadly.io)
   or similar) and install it with your own Apple account. A free account works.
3. The first time, iOS asks you to trust the developer: Settings ▸ General ▸ VPN & Device Management ▸ your Apple ID ▸
   Trust, then open ShapeCraft again.

A free Apple account's signature lasts 7 days: refresh it in your sideloading tool (AltStore can do it on its own).
Installing a newer .ipa over it keeps your projects.

## What's different on a phone

- **Other iPhones view.** Projects show upright, and a project turns the phone sideways to look at it: one finger
  turns, two pan and pinch, a double tap looks straight at a face. Display Modes, the standard views and Screenshot
  are on the right; Export is under the project's name. Open a STEP, .shapr or other file from Files to view it.
  Nothing changes the model there: new projects, tools, tutorials and Undo wait for a computer or an iPhone Duo.
- **On the Duo, fingers move the view and the stylus works the model.** One finger turns the view, two pan and pinch to zoom, a
  double tap zooms to a face, and a three-finger swipe undoes or redoes. A stylus taps, draws, drags handles, and a
  drag on empty space draws a selection box. The stylus never moves the camera.
- **iPhone Duo.** Folded, the front screen holds your projects (upright). Open it up to model on the inside screen
  (sideways). A project started on the front screen waits with an "Open up to model" card and picks up when you
  unfold. The projects screen fills iOS's vertical bar with its own buttons; in a project, the tools keep clear of the
  status corner.
- **Files.** Import and Export go through the Files app. Projects live in the app's Documents folder, which the
  Files app shows under On My iPhone ▸ ShapeCraft.
- Menus open as iOS sheets, and the interface uses the system font.

## Build it yourself

You need a Mac with Xcode 27.1 or newer, Rust (`rustup target add aarch64-apple-ios aarch64-apple-ios-sim`), CMake
and Ninja, an OpenCASCADE 7.9 source tree, and an Apple ID signed in to Xcode (a free account works).

```sh
# Your own team id (Xcode ▸ Settings ▸ Accounts) and a bundle id that's yours.
export SHAPECRAFT_IOS_TEAM=ABCDE12345
export SHAPECRAFT_IOS_BUNDLE=com.example.shapecraft

# 1. OpenCASCADE for iOS (once; the phone and the simulator each need their own).
OCCT_SRC=~/src/OCCT-7_9_3 cargo xtask ios-occt device
OCCT_SRC=~/src/OCCT-7_9_3 cargo xtask ios-occt sim

# 2. A signing profile that names your phone (plugged in by USB; UDID from `xcrun devicectl list devices`).
cargo xtask ios-provision <UDID>

# 3. Build, install and start it.
cargo xtask ios-run device <UDID>
# or, in a booted simulator:
cargo xtask ios-run sim <SIMULATOR-UDID>

# The release's sideload .ipa (ad-hoc signed; no team needed): target/shapecraft-<version>-ios-arm64.ipa
cargo xtask ios-ipa
```

Trust the developer on the phone as above. With a free Apple account, the signing profile lasts 7 days. After that, run step 2 and 3 again to keep using it
(your projects stay).

## Driving the simulator from scripts

A simulator build listens on the control channel (see [mcp.md](mcp.md)) when it's started with
`SIMCTL_CHILD_SHAPECRAFT_CONTROL_PORT=<port>`. The simulator shares the Mac's network, so scripts on the Mac reach it at
`127.0.0.1:<port>`. Builds for a phone never listen.
