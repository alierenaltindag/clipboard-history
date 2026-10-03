mod clipboard;
mod commands;

use clap::{Parser, Subcommand};
use clipboard_history_core::config::AppConfig;
use commands::*;

#[derive(Parser, Debug)]
#[command(
    name = "clipboard-history",
    author,
    version,
    about = "Linux Universal Clipboard History Manager CLI (Win+V alternative)"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    #[command(about = "Toggle the popup GUI window (invokes clipboard-history-gui)")]
    Toggle,

    #[command(about = "List clipboard history items")]
    List {
        #[arg(
            short,
            long,
            default_value = "20",
            help = "Max number of items to display"
        )]
        limit: usize,

        #[arg(short, long, default_value = "0", help = "Offset")]
        offset: usize,

        #[arg(
            short,
            long,
            help = "Filter by type (text, html, image, urilist, code)"
        )]
        r#type: Option<String>,

        #[arg(short, long, help = "Show only pinned items")]
        pinned: bool,
    },

    #[command(about = "Search clipboard history")]
    Search {
        #[arg(help = "Search query string")]
        query: String,

        #[arg(short, long, default_value = "20")]
        limit: usize,
    },

    #[command(about = "Pin an entry by ID to prevent automatic eviction")]
    Pin {
        #[arg(help = "Entry ID")]
        id: String,
    },

    #[command(about = "Unpin an entry by ID")]
    Unpin {
        #[arg(help = "Entry ID")]
        id: String,
    },

    #[command(about = "Delete an entry by ID")]
    Delete {
        #[arg(help = "Entry ID")]
        id: String,
    },

    #[command(about = "Clear clipboard history")]
    Clear {
        #[arg(short, long, help = "Delete all entries including pinned")]
        all: bool,
    },

    #[command(about = "Pause clipboard tracking")]
    Pause,

    #[command(about = "Resume clipboard tracking")]
    Resume,

    #[command(about = "Display daemon status and metrics")]
    Status,

    #[command(about = "Add a text entry to clipboard history manually")]
    Add {
        #[arg(help = "Text to add")]
        text: String,
    },

    #[command(about = "Manage permanent canned snippets")]
    Snippets {
        #[command(subcommand)]
        action: Option<SnippetAction>,
    },

    #[command(about = "Manage sequential paste queue")]
    Queue {
        #[command(subcommand)]
        action: Option<QueueAction>,
    },

    #[command(about = "Extract text from an image entry using OCR")]
    Ocr {
        #[arg(help = "Entry ID or blob hash")]
        id: String,
    },

    #[command(about = "Concatenate multiple clipboard entries and copy to clipboard")]
    Join {
        #[arg(short, long, help = "Custom delimiter (default: newline)")]
        delimiter: Option<String>,
        #[arg(short, long, help = "Format as numbered list (1. ..., 2. ...)")]
        numbered: bool,
        #[arg(short, long, help = "Format as bulleted list (- ..., - ...)")]
        bullets: bool,
        #[arg(help = "Entry IDs to concatenate in order")]
        ids: Vec<String>,
    },

    #[command(about = "Compare and display visual diff between two entries")]
    Diff {
        #[arg(help = "First entry ID (Original / A)")]
        id_a: String,
        #[arg(help = "Second entry ID (Modified / B)")]
        id_b: String,
        #[arg(long, help = "Disable ANSI color formatting in output")]
        no_color: bool,
    },

    #[command(about = "Delete multiple entries by ID in batch")]
    BatchDelete {
        #[arg(help = "Entry IDs to delete")]
        ids: Vec<String>,
    },

    #[command(about = "Pin or unpin multiple entries in batch")]
    BatchPin {
        #[arg(long, help = "Unpin instead of pin")]
        unpin: bool,
        #[arg(help = "Entry IDs to pin/unpin")]
        ids: Vec<String>,
    },

    #[command(about = "Clean tracking parameters from a URL or clipboard entry")]
    CleanUrl {
        #[arg(help = "URL string or clipboard entry ID")]
        target: String,
    },

    #[command(about = "Manage clipboard history configuration settings")]
    Config {
        #[command(subcommand)]
        action: Option<ConfigAction>,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let socket_path = AppConfig::socket_path();

    match cli.command {
        Commands::Toggle => handle_toggle(&socket_path).await,
        Commands::List {
            limit,
            offset,
            r#type,
            pinned,
        } => handle_list(&socket_path, limit, offset, r#type, pinned).await,
        Commands::Search { query, limit } => handle_search(&socket_path, query, limit).await,
        Commands::Pin { id } => handle_pin(&socket_path, id).await,
        Commands::Unpin { id } => handle_unpin(&socket_path, id).await,
        Commands::Delete { id } => handle_delete(&socket_path, id).await,
        Commands::Clear { all } => handle_clear(&socket_path, all).await,
        Commands::Pause => handle_pause(&socket_path).await,
        Commands::Resume => handle_resume(&socket_path).await,
        Commands::Status => handle_status(&socket_path).await,
        Commands::Add { text } => handle_add(&socket_path, text).await,
        Commands::Snippets { action } => handle_snippets(&socket_path, action).await,
        Commands::Queue { action } => handle_queue(&socket_path, action).await,
        Commands::Ocr { id } => handle_ocr(&socket_path, id).await,
        Commands::Join {
            delimiter,
            numbered,
            bullets,
            ids,
        } => handle_join(&socket_path, delimiter, numbered, bullets, ids).await,
        Commands::Diff {
            id_a,
            id_b,
            no_color,
        } => handle_diff(&socket_path, id_a, id_b, no_color).await,
        Commands::BatchDelete { ids } => handle_batch_delete(&socket_path, ids).await,
        Commands::BatchPin { unpin, ids } => handle_batch_pin(&socket_path, unpin, ids).await,
        Commands::CleanUrl { target } => handle_clean_url(&socket_path, target).await,
        Commands::Config { action } => handle_config(&socket_path, action).await?,
    }

    Ok(())
}
