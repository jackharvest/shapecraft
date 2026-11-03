//! Compiles the C++ shim and links OCCT dynamically.
//!
//! OCCT is found through `OCCT_INCLUDE_DIR` / `OCCT_LIB_DIR`, else the usual install locations.

use std::env;
use std::path::PathBuf;

const LIBS: &[&str] = &[
    "TKernel",
    "TKMath",
    "TKG2d",
    "TKG3d",
    "TKGeomBase",
    "TKGeomAlgo",
    "TKBRep",
    "TKTopAlgo",
    "TKPrim",
    "TKBO",
    "TKBool",
    "TKShHealing",
    "TKFillet",
    "TKOffset",
    "TKFeat",
    "TKHLR",
    "TKMesh",
    "TKXSBase",
    "TKDE",
    "TKDESTEP",
    "TKDEIGES",
    "TKLCAF",
    "TKCAF",
    "TKCDF",
    "TKXCAF",
    "TKVCAF",
    "TKV3d",
    "TKService",
];

fn first_existing(candidates: &[&str]) -> Option<PathBuf> {
    candidates.iter().map(PathBuf::from).find(|p| p.exists())
}

fn main() {
    println!("cargo:rerun-if-changed=cpp/shim.cpp");
    println!("cargo:rerun-if-changed=cpp/brep.cpp");
    println!("cargo:rerun-if-env-changed=OCCT_INCLUDE_DIR");
    println!("cargo:rerun-if-env-changed=OCCT_LIB_DIR");

    let include = env::var_os("OCCT_INCLUDE_DIR").map(PathBuf::from).or_else(|| {
        first_existing(&[
            "/opt/homebrew/opt/opencascade/include/opencascade",
            "/usr/local/opt/opencascade/include/opencascade",
            "/usr/include/opencascade",
            "/usr/local/include/opencascade",
        ])
    });
    let lib = env::var_os("OCCT_LIB_DIR").map(PathBuf::from).or_else(|| {
        first_existing(&[
            "/opt/homebrew/opt/opencascade/lib",
            "/usr/local/opt/opencascade/lib",
            "/usr/lib/x86_64-linux-gnu",
            "/usr/lib/aarch64-linux-gnu",
            "/usr/lib64",
            "/usr/lib",
        ])
    });
    let Some(include) = include else {
        println!("cargo:warning=OpenCASCADE headers not found; set OCCT_INCLUDE_DIR");
        std::process::exit(1);
    };

    cc::Build::new()
        .cpp(true)
        .std("c++17")
        .file("cpp/shim.cpp")
        .file("cpp/brep.cpp")
        .include(&include)
        .warnings(false)
        .compile("shapecraft_occt_shim");

    if let Some(lib) = lib {
        println!("cargo:rustc-link-search=native={}", lib.display());
        // Find the libraries at run time without DYLD/LD paths during development (MSVC has no rpath: there the
        // OCCT DLLs go on PATH or beside the executable).
        if env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc") {
            println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib.display());
        }
    }
    for l in LIBS {
        println!("cargo:rustc-link-lib=dylib={l}");
    }
}
