# ShapeCraft app icon

`shapecraft.svg` is the master (full-bleed tile); `shapecraft-mac.svg` sits on Apple's icon grid with a soft
shadow. `png/` holds the rendered sizes, `shapecraft.icns` (macOS) and `shapecraft.ico` (Windows) are built from
them. Small sizes use heavier strokes and no blur so they stay crisp (see `VARIANTS` in `gen/icon_svg.py`).

Regenerate (needs Python with Pillow and NumPy):

```sh
python3 gen/icon_svg.py out/svg      # SVG variants
python3 gen/icon_png.py out/png      # PNG renders (shapecraft-N.png, mac-N.png)
# .icns: copy mac-N.png into ShapeCraft.iconset as icon_NxN[@2x].png, then: iconutil -c icns ShapeCraft.iconset
```
