pub mod traits;
pub mod wayland;
pub mod x11;

#[allow(unused_imports)]
pub use traits::{ClipboardWatcher, RawClipboardEvent};
pub use wayland::WaylandWatcher;
pub use x11::X11Watcher;
