use cfg_if::cfg_if;
use tracing_subscriber::{layer::SubscriberExt, EnvFilter};
use tracing_subscriber::util::SubscriberInitExt;

cfg_if! {
    if #[cfg(target_arch = "wasm32")] {
        pub fn init() {
            // Log to browser console via tracing-wasm
            let filter = EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info"));

            let wasm_layer = tracing_wasm::WASMLayer::new(tracing_wasm::WASMLayerConfig::default());

            tracing_subscriber::registry()
                .with(filter)
                .with(wasm_layer)
                .init();

            // Panics with stacktrace
            #[cfg(feature = "console_error_panic_hook")]
            console_error_panic_hook::set_once();
        }
    } else {
        use tracing_appender::non_blocking::WorkerGuard;
        use tracing_subscriber::fmt;
        use std::env;
        use std::io;
        use once_cell::sync::OnceCell;

        static FILE_GUARD: OnceCell<WorkerGuard> = OnceCell::new();

        pub fn init() {
            // Env filter: use RUST_LOG or default to info
            let filter = EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info"));

            // Console (stderr) layer with file/line
            let console_layer = fmt::layer()
                .with_writer(io::stderr)
                .with_target(true)
                .with_file(true)
                .with_line_number(true)
                .with_level(true)
                .compact();

            // Optional file logging (RUST_LOG_FILE=logs/app.log or default logs/app.log)
            let log_path = env::var("RUST_LOG_FILE").unwrap_or_else(|_| "logs/app.log".to_string());
            let (nb_writer, guard) = tracing_appender::non_blocking(
                tracing_appender::rolling::daily(
                    std::path::Path::new(&log_path).parent().unwrap_or(std::path::Path::new(".")),
                    std::path::Path::new(&log_path).file_name().unwrap_or(std::ffi::OsStr::new("app.log")),
                )
            );
            let _ = FILE_GUARD.set(guard);

            let file_layer = fmt::layer()
                .with_writer(nb_writer)
                .with_target(true)
                .with_file(true)
                .with_line_number(true)
                .with_level(true)
                .compact();

            tracing_subscriber::registry()
                .with(filter)
                .with(console_layer)
                .with(file_layer)
                .init();

            // Hook panics to log with backtrace
            std::panic::set_hook(Box::new(|info| {
                let mut msg = String::new();
                if let Some(loc) = info.location() {
                    msg.push_str(&format!("panic at {}:{}:{} ", loc.file(), loc.line(), loc.column()));
                }
                if let Some(s) = info.payload().downcast_ref::<&str>() { msg.push_str(s); }
                else if let Some(s) = info.payload().downcast_ref::<String>() { msg.push_str(s); }
                else { msg.push_str("<non-string panic>"); }
                let bt = std::backtrace::Backtrace::force_capture();
                tracing::error!("{}\nBacktrace:\n{:?}", msg, bt);
            }));
        }
    }
}
