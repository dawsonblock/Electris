use std::panic::AssertUnwindSafe;
use electro_core::paths;
use tracing_subscriber::prelude::*;

pub fn init_logging(is_tui: bool) {
    if is_tui {
        // TUI mode: write logs to ~/.electro/tui.log so they don't corrupt the display
        let log_dir = paths::electro_home();
        std::fs::create_dir_all(&log_dir).ok();
        if let Ok(log_file) = std::fs::File::create(paths::tui_log_file()) {
            tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
                )
                .with_writer(std::sync::Mutex::new(log_file))
                .with_ansi(false)
                .json()
                .init();
        }
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
            )
            .json()
            .init();
    }
}

pub fn init_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let payload = if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else if let Some(s) = info.payload().downcast_ref::<&str>() {
            s.to_string()
        } else {
            "unknown panic payload".to_string()
        };
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "unknown".to_string());
        tracing::error!(
            panic.payload = %payload,
            panic.location = %location,
            "PANIC caught — task will attempt recovery"
        );
    }));
}
