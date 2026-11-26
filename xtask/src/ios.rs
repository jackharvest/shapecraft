//! iOS builds: OpenCASCADE cross-compiled with Xcode's clang and linked in statically, then the app itself (the
//! desktop interface in a UIKit window on Metal), signed with a developer team and run on a device or simulator.
//!
//!   OCCT_SRC=… cargo xtask ios-occt [device|sim]   → target/ios/occt-<device|sim>
//!
//! macOS host with Xcode 27.1 or newer (the iPhone Duo's SDK); `xcode-select` may point anywhere, Xcode is found
//! through DEVELOPER_DIR or /Applications/Xcode.app.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};

use crate::android::{find, linked_toolkits};
use crate::util::{clear, copy, run, s, target};

/// The oldest iOS the app runs on: the SDK's own floor, so iPads that stopped at iPadOS 15 or 16 still run it.
/// Anything newer (the Duo's reserved regions, iOS 27.1) is checked for at run time.
pub const MIN_IOS: &str = "15.0";

pub struct Kind {
    pub name: &'static str,
    /// Xcode's SDK name.
    pub sdk: &'static str,

    pub triple: &'static str,
}

pub fn kind(rest: &[String]) -> Result<Kind> {
    match rest.first().map(String::as_str) {
        None | Some("device") => Ok(Kind { name: "device", sdk: "iphoneos", triple: "aarch64-apple-ios" }),
        Some("sim") => Ok(Kind { name: "sim", sdk: "iphonesimulator", triple: "aarch64-apple-ios-sim" }),
        Some(other) => bail!("unknown iOS kind {other} (device or sim)"),
    }
}

/// Xcode's developer folder, set for the tools this process starts (xcrun, cmake's compiler checks).
pub fn developer_dir() -> Result<PathBuf> {
    let dir =
        std::env::var_os("DEVELOPER_DIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/Applications/Xcode.app/Contents/Developer"));
    if !dir.join("Platforms/iPhoneOS.platform").exists() {
        bail!("{} has no iOS platform: install Xcode or set DEVELOPER_DIR", dir.display());
    }
    // SAFETY: set once at start, before any thread is spawned.
    unsafe { std::env::set_var("DEVELOPER_DIR", &dir) };
    Ok(dir)
}

pub fn occt_dir(k: &Kind) -> PathBuf {
    target().join("ios").join(format!("occt-{}", k.name))
}

/// Configures and builds OpenCASCADE for iOS: static libraries, only the toolkits our shim links, none of the
/// optional third-party products — the same cut as the Android build.
pub fn occt(rest: &[String]) -> Result<()> {
    let k = kind(rest)?;
    developer_dir()?;
    let src = PathBuf::from(std::env::var_os("OCCT_SRC").context("set OCCT_SRC to an OCCT 7.9 source tree")?);
    let build = target().join("ios").join(format!("occt-build-{}", k.name));
    let install = occt_dir(&k);
    let mut args = vec![
        "-S".to_string(),
        s(&src).to_string(),
        "-B".to_string(),
        s(&build).to_string(),
        "-G".to_string(),
        "Ninja".to_string(),
        "-DCMAKE_SYSTEM_NAME=iOS".to_string(),
        "-DCMAKE_OSX_ARCHITECTURES=arm64".to_string(),
        format!("-DCMAKE_OSX_SYSROOT={}", k.sdk),
        format!("-DCMAKE_OSX_DEPLOYMENT_TARGET={MIN_IOS}"),
        "-DCMAKE_BUILD_TYPE=Release".to_string(),
        "-DBUILD_LIBRARY_TYPE=Static".to_string(),
        "-DBUILD_DOC_Overview=OFF".to_string(),
        "-DBUILD_SAMPLES_QT=OFF".to_string(),
        "-DBUILD_MODULE_Draw=OFF".to_string(),
    ];
    for off in ["FREETYPE", "OPENGL", "GLES2", "TK", "FREEIMAGE", "RAPIDJSON", "TBB", "VTK", "DRACO", "OPENVR", "FFMPEG"] {
        args.push(format!("-DUSE_{off}=OFF"));
    }
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    run("cmake", &args)?;
    let toolkits = linked_toolkits()?;
    let mut build_args = vec!["--build", s(&build), "--target"];
    build_args.extend(toolkits.iter().map(String::as_str));
    run("cmake", &build_args)?;
    clear(&install)?;
    let lib = install.join("lib");
    for tk in &toolkits {
        let name = format!("lib{tk}.a");
        let found = find(&build, &name).with_context(|| format!("{name} wasn't built"))?;
        copy(&found, &lib.join(&name))?;
    }
    let headers = build.join("include/opencascade");
    let into = install.join("include/opencascade");
    std::fs::create_dir_all(&into)?;
    for entry in std::fs::read_dir(&headers).with_context(|| format!("no headers in {}", headers.display()))? {
        let path = entry?.path();
        if let Some(name) = path.file_name() {
            std::fs::copy(&path, into.join(name)).with_context(|| format!("copying {}", path.display()))?;
        }
    }
    println!("OCCT for iOS {} ({} toolkits) → {}", k.name, toolkits.len(), install.display());
    Ok(())
}

