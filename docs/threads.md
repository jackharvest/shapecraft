# Modeled threads

Select the cylindrical wall of a shaft or hole, then choose **Tools → Thread** (also in command search and the
curved-face toolbar). ShapeCraft detects External or Internal from the wall. Pick a series and size on the
Parameter Card, set the length, and choose **Done** or press Return. Escape cancels. The length arrow can be dragged,
and typing a number sets its length. Settings are editable later in History.

The 58 presets cover metric coarse M2–M24, common metric fine pitches M3–M24, and Unified inch UNC/UNF #2–1 inch.
The initial series follows the document units. Inch presets retain their inch diameter and threads per inch;
geometry and command lengths use millimeters (1 inch = 25.4 mm).

- External: start with a shaft at the preset's nominal diameter (6 mm for M6).
- Internal: start with a tapping hole, approximately nominal diameter minus pitch (5 mm for M6x1,
  5.08 mm for 1/4-20 UNC). A clearance hole is too large to thread.
- Length and Offset are measured along the selected wall's axis from its first rim. **Start at Other End** reverses
  that direction, independently of **Hand** (Right or Left).
- **Radial Clearance** shrinks the external profile or enlarges the internal profile, including its crests.
  It defaults to zero and accepts up to one quarter of the pitch. This is an allowance for fit, not a tolerance class.

Threads are real B-rep cuts and are included in solid and mesh exports. They use single-start 60° profiles with
flat roots: internal radial depth 5H/8, external 17H/24, where H = √3 P / 2. External roots are a flat approximation
at the basic minor diameter. No ISO 6g/6H or Unified 2A/2B fit, rounded root, tapered pipe thread, runout, or lead-in
chamfer is implied. Add end chamfers separately. The cut is clipped to the requested length and preserves the
rest of the body. Partial cylinders, interrupted walls, incompatible diameters, and more than 100 turns are refused.

## Commands

`thread_presets {}` lists the preset IDs, series, diameters, and pitches. `thread` accepts one selected cylindrical
face or explicit `body` and `face` IDs. It creates one undoable history step; programmatic calls open no dialogs.

```json
{"command":"thread","params":{"body":2,"face":0,"preset":"M6x1","kind":"external","length":8}}
```

```json
{"command":"thread","params":{"body":2,"face":0,"preset":"1/4-20 UNC","kind":"internal","length":6.35,"offset":0,"hand":"left","reverse":true,"clearance":0.05}}
```

The face index is topology-dependent: select or inspect the actual wall instead of assuming the example's index.
Omitting `kind` detects it from the face; `hand` defaults to `right`, and offset and clearance to zero.

## Dimension references

Preset values and profile dimensions are derived from publicly available fastener and gage data, with no copied
assets or implementation code:

- [KIPP: measurement units and metric coarse/fine pitches](https://www.kippusa.com/en-us/measurement-units)
- [Walden Gage: thread and gage reference charts](https://waldengage.com/thread-reference/)

These references describe thread dimensions. ShapeCraft's basic modeled profiles do not certify a manufactured fit.
