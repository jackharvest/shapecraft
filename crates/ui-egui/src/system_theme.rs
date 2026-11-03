//! The desktop's light or dark preference on Linux, where the window toolkit reports none: X11 has no such concept
//! and Wayland's answer is only ever what the app itself asked for. The freedesktop portal knows, and says when it
//! changes. macOS and Windows report it through the window, so `light` is `None` there.

#[cfg(target_os = "linux")]
mod portal {
    use std::sync::atomic::{AtomicU8, Ordering};
    use std::sync::{Once, mpsc};
    use std::time::Duration;

    use zbus::zvariant::{OwnedValue, Value};

    const UNKNOWN: u8 = 0;
    const DARK: u8 = 1;
    const LIGHT: u8 = 2;
    const NAMESPACE: &str = "org.freedesktop.appearance";
    const KEY: &str = "color-scheme";

    static SCHEME: AtomicU8 = AtomicU8::new(UNKNOWN);
    static STARTED: Once = Once::new();

    /// The portal's value: 1 prefers dark, 2 prefers light, 0 no preference. Desktops default to light, so 0 is light.
    fn scheme(value: &Value) -> Option<u8> {
        match value {
            Value::U32(1) => Some(DARK),
            Value::U32(_) => Some(LIGHT),
            Value::Value(inner) => scheme(inner),
            _ => None,
        }
    }

    fn follow(ctx: &egui::Context, first: &mpsc::Sender<()>) -> zbus::Result<()> {
        let bus = zbus::blocking::Connection::session()?;
        let proxy = zbus::blocking::Proxy::new(
            &bus,
            "org.freedesktop.portal.Desktop",
            "/org/freedesktop/portal/desktop",
            "org.freedesktop.portal.Settings",
        )?;
        let changes = proxy.receive_signal("SettingChanged")?;
        let reply = proxy.call_method("Read", &(NAMESPACE, KEY))?;
        let value: OwnedValue = reply.body().deserialize()?;
        SCHEME.store(scheme(&value).unwrap_or(UNKNOWN), Ordering::Relaxed);
        let _ = first.send(());
        for change in changes {
            let body = change.body();
            let Ok((namespace, key, value)) = body.deserialize::<(String, String, OwnedValue)>() else { continue };
            if namespace == NAMESPACE && key == KEY {
                SCHEME.store(scheme(&value).unwrap_or(UNKNOWN), Ordering::Relaxed);
                ctx.request_repaint();
            }
        }
        Ok(())
    }

    pub fn watch(ctx: &egui::Context) {
        STARTED.call_once(|| {
            let ctx = ctx.clone();
            let (first, answered) = mpsc::channel();
            // No session bus or no portal just means no answer, and the app stays dark.
            let started = std::thread::Builder::new().name("system-theme".into()).spawn(move || {
                let _ = follow(&ctx, &first);
            });
            if started.is_ok() {
                // Long enough to open in the right theme, short enough that a silent portal costs nothing.
                let _ = answered.recv_timeout(Duration::from_millis(200));
            }
        });
    }

    pub fn light() -> Option<bool> {
        match SCHEME.load(Ordering::Relaxed) {
            DARK => Some(false),
            LIGHT => Some(true),
            _ => None,
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn the_portals_values_read_as_their_schemes() {
            assert_eq!(scheme(&Value::U32(1)), Some(DARK));
            assert_eq!(scheme(&Value::U32(2)), Some(LIGHT));
            assert_eq!(scheme(&Value::U32(0)), Some(LIGHT));
            assert_eq!(scheme(&Value::Value(Box::new(Value::Value(Box::new(Value::U32(1)))))), Some(DARK));
            assert_eq!(scheme(&Value::Bool(true)), None);
        }
    }
}

/// Starts following the desktop's preference. Only the real window calls this, so offscreen renders never depend on it.
pub fn watch(_ctx: &egui::Context) {
    #[cfg(target_os = "linux")]
    portal::watch(_ctx);
}

/// Whether the desktop prefers light, when only the portal can say.
pub fn light() -> Option<bool> {
    #[cfg(target_os = "linux")]
    return portal::light();
    #[cfg(not(target_os = "linux"))]
    None
}