/// The signing team and bundle id, from SHAPECRAFT_IOS_TEAM and SHAPECRAFT_IOS_BUNDLE (personal, so not in the repo).
pub fn identity() -> Result<(String, String)> {
    let team = std::env::var("SHAPECRAFT_IOS_TEAM").context("set SHAPECRAFT_IOS_TEAM to your Apple developer team id")?;
    let bundle = std::env::var("SHAPECRAFT_IOS_BUNDLE").context("set SHAPECRAFT_IOS_BUNDLE to a bundle id of your own")?;
    Ok((team, bundle))
}

/// Gets a provisioning profile for the bundle id that names the attached device: a stub Xcode project with the same
/// id, built once with automatic signing (Xcode registers the device and downloads the profile; a free team works).
pub fn provision(rest: &[String]) -> Result<()> {
    developer_dir()?;
    let (team, bundle) = identity()?;
    let device = rest.first().context("give the device's UDID (xcrun devicectl list devices)")?;
    let dir = target().join("ios").join("provision");
    let proj = dir.join("Provision.xcodeproj");
    std::fs::create_dir_all(&proj)?;
    std::fs::write(dir.join("main.swift"), "import UIKit\nUIApplicationMain(CommandLine.argc, CommandLine.unsafeArgv, nil, nil)\n")?;
    std::fs::write(proj.join("project.pbxproj"), PBXPROJ.replace("@TEAM@", &team).replace("@BUNDLE@", &bundle))?;
    run(
        "xcodebuild",
        &[
            "-project",
            s(&proj),
            "-scheme",
            "Provision",
            "-destination",
            &format!("id={device}"),
            "-derivedDataPath",
            s(&dir.join("derived")),
            "-allowProvisioningUpdates",
            "-allowProvisioningDeviceRegistration",
            "build",
        ],
    )?;
    println!("profile for {bundle} is in ~/Library/Developer/Xcode/UserData/Provisioning Profiles");
    Ok(())
}

const PBXPROJ: &str = r#"// !$*UTF8*$!
{
	archiveVersion = 1;
	classes = {};
	objectVersion = 56;
	objects = {
		A1000000000000000000000A = {isa = PBXBuildFile; fileRef = A1000000000000000000000B; };
		A1000000000000000000000B = {isa = PBXFileReference; lastKnownFileType = sourcecode.swift; path = main.swift; sourceTree = "<group>"; };
		A1000000000000000000000C = {isa = PBXFileReference; explicitFileType = wrapper.application; includeInIndex = 0; path = Provision.app; sourceTree = BUILT_PRODUCTS_DIR; };
		A1000000000000000000000D = {isa = PBXGroup; children = (A1000000000000000000000B, A1000000000000000000000E); sourceTree = "<group>"; };
		A1000000000000000000000E = {isa = PBXGroup; children = (A1000000000000000000000C); name = Products; sourceTree = "<group>"; };
		A1000000000000000000000F = {isa = PBXSourcesBuildPhase; buildActionMask = 2147483647; files = (A1000000000000000000000A); runOnlyForDeploymentPostprocessing = 0; };
		A10000000000000000000010 = {isa = PBXNativeTarget; buildConfigurationList = A10000000000000000000013; buildPhases = (A1000000000000000000000F); buildRules = (); dependencies = (); name = Provision; productName = Provision; productReference = A1000000000000000000000C; productType = "com.apple.product-type.application"; };
		A10000000000000000000011 = {isa = PBXProject; buildConfigurationList = A10000000000000000000016; compatibilityVersion = "Xcode 14.0"; mainGroup = A1000000000000000000000D; productRefGroup = A1000000000000000000000E; projectDirPath = ""; projectRoot = ""; targets = (A10000000000000000000010); };
		A10000000000000000000012 = {isa = XCBuildConfiguration; buildSettings = {
			CODE_SIGN_STYLE = Automatic; DEVELOPMENT_TEAM = @TEAM@; PRODUCT_BUNDLE_IDENTIFIER = "@BUNDLE@"; PRODUCT_NAME = Provision;
			GENERATE_INFOPLIST_FILE = YES; INFOPLIST_KEY_UILaunchScreen_Generation = YES; SDKROOT = iphoneos; SWIFT_VERSION = 5.0;
			IPHONEOS_DEPLOYMENT_TARGET = 15.0; TARGETED_DEVICE_FAMILY = "1,2"; }; name = Debug; };
		A10000000000000000000013 = {isa = XCConfigurationList; buildConfigurations = (A10000000000000000000012); defaultConfigurationName = Debug; };
		A10000000000000000000014 = {isa = XCBuildConfiguration; buildSettings = { SDKROOT = iphoneos; }; name = Debug; };
		A10000000000000000000016 = {isa = XCConfigurationList; buildConfigurations = (A10000000000000000000014); defaultConfigurationName = Debug; };
	};
	rootObject = A10000000000000000000011;
}
"#;

