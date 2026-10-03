mod app;
mod diff_dialog;
mod injector;
mod list_row;
mod search;
mod settings_dialog;
mod snippet_dialog;
mod style;
mod transforms_dialog;
mod window;

use clap::Parser;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(
    name = "clipboard-history-gui",
    author,
    version,
    about = "Linux Universal Clipboard History Manager GUI Popup"
)]
struct Cli {
    #[arg(short, long, help = "Toggle the visibility of the popup window")]
    toggle: bool,

    #[arg(short, long, help = "Open preferences settings dialog")]
    settings: bool,
}

fn main() {
    let _ = Cli::parse();

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    #[cfg(feature = "gtk")]
    {
        let _ = libadwaita::init();
        info!("Launching native GTK4/Libadwaita Clipboard History popup");
        let app = app::ClipboardApp::new();
        std::process::exit(app.run());
    }

    #[cfg(not(feature = "gtk"))]
    {
        info!("Running clipboard-history-gui in standalone injector cascade mode");
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let cascade = InjectorCascade::new();
            let (name, success) = cascade.execute_paste().await;
            info!(
                "Paste injector probe completed: name={}, success={}",
                name, success
            );
        });
    }
}
