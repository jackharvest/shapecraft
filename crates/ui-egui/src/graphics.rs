//! Which graphics path the app draws with: the user's choice (Preferences ▸ ShapeCraft, or `--graphics`), read once at
//! startup. Automatic tries the GPU's modern API first, then OpenGL, then the processor, so that almost any machine
//! gets a picture, and a real GPU on OpenGL (far faster) wins over drawing on the processor. Paths this machine doesn't
//! have stay listed but can't be picked.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

pub const AUTOMATIC: &str = "Automatic";
pub const VULKAN: &str = "Vulkan";
pub const METAL: &str = "Metal";
pub const DX12: &str = "DirectX 12";
pub const CPU: &str = "CPU Only";
pub const OPENGL: &str = "OpenGL";
/// The default GPU: discrete over integrated. Otherwise Preferences holds a GPU's name.
pub const BEST_GPU: &str = "Best Available";

/// The order Automatic tries them in, and the order of every fallback.
const ORDER: [&str; 5] = [VULKAN, METAL, DX12, OPENGL, CPU];

/// The choices Preferences lists on this platform.
pub fn choices() -> &'static [&'static str] {
    if cfg!(target_os = "macos") {
        &[AUTOMATIC, METAL, OPENGL, CPU]
    } else if cfg!(target_os = "windows") {
        &[AUTOMATIC, VULKAN, DX12, OPENGL, CPU]
    } else {
        &[AUTOMATIC, VULKAN, OPENGL, CPU]
    }
}

/// `--graphics` spelled loosely: auto, vulkan, metal, dx12, cpu, opengl (or gl). Anything else is None.
pub fn parse(name: &str) -> Option<&'static str> {
    match name.to_ascii_lowercase().as_str() {
        "auto" | "automatic" => Some(AUTOMATIC),
        "vulkan" => Some(VULKAN),
        "metal" => Some(METAL),
        "dx12" | "directx" | "directx 12" => Some(DX12),
        "cpu" | "cpu only" => Some(CPU),
        "gl" | "opengl" => Some(OPENGL),
        _ => None,
    }
}

/// The paths to try for a choice, best first: the choice itself, then everything Automatic would try.
fn attempts(choice: &str) -> Vec<&'static str> {
    let first = ORDER.iter().copied().find(|p| *p == choice);
    first.into_iter().chain(ORDER.iter().copied().filter(|p| Some(*p) != first)).collect()
}

/// The startup attempts for a choice, each a list of paths to try, in `attempts` order. OpenGL always gets an attempt of
/// its own: merely loading it next to Vulkan crashes AMD's driver seconds after launch (atio6axx.dll 32.0.32015, Radeon
/// 860M), in native code, where no fallback can catch it.
pub fn passes(choice: &str) -> Vec<Vec<&'static str>> {
    let mut passes: Vec<Vec<&'static str>> = Vec::new();
    for path in attempts(choice) {
        match passes.last_mut() {
            Some(pass) if path != OPENGL && pass.first().is_some_and(|p| *p != OPENGL) => pass.push(path),
            _ => passes.push(vec![path]),
        }
    }
    passes
}

/// Every path for a choice in one attempt, best first, for a platform where the window can be made only once (Android:
/// winit's event loop can't be built twice in a process). The AMD crash `passes` avoids is a desktop driver's.
pub fn single_pass(choice: &str) -> Vec<&'static str> {
    attempts(choice)
}

/// Where the crash memory lives: next to the settings. `graphics-starting` names the path a run is starting on and goes
/// away once that run has drawn for a while or ended on its own; `graphics-skip` lists paths that took the app down.
fn state_dir() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("ShapeCraft"))
}

const STARTING: &str = "graphics-starting";
const SKIP: &str = "graphics-skip";
/// Long enough to cover a driver that dies seconds after the first picture (AMD's took 5-10 s).
const SETTLE: std::time::Duration = std::time::Duration::from_secs(20);

static SKIPPED_NOW: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());
static JUST_CRASHED: Mutex<Option<&'static str>> = Mutex::new(None);

fn known(name: &str) -> Option<&'static str> {
    ORDER.iter().copied().find(|p| *p == name.trim())
}

fn read_skip(dir: &Path) -> Vec<&'static str> {
    std::fs::read_to_string(dir.join(SKIP)).unwrap_or_default().lines().filter_map(known).collect()
}

