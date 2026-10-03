pub mod daemon_ctrl;
pub mod entries;
pub mod queue;
pub mod snippets;
pub mod transforms;

pub use daemon_ctrl::{handle_pause, handle_resume, handle_status, handle_toggle};
pub use entries::{
    handle_add, handle_batch_delete, handle_batch_pin, handle_clear, handle_delete, handle_list,
    handle_pin, handle_search, handle_unpin,
};
pub use queue::{handle_queue, QueueAction};
pub use snippets::{handle_snippets, SnippetAction};
pub use transforms::{handle_clean_url, handle_diff, handle_join, handle_ocr};

pub fn print_daemon_offline_error() {
    eprintln!("Error: Could not connect to clipboard-history-daemon.");
    eprintln!("Ensure the daemon is running:");
    eprintln!("  systemctl --user start clipboard-history");
    eprintln!("  or run: clipboard-history-daemon &");
}
