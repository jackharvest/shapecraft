//! Which graphics path the app draws with: the user's choice (Preferences ▸ ShapeCraft, or `--graphics`), read once at
//! startup. Automatic tries the GPU's modern API first, then OpenGL, then the processor, so that almost any machine
//! gets a picture, and a real GPU on OpenGL (far faster) wins over drawing on the processor. Paths this machine doesn't
//! have stay listed but can't be picked.

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
    status_cell().lock().map(|s| s.clone()).unwrap_or_default()
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
    Ok(best.0.clone())
}

/// The window's wgpu setup for one of a choice's `passes`. The device asks for everything the adapter offers instead of WebGPU's
/// defaults, which old GPUs and OpenGL don't meet.
pub fn configuration(choice: &str, gpu: &str, pass: &[&'static str]) -> egui_wgpu::WgpuConfiguration {
    let (choice, gpu, pass) = (choice.to_string(), gpu.to_string(), pass.to_vec());
    let mut create = egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    // On top of what WGPU_BACKEND already narrowed this to.
    create.instance_descriptor.backends &= if pass == [OPENGL] { wgpu::Backends::GL } else { wgpu::Backends::PRIMARY };
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
        eprintln!(
            "ShapeCraft needs a desktop to draw on, and this session has none (neither WAYLAND_DISPLAY nor DISPLAY is set).\n\
             For modeling without one, use shapecraft-cli (run, render, mcp)."
        );
        return;
    }
    eprintln!("ShapeCraft couldn't start its graphics: {reason}");
    let _ = rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
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