fn write_skip(dir: &Path, skip: &[&str]) {
    let _ = if skip.is_empty() {
        std::fs::remove_file(dir.join(SKIP))
    } else {
        std::fs::create_dir_all(dir).and_then(|()| std::fs::write(dir.join(SKIP), skip.join("\n") + "\n"))
    };
}

/// A marker left by the last run means it died in native code (a graphics driver) before it settled: its path joins the
/// skip list. Returns the paths to skip and the one that just crashed.
fn take_crash(dir: &Path) -> (Vec<&'static str>, Option<&'static str>) {
    let mut skip = read_skip(dir);
    let crashed = std::fs::read_to_string(dir.join(STARTING)).ok().and_then(|s| known(&s));
    let _ = std::fs::remove_file(dir.join(STARTING));
    if let Some(path) = crashed.filter(|p| !skip.contains(p)) {
        skip.push(path);
        write_skip(dir, &skip);
    }
    (skip, crashed)
}

/// `passes` without the skipped paths, unless the user asked for that path by name for this run (`--graphics`). A crash
/// on Intel's Vulkan driver inside Windows Sandbox (igvk64.dll, 0xC0000005) took the app down where DirectX 12 and
/// OpenGL both work, and nothing in the process can catch that. When every path is skipped, nothing is.
fn passes_without(choice: &str, skip: &[&str], forced: bool) -> Vec<Vec<&'static str>> {
    let skip: Vec<&str> = skip.iter().copied().filter(|p| !(forced && *p == choice)).collect();
    let kept: Vec<Vec<&'static str>> = passes(choice)
        .into_iter()
        .map(|pass| pass.into_iter().filter(|p| !skip.contains(p)).collect::<Vec<_>>())
        .filter(|pass| !pass.is_empty())
        .collect();
    if kept.is_empty() { passes(choice) } else { kept }
}

/// A `--graphics` value that names no path: worth a line in the log, since Preferences decides instead.
pub fn unknown_flag(value: &str) {
    log::warn!("--graphics {value:?} isn't a graphics path (auto, vulkan, metal, dx12, opengl, cpu), so Preferences decides");
}

/// The desktop app's startup attempts: `passes`, minus any path that crashed a run before (see `passes_without`).
/// `forced` is true when `--graphics` named the choice.
pub fn launch_passes(choice: &str, forced: bool) -> Vec<Vec<&'static str>> {
    let Some(dir) = state_dir() else { return passes(choice) };
    let (skip, crashed) = take_crash(&dir);
    if let Some(path) = crashed {
        log::warn!(
            "graphics: the last run died while starting on {path}, so it's skipped from now on (Preferences ▸ ShapeCraft tries it again)"
        );
    }
    if let Ok(mut s) = JUST_CRASHED.lock() {
        *s = crashed;
    }
    if let Ok(mut s) = SKIPPED_NOW.lock() {
        *s = skip.clone();
    }
    let passes = passes_without(choice, &skip, forced);
    log::info!("graphics: {choice}, trying {passes:?}");
    passes
}

/// The paths this run left out because they crashed the app before.
pub fn skipped() -> Vec<&'static str> {
    SKIPPED_NOW.lock().map(|s| s.clone()).unwrap_or_default()
}

/// Picking a skipped path in Preferences gives it another go on the next start.
pub fn try_again(path: &str) {
    if let Some(dir) = state_dir() {
        let skip: Vec<&str> = read_skip(&dir).into_iter().filter(|p| *p != path).collect();
        write_skip(&dir, &skip);
    }
}

fn mark_starting(path: &str) {
    if let Some(dir) = state_dir() {
        let _ = std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(dir.join(STARTING), path));
    }
}

/// This run got past the point where a driver crash counts against its path: it ended on its own, or is restarting itself.
pub fn settled() {
    if let Some(dir) = state_dir() {
        let _ = std::fs::remove_file(dir.join(STARTING));
    }
}

/// Call on every frame: `SETTLE` after the first one, the path has proven itself.
pub fn frame_drawn() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let _ = std::thread::Builder::new().name("graphics-settle".into()).spawn(|| {
            std::thread::sleep(SETTLE);
            settled();
        });
    });
}

/// Once, on the run after a crash: which path crashed and which one is drawing instead.
pub fn take_crash_notice() -> Option<(&'static str, &'static str)> {
    let crashed = JUST_CRASHED.lock().ok()?.take()?;
    Some((crashed, status().active?.path))
}

