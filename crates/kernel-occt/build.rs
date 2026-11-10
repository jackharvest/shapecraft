//! Compiles the C++ shim and links OCCT.
//!
//! OCCT is found through `OCCT_INCLUDE_DIR` / `OCCT_LIB_DIR`, else the usual install locations. It is linked
//! dynamically, except when the library folder holds only static archives (`libTKernel.a`, as an Android NDK build
//! makes) or `OCCT_STATIC=1` is set.

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
    println!("cargo:rerun-if-env-changed=OCCT_STATIC");

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

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let mut build = cc::Build::new();
    if target_os == "android" {
        // One self-contained executable: the NDK's static libc++ (OCCT is built with it too), linked below.
        build.cpp_link_stdlib(None);
    }
    build
        .cpp(true)
        .std("c++17")
        .file("cpp/shim.cpp")
        .file("cpp/brep.cpp")
        .include(&include)
        .warnings(false)
        .compile("shapecraft_occt_shim");

    let is_static = env::var_os("OCCT_STATIC").is_some_and(|v| v == "1") || lib.as_deref().is_some_and(only_static);
    if let Some(lib) = &lib {
        println!("cargo:rustc-link-search=native={}", lib.display());
        // Find the libraries at run time without DYLD/LD paths during development (MSVC has no rpath: there the
        // OCCT DLLs go on PATH or beside the executable).
        if !is_static && env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc") {
            println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib.display());
        }
    }
    if is_static {
        // Archives resolve left to right with GNU ld: users before what they use, so the list goes top-down.
        for l in LIBS.iter().rev() {
            println!("cargo:rustc-link-lib=static={l}");
        }
    } else {
        for l in LIBS {
            println!("cargo:rustc-link-lib=dylib={l}");
        }
    }
    if target_os == "android" {
        // After OCCT, which uses them: static libc++ needs its ABI half (exceptions, RTTI, new/delete) named too.
        // Plain -l names: the NDK clang finds both archives in its sysroot, which rustc itself does not search.
        println!("cargo:rustc-link-lib=c++_static");
        println!("cargo:rustc-link-lib=c++abi");
        // OCCT's messages go to logcat.
        println!("cargo:rustc-link-lib=log");
    }
}

/// True when the folder has OCCT as static archives and no shared library beside them.
fn only_static(lib: &std::path::Path) -> bool {
    let has = |name: &str| lib.join(name).exists();
    has("libTKernel.a") && !has("libTKernel.so") && !has("libTKernel.dylib")
}
