use crate::injector::copy_only::CopyOnlyInjector;
use crate::injector::traits::PasteInjector;
use crate::injector::wtype::WtypeInjector;
use crate::injector::x11_xtest::X11XTestInjector;
use crate::injector::xdotool::XdotoolInjector;
use crate::injector::ydotool::YdotoolInjector;
use tracing::info;

pub struct InjectorCascade {
    injectors: Vec<Box<dyn PasteInjector>>,
}

impl InjectorCascade {
    pub fn new() -> Self {
        let is_wayland = std::env::var("WAYLAND_DISPLAY").is_ok()
            || std::env::var("XDG_SESSION_TYPE").unwrap_or_default() == "wayland";

        let mut injectors: Vec<Box<dyn PasteInjector>> = Vec::new();

        if is_wayland {
            injectors.push(Box::new(WtypeInjector::new()));
            injectors.push(Box::new(YdotoolInjector::new()));
        } else {
            injectors.push(Box::new(X11XTestInjector::new()));
            injectors.push(Box::new(XdotoolInjector::new()));
        }

        // Always fallback to copy-only
        injectors.push(Box::new(CopyOnlyInjector::new()));

        Self { injectors }
    }

    pub async fn execute_paste(&self) -> (&'static str, bool) {
        for injector in &self.injectors {
            if injector.is_available() {
                info!(
                    "Attempting synthetic paste via injector: {}",
                    injector.name()
                );
                if injector.inject().await {
                    return (injector.name(), true);
                }
            }
        }

        ("none", false)
    }
}

impl Default for InjectorCascade {
    fn default() -> Self {
        Self::new()
    }
}
