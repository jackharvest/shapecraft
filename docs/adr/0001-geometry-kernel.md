# ADR 0001: Geometry kernel

Date: 2026-10-19 · Status: accepted

## Context
The leading commercial direct modelers are built on Parasolid. We need a B-rep kernel with robust booleans, fillets, chamfers,
shells, offsets, lofts, sweeps and STEP/IGES exchange. The pure-Rust options (truck, Fornjot) don't have
production-quality fillets or booleans yet. Writing our own kernel would cost more than the rest of the app.

## Decision
- Use **OpenCASCADE Technology (OCCT) 7.9**, LGPL-2.1 with the OCCT exception, as the first kernel.
- It sits behind our own traits in `crates/kernel` (no OCCT types). The only crate that touches OCCT is
  `crates/kernel-occt`: a small C++ shim built with the `cc` crate, linked **dynamically** against the system/
  bundled OCCT libraries so users can replace them (LGPL relinking requirement).
- ShapeCraft's own code stays MIT OR Apache-2.0.
- Other kernels (e.g. truck) can be added later as another `kernel-*` crate behind the same traits.

## Consequences
- Builds need OCCT installed (Homebrew `opencascade`, distro packages on Linux, vcpkg/conda on Windows).
- Release bundles ship OCCT's shared libraries with its license and a note on how to replace them.
- The WASM build is postponed (OCCT has an Emscripten build, but it's large). Native platforms come first.
- C++ exceptions are caught in the shim. Rust sees only error codes.