/// The app's bundle: apps/shapecraft-ios built against OCCT for the kind, its Info.plist and icons, signed — with
/// the newest provisioning profile for the bundle id on a device, ad hoc on the simulator.
pub fn app(rest: &[String]) -> Result<PathBuf> {
    let k = kind(rest)?;
    let (team, bundle) = identity()?;
    build_app(&k, &bundle, Some(&team))
}

/// The bundle id a sideload build carries. Sideloading tools give it one of the user's own as they sign it.
const SIDELOAD_BUNDLE: &str = "com.jackharvest.shapecraft";

/// An .ipa for sideloading (AltStore, Sideloadly and the like): the phone build with an ad-hoc signature, which the
/// tool replaces with the user's own Apple account's. → target/shapecraft-<version>-ios-arm64.ipa
pub fn ipa(_rest: &[String]) -> Result<PathBuf> {
    let k = kind(&[])?;
    let bundle = std::env::var("SHAPECRAFT_IOS_BUNDLE").unwrap_or_else(|_| SIDELOAD_BUNDLE.to_string());
    let app = build_app(&k, &bundle, None)?;
    let stage = target().join("ios").join("ipa");
    clear(&stage)?;
    run("ditto", &[s(&app), s(&stage.join("Payload/ShapeCraft.app"))])?;
    let out = target().join(format!("shapecraft-{}-ios-arm64.ipa", crate::util::version()?));
    let _ = std::fs::remove_file(&out);
    // An .ipa is a zip of Payload/; ditto keeps the bundle's symlinks and permissions as iOS expects.
    let status = std::process::Command::new("ditto")
        .args(["-c", "-k", "--sequesterRsrc", "--keepParent", "Payload", s(&out)])
        .current_dir(&stage)
        .status()?;
    if !status.success() {
        bail!("zipping the .ipa failed");
    }
    println!("sideload build → {}", out.display());
    Ok(out)
}

/// Builds the sideload .ipa here (Xcode 27.1+, which the CI runners don't have yet) and puts it in a private draft
/// release "ipa-<version>" for release.yml's ios job to check and publish with the rest (needs `gh`, signed in).
pub fn stage(rest: &[String]) -> Result<()> {
    let ipa = ipa(rest)?;
    let sdk = crate::util::output("xcrun", &["--sdk", "iphoneos", "--show-sdk-version"])?.trim().to_string();
    let newer = |v: &str| v.split('.').map(|p| p.parse::<u32>().unwrap_or(0)).collect::<Vec<_>>();
    if newer(&sdk) < vec![27, 1] {
        bail!("iOS SDK {sdk} is older than 27.1: the iPhone Duo would run this .ipa letterboxed");
    }
    let tag = format!("ipa-{}", crate::util::version()?);
    let exists = std::process::Command::new("gh").args(["release", "view", &tag]).output()?.status.success();
    if exists {
        run("gh", &["release", "upload", &tag, s(&ipa), "--clobber"])?;
    } else {
        run(
            "gh",
            &[
                "release",
                "create",
                &tag,
                s(&ipa),
                "--draft",
                "--title",
                &tag,
                "--notes",
                "Staging for release.yml (iOS SDK 27.1+); deleted when the release publishes.",
            ],
        )?;
    }
    println!("staged {} in draft release {tag} (iOS SDK {sdk})", ipa.display());
    Ok(())
}

