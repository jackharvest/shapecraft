# ShapeCraft app icon

`shapecraft.svg` is the master (full-bleed tile); `shapecraft-mac.svg` sits on Apple's icon grid with a soft
shadow. `png/` holds the rendered sizes, `shapecraft.icns` (macOS) and `shapecraft.ico` (Windows) are built from
them. Small sizes use heavier strokes and no blur so they stay crisp (see `VARIANTS` in `xtask/src/icons.rs`).

Regenerate (Rust, no other tools; the .icns needs macOS's iconutil):

```sh
cargo xtask icons /tmp/icon      # svg/ variants, png/ renders (shapecraft-N.png, mac-N.png), shapecraft.ico, shapecraft.icns
```

The SVG is the one source: the PNGs are rendered from it with resvg. Check the renders, then copy what changed here.