/// An adapter found at startup.
#[derive(Clone, Debug, PartialEq)]
pub struct Found {
    pub path: &'static str,
    pub name: String,
}

/// What this run ended up with, for Preferences.
#[derive(Clone, Debug, Default)]
pub struct Status {
    /// The choice the app started with, and the GPU asked for.
    pub choice: String,
    pub gpu: String,
    /// Every adapter usable with the window, among the paths this run looked at.
    pub found: Vec<Found>,
    pub active: Option<Found>,
    /// This run started on OpenGL, so it looked at nothing else; otherwise it never looked at OpenGL.
    pub opengl_pass: bool,
    /// Paths left out because they crashed the app at start before.
    pub skipped: Vec<&'static str>,
}

impl Status {
    /// The GPUs to choose between (not software renderers), by name.
    pub fn gpus(&self) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        for f in self.found.iter().filter(|f| f.path != CPU) {
            if !names.contains(&f.name) {
                names.push(f.name.clone());
            }
        }
        names
    }

    /// True for a path this run didn't look at, because OpenGL never starts beside the others.
    pub fn unchecked(&self, path: &str) -> bool {
        self.active.is_some() && (path == OPENGL) != self.opengl_pass && !(path == OPENGL && cfg!(target_os = "macos"))
    }
}

static STATUS: OnceLock<Mutex<Status>> = OnceLock::new();

fn status_cell() -> &'static Mutex<Status> {
    STATUS.get_or_init(Default::default)
}

pub fn status() -> Status {
    let mut s = status_cell().lock().map(|s| s.clone()).unwrap_or_default();
    s.skipped = skipped();
    s
}

fn path_of(info: &wgpu::AdapterInfo) -> Option<&'static str> {
    if info.device_type == wgpu::DeviceType::Cpu {
        return Some(CPU);
    }
    match info.backend {
        wgpu::Backend::Vulkan => Some(VULKAN),
        wgpu::Backend::Metal => Some(METAL),
        wgpu::Backend::Dx12 => Some(DX12),
        wgpu::Backend::Gl => Some(OPENGL),
        _ => None,
    }
}

/// Lower is better among adapters on the same path: a real GPU over a virtual one.
fn device_rank(kind: wgpu::DeviceType) -> u8 {
    match kind {
        wgpu::DeviceType::DiscreteGpu => 0,
        wgpu::DeviceType::IntegratedGpu => 1,
        wgpu::DeviceType::VirtualGpu => 2,
        _ => 3,
    }
}

/// The best of `found` (path, kind) for a choice, among a pass's paths. Automatic puts a discrete GPU over an integrated one
/// even when the integrated one has the better path; a path picked by hand comes first.
fn pick(choice: &str, pass: &[&str], found: &[(&str, wgpu::DeviceType)]) -> Option<usize> {
    let order = |path: &str| pass.iter().position(|p| *p == path);
    let ranked = found.iter().enumerate().filter_map(|(i, f)| order(f.0).map(|o| (i, o, device_rank(f.1))));
    if choice == AUTOMATIC { ranked.min_by_key(|r| (r.2, r.1)) } else { ranked.min_by_key(|r| (r.1, r.2)) }.map(|r| r.0)
}