/// Builds ShapeCraft.app for `k`: signed for the team's devices, or (no team) ad-hoc for a sideloading tool to sign.
fn build_app(k: &Kind, bundle: &str, team: Option<&str>) -> Result<PathBuf> {
    let dev = developer_dir()?;
    let occt = occt_dir(k);
    if !occt.join("lib/libTKernel.a").exists() {
        bail!("no OCCT for iOS {} yet: OCCT_SRC=… cargo xtask ios-occt {}", k.name, k.name);
    }
    let status = std::process::Command::new("cargo")
        .args(["build", "--release", "-p", "shapecraft-ios", "--target", k.triple])
        .env("OCCT_INCLUDE_DIR", occt.join("include/opencascade"))
        .env("OCCT_LIB_DIR", occt.join("lib"))
        .env("IPHONEOS_DEPLOYMENT_TARGET", MIN_IOS)
        .status()?;
    if !status.success() {
        bail!("cargo build for {} failed", k.triple);
    }
    let bin = target().join(k.triple).join("release/shapecraft-ios");
    let app = target().join("ios").join(k.name).join("ShapeCraft.app");
    clear(&app)?;
    copy(&bin, &app.join("ShapeCraft"))?;
    // Icons: the flat icon at the sizes the home screen asks for (60 pt at 2× and 3×, iPad 76/83.5 pt at 2×).
    let icon = crate::util::root().join("assets/icon/png/shapecraft-1024.png");
    for (name, px) in [
        ("AppIcon60x60@2x.png", 120),
        ("AppIcon60x60@3x.png", 180),
        ("AppIcon76x76@2x~ipad.png", 152),
        ("AppIcon83.5x83.5@2x~ipad.png", 167),
    ] {
        let px = px.to_string();
        run("sips", &["-z", &px, &px, s(&icon), "--out", s(&app.join(name))])?;
    }
    let sdk_version = crate::util::output("xcrun", &["--sdk", k.sdk, "--show-sdk-version"])?.trim().to_string();
    let platform = if k.name == "sim" { "iPhoneSimulator" } else { "iPhoneOS" };
    let plist = INFO_PLIST
        .replace("@BUNDLE@", bundle)
        .replace("@VERSION@", &crate::util::version()?)
        .replace("@MIN@", MIN_IOS)
        .replace("@PLATFORM@", platform)
        .replace("@SDK@", &format!("{}{}", k.sdk, sdk_version))
        .replace("@SDKVER@", &sdk_version);
    std::fs::write(app.join("Info.plist"), plist)?;
    if let (Some(team), "device") = (team, k.name) {
        let profile = newest_profile(bundle)?;
        copy(&profile, &app.join("embedded.mobileprovision"))?;
        let ents = target().join("ios").join("entitlements.plist");
        std::fs::write(&ents, ENTITLEMENTS.replace("@TEAM@", team).replace("@BUNDLE@", bundle))?;
        let identity = crate::util::output("security", &["find-identity", "-v", "-p", "codesigning"])?;
        let hash = identity
            .lines()
            .find(|l| l.contains("Apple Development"))
            .and_then(|l| l.split_whitespace().nth(1))
            .context("no Apple Development signing identity in the keychain")?
            .to_string();
        run("codesign", &["--force", "--sign", &hash, "--entitlements", s(&ents), "--timestamp=none", s(&app)])?;
    } else {
        run("codesign", &["--force", "--sign", "-", s(&app)])?;
    }
    let _ = dev;
    println!("ShapeCraft.app for iOS {} → {}", k.name, app.display());
    Ok(app)
}

/// Builds the app, installs it on the device (UDID, see `xcrun devicectl list devices`) or booted simulator, and
/// starts it.
pub fn run_app(rest: &[String]) -> Result<()> {
    let app = app(rest)?;
    let k = kind(rest)?;
    let (_, bundle) = identity()?;
    let device = rest.get(1).context("give the device's or simulator's UDID after the kind")?;
    if k.name == "sim" {
        run("xcrun", &["simctl", "install", device, s(&app)])?;
        run("xcrun", &["simctl", "launch", "--terminate-running-process", device, &bundle])?;
    } else {
        run("xcrun", &["devicectl", "device", "install", "app", "--device", device, s(&app)])?;
        run("xcrun", &["devicectl", "device", "process", "launch", "--terminate-existing", "--device", device, &bundle])?;
    }
    Ok(())
}

