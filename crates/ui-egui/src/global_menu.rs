//! The desktop's global menu on Linux (KDE Plasma's AppMenu, #97): the menus as `com.canonical.dbusmenu` on the session
//! bus. The window tells the compositor where they are (our winit, `set_app_menu`); while something shows them (the
//! registrar is on the bus) the window leaves its own row of menus out.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::time::Duration;

use zbus::zvariant::{OwnedValue, Value};

use crate::menubar::{Entry, MenuAction, MenuState, When};

pub const PATH: &str = "/MenuBar";
const INTERFACE: &str = "com.canonical.dbusmenu";
const REGISTRAR: &str = "com.canonical.AppMenu.Registrar";

static BUS: OnceLock<zbus::blocking::Connection> = OnceLock::new();
static ANNOUNCED: AtomicBool = AtomicBool::new(false);

/// Connects to the session bus before the window opens. Returns the name the menus are found under, for the window to
/// tell the compositor; None with no bus, or one that doesn't answer at once.
pub fn connect() -> Option<String> {
    let (send, got) = mpsc::channel();
    std::thread::Builder::new()
        .name("menu-bus".into())
        .spawn(move || {
            let _ = send.send(zbus::blocking::Connection::session());
        })
        .ok()?;
    let bus = got.recv_timeout(Duration::from_millis(500)).ok()?.ok()?;
    let name = bus.unique_name()?.to_string();
    BUS.set(bus).ok()?;
    Some(name)
}

/// The window told its compositor where the menus are (Wayland, with a compositor that knows the protocol).
pub fn announced() {
    ANNOUNCED.store(true, Ordering::Relaxed);
}

/// One row of the menus, by its place in the list (the root is 0).
struct Node {
    label: &'static str,
    accel: Option<&'static str>,
    action: Option<(MenuAction, When)>,
    children: Option<Vec<usize>>,
}

#[derive(Default)]
struct Model {
    nodes: Vec<Node>,
    state: MenuState,
    revision: u32,
    lang: &'static str,
    picked: Vec<MenuAction>,
}

type Properties = HashMap<String, OwnedValue>;
type Layout = (i32, Properties, Vec<OwnedValue>);

fn own<'a>(v: impl Into<Value<'a>>) -> Option<OwnedValue> {
    OwnedValue::try_from(v.into()).ok()
}

/// "CmdOrCtrl+Shift+P" as the menu protocol writes it.
fn shortcut(accel: &str) -> Vec<Vec<String>> {
    let key = |k: &str| match k {
        "CmdOrCtrl" | "Cmd" | "Ctrl" => "Control".to_string(),
        "Comma" => ",".into(),
        k => k.to_string(),
    };
    vec![accel.split('+').map(key).collect()]
}