fn select(
    choice: &str,
    gpu: &str,
    pass: &[&str],
    adapters: &[wgpu::Adapter],
    surface: Option<&wgpu::Surface<'_>>,
) -> Result<wgpu::Adapter, String> {
    let usable: Vec<(&wgpu::Adapter, wgpu::AdapterInfo, &'static str)> = adapters
        .iter()
        .filter(|a| surface.is_none_or(|s| a.is_surface_supported(s)))
        .filter_map(|a| {
            let info = a.get_info();
            path_of(&info).map(|p| (a, info, p))
        })
        .collect();
    // The GPU asked for when it's here; SHAPECRAFT_ADAPTER (part of a name) overrides Preferences for testing. A GPU
    // that has gone away, or that this path can't reach, must not stop the app starting.
    let wanted = std::env::var("SHAPECRAFT_ADAPTER").ok().filter(|w| !w.is_empty()).unwrap_or_else(|| gpu.to_string()).to_lowercase();
    let named: Vec<_> = usable.iter().filter(|u| wanted != BEST_GPU.to_lowercase() && u.1.name.to_lowercase().contains(&wanted)).collect();
    let among: Vec<_> = if named.is_empty() { usable.iter().collect() } else { named };
    let kinds: Vec<(&str, wgpu::DeviceType)> = among.iter().map(|u| (u.2, u.1.device_type)).collect();
    let best = pick(choice, pass, &kinds).and_then(|i| among.get(i)).ok_or_else(|| "no usable graphics adapter".to_string())?;
    if let Ok(mut s) = status_cell().lock() {
        s.choice = choice.to_string();
        s.gpu = gpu.to_string();
        // Best first, so that the first adapter listed on a path is the one that path would use.
        let mut ranked: Vec<_> = usable.iter().collect();
        ranked.sort_by_key(|u| device_rank(u.1.device_type));
        s.found = ranked.into_iter().map(|u| Found { path: u.2, name: u.1.name.clone() }).collect();
        s.active = Some(Found { path: best.2, name: best.1.name.clone() });
        s.opengl_pass = pass == [OPENGL];
    }
    log::info!("graphics: using {} on {} ({:?}, driver {} {})", best.2, best.1.name, best.1.device_type, best.1.driver, best.1.driver_info);
    mark_starting(best.2);
    Ok(best.0.clone())
}

/// The APIs a pass loads, and no others: a driver that crashed the app must not even be loaded to be listed. Drawing on
/// the processor goes through whichever modern API has a software device (WARP on DirectX 12, lavapipe on Vulkan).
fn backends(pass: &[&str]) -> wgpu::Backends {
    let mut b = wgpu::Backends::empty();
    for path in pass {
        b |= match *path {
            VULKAN => wgpu::Backends::VULKAN,
            METAL => wgpu::Backends::METAL,
            DX12 => wgpu::Backends::DX12,
            OPENGL => wgpu::Backends::GL,
            _ => wgpu::Backends::PRIMARY.difference(skip_backends()),
        };
    }
    b
}

fn skip_backends() -> wgpu::Backends {
    backends(&skipped().into_iter().filter(|p| *p != CPU).collect::<Vec<_>>())
}

/// The window's wgpu setup for one of a choice's `passes`. The device asks for everything the adapter offers instead of WebGPU's
/// defaults, which old GPUs and OpenGL don't meet.
pub fn configuration(choice: &str, gpu: &str, pass: &[&'static str]) -> egui_wgpu::WgpuConfiguration {
    let (choice, gpu, pass) = (choice.to_string(), gpu.to_string(), pass.to_vec());
    let mut create = egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    // On top of what WGPU_BACKEND already narrowed this to.
    create.instance_descriptor.backends &= backends(&pass);
    create.native_adapter_selector = Some(Arc::new(move |adapters, surface| select(&choice, &gpu, &pass, adapters, surface)));
    create.device_descriptor = Arc::new(|adapter| wgpu::DeviceDescriptor {
        label: Some("shapecraft device"),
        required_limits: adapter.limits(),
        ..Default::default()
    });
    egui_wgpu::WgpuConfiguration { wgpu_setup: egui_wgpu::WgpuSetup::CreateNew(create), ..Default::default() }
}

/// True once per run when the app ended up drawing on the processor, so it can say why things are slow.
pub fn take_slow_warning() -> bool {
    static SHOWN: AtomicBool = AtomicBool::new(false);
    status().active.is_some_and(|a| a.path == CPU) && !SHOWN.swap(true, Ordering::Relaxed)
}

/// No path worked at all: say so in words instead of vanishing. Failing to show this is no worse than not asking.
pub fn report_failure(reason: &str) {
    let display = std::env::var_os("WAYLAND_DISPLAY").or_else(|| std::env::var_os("DISPLAY")).is_some();
    if cfg!(target_os = "linux") && !display {
        log::error!(
            "ShapeCraft needs a desktop to draw on, and this session has none (neither WAYLAND_DISPLAY nor DISPLAY is set).\n\
             For modeling without one, use shapecraft-cli (run, render, mcp)."
        );
        return;
    }
    log::error!("ShapeCraft couldn't start its graphics: {reason}");
    let _ = crate::dialogs::MessageDialog::new()
        .set_level(crate::dialogs::MessageLevel::Error)
        .set_title("ShapeCraft can't start")
        .set_description(format!(
            "No graphics path worked on this computer (Vulkan, OpenGL or the processor). Updating the graphics driver usually helps.\n\n{reason}"
        ))
        .show();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_prefers_the_gpu_then_opengl_then_the_processor() {
        assert_eq!(attempts(AUTOMATIC), [VULKAN, METAL, DX12, OPENGL, CPU]);
    }

    #[test]
    fn a_choice_goes_first_and_the_rest_stay_as_fallbacks() {
        assert_eq!(attempts(OPENGL), [OPENGL, VULKAN, METAL, DX12, CPU]);
        assert_eq!(attempts(CPU), [CPU, VULKAN, METAL, DX12, OPENGL]);
    }

    #[test]
    fn automatic_puts_discrete_over_integrated_even_on_a_lesser_path() {
        use wgpu::DeviceType::{DiscreteGpu, IntegratedGpu};
        let pass = [VULKAN, METAL, DX12];
        assert_eq!(pick(AUTOMATIC, &pass, &[(VULKAN, IntegratedGpu), (VULKAN, DiscreteGpu)]), Some(1));
        assert_eq!(pick(AUTOMATIC, &pass, &[(VULKAN, IntegratedGpu), (DX12, DiscreteGpu)]), Some(1));
        assert_eq!(pick(VULKAN, &pass, &[(VULKAN, IntegratedGpu), (DX12, DiscreteGpu)]), Some(0));
        assert_eq!(pick(AUTOMATIC, &[OPENGL], &[(VULKAN, DiscreteGpu)]), None);
    }

    #[test]
    fn opengl_never_shares_a_pass() {
        assert_eq!(passes(AUTOMATIC), [vec![VULKAN, METAL, DX12], vec![OPENGL], vec![CPU]]);
        assert_eq!(passes(OPENGL), [vec![OPENGL], vec![VULKAN, METAL, DX12, CPU]]);
        assert_eq!(passes(CPU), [vec![CPU, VULKAN, METAL, DX12], vec![OPENGL]]);
    }

    #[test]
    fn a_path_that_crashed_is_left_out() {
        assert_eq!(passes_without(AUTOMATIC, &[VULKAN], false), [vec![METAL, DX12], vec![OPENGL], vec![CPU]]);
        assert_eq!(passes_without(VULKAN, &[VULKAN], false), [vec![METAL, DX12], vec![OPENGL], vec![CPU]]);
        assert_eq!(passes_without(VULKAN, &[VULKAN], true), passes(VULKAN));
        assert_eq!(passes_without(OPENGL, &[OPENGL], false), [vec![VULKAN, METAL, DX12, CPU]]);
        assert_eq!(passes_without(AUTOMATIC, &ORDER, false), passes(AUTOMATIC));
    }

    #[test]
    fn backends_match_the_pass() {
        assert_eq!(backends(&[VULKAN, METAL, DX12]), wgpu::Backends::VULKAN | wgpu::Backends::METAL | wgpu::Backends::DX12);
        assert_eq!(backends(&[DX12]), wgpu::Backends::DX12);
        assert_eq!(backends(&[OPENGL]), wgpu::Backends::GL);
    }

    #[test]
    fn a_marker_left_behind_counts_as_a_crash_once() {
        #![allow(clippy::unwrap_used)]
        let dir = std::env::temp_dir().join(format!("shapecraft-gfx-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(take_crash(&dir), (vec![], None));
        std::fs::write(dir.join(STARTING), VULKAN).unwrap();
        assert_eq!(take_crash(&dir), (vec![VULKAN], Some(VULKAN)));
        assert_eq!(take_crash(&dir), (vec![VULKAN], None));
        std::fs::write(dir.join(STARTING), "Glide").unwrap();
        assert_eq!(take_crash(&dir), (vec![VULKAN], None));
        write_skip(&dir, &[]);
        assert_eq!(take_crash(&dir), (vec![], None));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn graphics_flag_names() {
        assert_eq!(parse("GL"), Some(OPENGL));
        assert_eq!(parse("cpu"), Some(CPU));
        assert_eq!(parse("auto"), Some(AUTOMATIC));
        assert_eq!(parse("glide"), None);
    }

    #[test]
    fn every_listed_choice_can_be_parsed() {
        for c in choices() {
            assert_eq!(parse(c), Some(*c));
        }
    }
}
