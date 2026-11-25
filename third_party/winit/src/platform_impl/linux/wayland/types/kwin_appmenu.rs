//! ShapeCraft patch: where a window's menus are, for KDE's global menu (see PATCH.md).

use sctk::globals::GlobalData;
use sctk::reexports::client::globals::{BindError, GlobalList};
use sctk::reexports::client::protocol::wl_surface::WlSurface;
use sctk::reexports::client::{delegate_dispatch, Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols_plasma::appmenu::client::org_kde_kwin_appmenu::OrgKdeKwinAppmenu;
use wayland_protocols_plasma::appmenu::client::org_kde_kwin_appmenu_manager::OrgKdeKwinAppmenuManager;

use crate::platform::wayland::{app_menu, mark_app_menu_announced};
use crate::platform_impl::wayland::state::WinitState;

#[derive(Debug, Clone)]
pub struct KWinAppMenuManager {
    manager: OrgKdeKwinAppmenuManager,
}

impl KWinAppMenuManager {
    pub fn new(
        globals: &GlobalList,
        queue_handle: &QueueHandle<WinitState>,
    ) -> Result<Self, BindError> {
        let manager = globals.bind(queue_handle, 1..=1, GlobalData)?;
        Ok(Self { manager })
    }

    /// Tells the compositor where this window's menus are, if the app said. The object goes with the window's surface.
    pub fn announce(&self, surface: &WlSurface, queue_handle: &QueueHandle<WinitState>) {
        if let Some((service, path)) = app_menu() {
            let menu = self.manager.create(surface, queue_handle, ());
            menu.set_address(service.clone(), path.clone());
            mark_app_menu_announced();
        }
    }
}

impl Dispatch<OrgKdeKwinAppmenuManager, GlobalData, WinitState> for KWinAppMenuManager {
    fn event(
        _: &mut WinitState,
        _: &OrgKdeKwinAppmenuManager,
        _: <OrgKdeKwinAppmenuManager as Proxy>::Event,
        _: &GlobalData,
        _: &Connection,
        _: &QueueHandle<WinitState>,
    ) {
    }
}

impl Dispatch<OrgKdeKwinAppmenu, (), WinitState> for KWinAppMenuManager {
    fn event(
        _: &mut WinitState,
        _: &OrgKdeKwinAppmenu,
        _: <OrgKdeKwinAppmenu as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<WinitState>,
    ) {
    }
}

delegate_dispatch!(WinitState: [OrgKdeKwinAppmenuManager: GlobalData] => KWinAppMenuManager);
delegate_dispatch!(WinitState: [OrgKdeKwinAppmenu: ()] => KWinAppMenuManager);