/// The newest provisioning profile Xcode downloaded for the bundle id (`cargo xtask ios-provision` makes one).
fn newest_profile(bundle: &str) -> Result<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from).context("no HOME")?;
    let dir = home.join("Library/Developer/Xcode/UserData/Provisioning Profiles");
    let mut best: Option<(String, PathBuf)> = None;
    for entry in std::fs::read_dir(&dir).with_context(|| format!("no profiles in {}", dir.display()))? {
        let path = entry?.path();
        let Ok(text) = crate::util::output("security", &["cms", "-D", "-i", s(&path)]) else { continue };
        if !text.contains(&format!(".{bundle}</string>")) {
            continue;
        }
        let expires =
            text.split("<key>ExpirationDate</key>").nth(1).and_then(|t| t.split("<date>").nth(1)).and_then(|t| t.split("</date>").next());
        let expires = expires.unwrap_or_default().to_string();
        if best.as_ref().is_none_or(|(e, _)| expires > *e) {
            best = Some((expires, path));
        }
    }
    best.map(|(_, p)| p).with_context(|| format!("no profile for {bundle}: cargo xtask ios-provision UDID"))
}

const ENTITLEMENTS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>application-identifier</key><string>@TEAM@.@BUNDLE@</string>
<key>com.apple.developer.team-identifier</key><string>@TEAM@</string>
<key>get-task-allow</key><true/>
</dict></plist>
"#;

const INFO_PLIST: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>@BUNDLE@</string>
<key>CFBundleExecutable</key><string>ShapeCraft</string>
<key>CFBundleName</key><string>ShapeCraft</string>
<key>CFBundleDisplayName</key><string>ShapeCraft</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
<key>CFBundleShortVersionString</key><string>@VERSION@</string>
<key>CFBundleVersion</key><string>@VERSION@</string>
<key>CFBundleSupportedPlatforms</key><array><string>@PLATFORM@</string></array>
<key>CFBundleIcons</key><dict><key>CFBundlePrimaryIcon</key><dict>
  <key>CFBundleIconFiles</key><array><string>AppIcon60x60</string></array><key>CFBundleIconName</key><string>AppIcon</string></dict></dict>
<key>CFBundleIcons~ipad</key><dict><key>CFBundlePrimaryIcon</key><dict>
  <key>CFBundleIconFiles</key><array><string>AppIcon60x60</string><string>AppIcon76x76</string><string>AppIcon83.5x83.5</string></array></dict></dict>
<key>MinimumOSVersion</key><string>@MIN@</string>
<key>DTPlatformName</key><string>@SDK@</string>
<key>DTSDKName</key><string>@SDK@</string>
<key>DTPlatformVersion</key><string>@SDKVER@</string>
<key>UIDeviceFamily</key><array><integer>1</integer><integer>2</integer></array>
<key>UIRequiredDeviceCapabilities</key><array><string>arm64</string><string>metal</string></array>
<key>UILaunchScreen</key><dict><key>UIColorName</key><string></string></dict>
<key>UIApplicationSupportsIndirectInputEvents</key><true/>
<key>UIApplicationSceneManifest</key><dict>
  <key>UIApplicationSupportsMultipleScenes</key><false/>
  <key>UISceneConfigurations</key><dict><key>UIWindowSceneSessionRoleApplication</key><array><dict>
    <key>UISceneConfigurationName</key><string>Default</string>
    <key>UISceneDelegateClassName</key><string>WinitSceneDelegate</string></dict></array></dict></dict>
<key>UIRequiresFullScreen</key><true/>
<key>UIStatusBarHidden</key><false/>
<key>UIFileSharingEnabled</key><true/>
<key>LSSupportsOpeningDocumentsInPlace</key><true/>
<key>UISupportedInterfaceOrientations</key><array>
  <string>UIInterfaceOrientationPortrait</string><string>UIInterfaceOrientationLandscapeLeft</string>
  <string>UIInterfaceOrientationLandscapeRight</string><string>UIInterfaceOrientationPortraitUpsideDown</string></array>
<key>UISupportedInterfaceOrientations~ipad</key><array>
  <string>UIInterfaceOrientationPortrait</string><string>UIInterfaceOrientationLandscapeLeft</string>
  <string>UIInterfaceOrientationLandscapeRight</string><string>UIInterfaceOrientationPortraitUpsideDown</string></array>
</dict></plist>
"#;
