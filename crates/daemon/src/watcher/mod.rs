pub mod klipper;
pub mod traits;
pub mod wayland;
pub mod x11;

pub use klipper::KlipperWatcher;
#[allow(unused_imports)]
pub use traits::{ClipboardWatcher, RawClipboardEvent, SelectionTarget};
pub use wayland::WaylandWatcher;
pub use x11::X11Watcher;
