{
  # Nix development shell: `nix develop`, or `direnv allow` with nix-direnv.
  # Gives the Rust toolchain, OpenCASCADE (the same 7.9 line CI uses) with
  # OCCT_INCLUDE_DIR / OCCT_LIB_DIR set for crates/kernel-occt/build.rs, and the
  # window/GPU libraries that winit and wgpu load at run time.
  description = "ShapeCraft development shell";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, rust-overlay }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      forAll = f: nixpkgs.lib.genAttrs systems (system:
        f (import nixpkgs { inherit system; overlays = [ rust-overlay.overlays.default ]; }));

      # CI builds with floating stable; flake.lock pins which stable this is.
      # A rust-toolchain.toml, if one is added later, takes over (flakes only
      # see git-tracked files, so it must be committed).
      toolchainFile = ./rust-toolchain.toml;
      pinned = builtins.pathExists toolchainFile;
      components = [ "rustfmt" "clippy" "rust-src" "rust-analyzer" ];

      mkToolchain = pkgs:
        if pinned then
          let tc = (builtins.fromTOML (builtins.readFile toolchainFile)).toolchain;
          in pkgs.rust-bin.fromRustupToolchain (tc // {
            components = (tc.components or [ ]) ++ components;
          })
        else
          pkgs.rust-bin.stable.latest.default.override { extensions = components; };
    in {
      devShells = forAll (pkgs:
        let
          # nixpkgs' OCCT 7.9.3 built with GCC 16 segfaults inside its own
          # booleans and distance queries (BRepAlgoAPI_Cut, Extrema_ExtCC) in the
          # engine tests; the same source built with GCC 14 passes. Built locally
          # (not in the binary cache), a few minutes on first `nix develop`.
          occt = pkgs.opencascade-occt.override { stdenv = pkgs.gcc14Stdenv; };
          # Loaded with dlopen by winit (Wayland/X11) and wgpu (Vulkan/GL), so
          # they must be on LD_LIBRARY_PATH, not just present at link time.
          # GPU drivers themselves come from the host (/run/opengl-driver on NixOS).
          runtimeLibs = with pkgs; [
            wayland
            libxkbcommon
            vulkan-loader
            libGL
            libx11
            libxcursor
            libxrandr
            libxi
          ];
        in {
          default = pkgs.mkShell {
            packages = [
              (mkToolchain pkgs)
              pkgs.pkg-config
              occt
            ];

            OCCT_INCLUDE_DIR = "${occt}/include/opencascade";
            OCCT_LIB_DIR = "${occt}/lib";
            # build.rs sets an rpath only on kernel-occt's own targets; tests and
            # apps in other crates find OCCT through this, as in CI.
            LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath ([ occt ] ++ runtimeLibs);

            shellHook = ''
              echo "shapecraft devshell: $(rustc --version) [${if pinned then "rust-toolchain.toml" else "stable"}], OCCT ${occt.version}"
            '';
          };
        });
    };
}