impl Model {
    fn build(&mut self, menus: Vec<(&'static str, Vec<Entry>)>) {
        self.nodes = vec![Node { label: "", accel: None, action: None, children: Some(Vec::new()) }];
        let mut top = Vec::new();
        for (title, entries) in menus {
            top.push(self.add_menu(title, entries));
        }
        if let Some(root) = self.nodes.first_mut() {
            root.children = Some(top);
        }
        self.revision = self.revision.wrapping_add(1);
    }

    fn add_menu(&mut self, label: &'static str, entries: Vec<Entry>) -> usize {
        let at = self.nodes.len();
        self.nodes.push(Node { label, accel: None, action: None, children: Some(Vec::new()) });
        let mut children = Vec::new();
        for e in entries {
            children.push(match e {
                Entry::Sub { label, items } => self.add_menu(label, items),
                Entry::Separator => {
                    self.nodes.push(Node { label: "", accel: None, action: None, children: None });
                    self.nodes.len() - 1
                }
                Entry::Item { label, accel, action, when } => {
                    self.nodes.push(Node { label, accel, action: Some((action, when)), children: None });
                    self.nodes.len() - 1
                }
            });
        }
        if let Some(n) = self.nodes.get_mut(at) {
            n.children = Some(children);
        }
        at
    }

    /// Whether anything in this menu can be used (the home screen leaves out the menus it can't use).
    fn usable(&self, at: usize) -> bool {
        self.nodes.get(at).is_some_and(|n| match (&n.children, &n.action) {
            (Some(c), _) => c.iter().any(|c| self.usable(*c)),
            (None, Some((_, when))) => when.holds(&self.state),
            (None, None) => false,
        })
    }

    fn properties(&self, at: usize) -> Properties {
        let mut out = Properties::new();
        let mut put = |key: &str, v: Option<OwnedValue>| {
            if let Some(v) = v {
                out.insert(key.to_string(), v);
            }
        };
        let Some(n) = self.nodes.get(at) else { return out };
        // An underscore marks the letter to press; a name's own is doubled.
        let label = |t: String| own(t.replace('_', "__"));
        match (&n.children, &n.action) {
            (Some(_), _) => {
                put("children-display", own("submenu"));
                if at > 0 {
                    put("label", label(crate::i18n::tr(n.label).to_string()));
                    put("visible", own(self.state.in_project || self.usable(at)));
                }
            }
            (None, Some((action, when))) => {
                let text = match action {
                    MenuAction::Ui(crate::workspace::toolbar::UiAction::LockGrid) => crate::menubar::grid_label(&self.state),
                    _ => crate::i18n::tr(n.label).to_string(),
                };
                put("label", label(text));
                put("enabled", own(when.holds(&self.state)));
                if let Some(on) = self.state.checked(action) {
                    put("toggle-type", own("checkmark"));
                    put("toggle-state", own(i32::from(on)));
                }
                if let Some(a) = n.accel {
                    put("shortcut", own(shortcut(a)));
                }
            }
            (None, None) => put("type", own("separator")),
        }
        out
    }

    fn layout(&self, at: usize, depth: i32) -> Layout {
        let children = self.nodes.get(at).and_then(|n| n.children.as_deref()).unwrap_or_default();
        let children = if depth == 0 { &[] } else { children };
        let made = children.iter().filter_map(|c| own(Value::new(self.layout(*c, depth - 1)))).collect();
        (at as i32, self.properties(at), made)
    }

    /// What picking a row does, if it can be picked now.
    fn pick(&mut self, id: i32) {
        let action = usize::try_from(id).ok().and_then(|i| self.nodes.get(i)).and_then(|n| n.action);
        if let Some((action, when)) = action
            && when.holds(&self.state)
        {
            self.picked.push(action);
        }
    }
}

struct Server {
    model: Arc<Mutex<Model>>,
    wake: Box<dyn Fn() + Send + Sync>,
}

impl Server {
    fn read<T: Default>(&self, f: impl FnOnce(&Model) -> T) -> T {
        self.model.lock().map(|m| f(&m)).unwrap_or_default()
    }
}

#[zbus::interface(name = "com.canonical.dbusmenu")]
impl Server {
    fn get_layout(&self, parent_id: i32, recursion_depth: i32, _property_names: Vec<String>) -> (u32, Layout) {
        let at = usize::try_from(parent_id).unwrap_or(0);
        self.read(|m| (m.revision, m.layout(at, recursion_depth)))
    }

    fn get_group_properties(&self, ids: Vec<i32>, _property_names: Vec<String>) -> Vec<(i32, Properties)> {
        self.read(|m| {
            let ids = if ids.is_empty() { (0..m.nodes.len() as i32).collect() } else { ids };
            ids.into_iter().map(|id| (id, usize::try_from(id).map(|i| m.properties(i)).unwrap_or_default())).collect()
        })
    }

    fn get_property(&self, id: i32, name: String) -> zbus::fdo::Result<OwnedValue> {
        let at = usize::try_from(id).map_err(|_| zbus::fdo::Error::InvalidArgs("no such item".into()))?;
        let found = self.read(|m| m.properties(at).remove(&name));
        found.ok_or_else(|| zbus::fdo::Error::InvalidArgs("no such property".into()))
    }

    fn event(&self, id: i32, event_id: String, _data: OwnedValue, _timestamp: u32) {
        if event_id == "clicked" {
            if let Ok(mut m) = self.model.lock() {
                m.pick(id);
            }
            (self.wake)();
        }
    }

    fn event_group(&self, events: Vec<(i32, String, OwnedValue, u32)>) -> Vec<i32> {
        for (id, event, data, time) in events {
            self.event(id, event, data, time);
        }
        Vec::new()
    }

    fn about_to_show(&self, _id: i32) -> bool {
        false
    }

    fn about_to_show_group(&self, _ids: Vec<i32>) -> (Vec<i32>, Vec<i32>) {
        (Vec::new(), Vec::new())
    }

    #[zbus(property)]
    fn version(&self) -> u32 {
        3
    }

    #[zbus(property)]
    fn text_direction(&self) -> String {
        "ltr".into()
    }

    #[zbus(property)]
    fn status(&self) -> String {
        "normal".into()
    }

    #[zbus(property)]
    fn icon_theme_path(&self) -> Vec<String> {
        Vec::new()
    }
}

/// The menus on the bus.
pub struct GlobalMenu {
    bus: zbus::blocking::Connection,
    model: Arc<Mutex<Model>>,
    shown: Arc<AtomicBool>,
    started: bool,
}

impl GlobalMenu {
    /// Puts the menus on the bus [`connect`] opened.
    pub fn start(wake: impl Fn() + Send + Sync + Clone + 'static) -> Option<Self> {
        let bus = BUS.get()?.clone();
        let model = Arc::new(Mutex::new(Model::default()));
        let server = Server { model: model.clone(), wake: Box::new(wake.clone()) };
        bus.object_server().at(PATH, server).ok()?;
        let shown = Arc::new(AtomicBool::new(false));
        let (watched, flag) = (bus.clone(), shown.clone());
        std::thread::Builder::new()
            .name("menu-registrar".into())
            .spawn(move || {
                let _ = watch(&watched, &flag, &wake);
            })
            .ok()?;
        Some(Self { bus, model, shown, started: false })
    }

    /// Something on the desktop shows the menus, so the window needn't.
    pub fn shown(&self) -> bool {
        ANNOUNCED.load(Ordering::Relaxed) && self.shown.load(Ordering::Relaxed)
    }

    /// What changed since the last frame goes to whoever shows the menus.
    pub fn update(&mut self, state: MenuState, menus: impl FnOnce() -> Vec<(&'static str, Vec<Entry>)>) {
        let lang = crate::i18n::current();
        let Ok(mut m) = self.model.lock() else { return };
        let rebuild = !self.started || m.lang != lang || m.state.in_project != state.in_project;
        if !rebuild && m.state == state {
            return;
        }
        let before: Vec<Properties> = if rebuild { Vec::new() } else { (0..m.nodes.len()).map(|i| m.properties(i)).collect() };
        (m.state, m.lang) = (state, lang);
        self.started = true;
        if rebuild {
            m.build(menus());
            let revision = m.revision;
            drop(m);
            let _ = self.bus.emit_signal(None::<&str>, PATH, INTERFACE, "LayoutUpdated", &(revision, 0i32));
            return;
        }
        let changed: Vec<(i32, Properties)> = (0..m.nodes.len())
            .map(|i| (i, m.properties(i)))
            .filter(|(i, now)| before.get(*i) != Some(now))
            .map(|(i, now)| (i as i32, now))
            .collect();
        drop(m);
        if !changed.is_empty() {
            let none: Vec<(i32, Vec<String>)> = Vec::new();
            let _ = self.bus.emit_signal(None::<&str>, PATH, INTERFACE, "ItemsPropertiesUpdated", &(changed, none));
        }
    }

    pub fn poll(&mut self) -> Vec<MenuAction> {
        self.model.lock().map(|mut m| std::mem::take(&mut m.picked)).unwrap_or_default()
    }
}

/// Follows whether the desktop has a place for global menus.
fn watch(bus: &zbus::blocking::Connection, shown: &AtomicBool, wake: &dyn Fn()) -> zbus::Result<()> {
    let dbus = zbus::blocking::fdo::DBusProxy::new(bus)?;
    let changes = dbus.receive_name_owner_changed_with_args(&[(0, REGISTRAR)])?;
    let there = dbus.name_has_owner(REGISTRAR.try_into()?)?;
    shown.store(there, Ordering::Relaxed);
    wake();
    for change in changes {
        let there = change.args()?.new_owner().is_some();
        shown.store(there, Ordering::Relaxed);
        wake();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(in_project: bool) -> Model {
        let mut m = Model { state: MenuState { in_project, ..Default::default() }, ..Default::default() };
        m.build(crate::menubar::menus_for(in_project));
        m
    }

    #[test]
    fn the_whole_menu_goes_on_the_bus() {
        let m = model(true);
        let (id, _, top) = m.layout(0, -1);
        assert_eq!(id, 0);
        assert!(top.len() > 4);
        // One level at a time, as desktops ask for it.
        assert!(m.layout(0, 0).2.is_empty());
        let label = |i: usize| m.properties(i).get("label").and_then(|v| String::try_from(v.clone()).ok());
        assert_eq!(label(1).as_deref(), Some("File"));
        assert!(m.properties(1).contains_key("children-display"));
    }

    #[test]
    fn only_what_is_enabled_can_be_picked() {
        let mut m = model(true);
        let undo = m.nodes.iter().position(|n| n.label.starts_with("Undo")).unwrap() as i32;
        m.pick(undo);
        assert!(m.picked.is_empty(), "nothing to undo");
        m.state.undo = true;
        m.pick(undo);
        assert_eq!(m.picked.len(), 1);
        m.pick(-5);
        m.pick(1_000_000);
        assert_eq!(m.picked.len(), 1);
    }

    #[test]
    fn home_hides_the_menus_it_cant_use() {
        let m = model(false);
        let hidden: Vec<&str> = (1..m.nodes.len())
            .filter(|i| m.properties(*i).get("visible").is_some_and(|v| bool::try_from(v) == Ok(false)))
            .filter_map(|i| m.nodes.get(i).map(|n| n.label))
            .collect();
        assert!(!hidden.contains(&"File"), "{hidden:?}");
    }

    #[test]
    fn shortcuts_are_spelled_the_protocols_way() {
        assert_eq!(shortcut("CmdOrCtrl+Shift+P"), [["Control", "Shift", "P"]]);
        assert_eq!(shortcut("L"), [["L"]]);
    }
}
