use anyhow::Result;
use goose_cli::cli::cli;

#[tokio::main]
async fn main() -> Result<()> {
    let is_serve = std::env::args().any(|a| a == "serve");
    let logging_result = if is_serve {
        goose_cli::logging::setup_logging_with_console(Some("serve"))
    } else {
        goose_cli::logging::setup_logging(None)
    };
    if let Err(e) = logging_result {
        eprintln!("Warning: Failed to initialize logging: {}", e);
    }

    let result = cli().await;

    #[cfg(feature = "otel")]
    if goose::otel::otlp::is_otlp_initialized() {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        goose::otel::otlp::shutdown_otlp();
    }

    result
}
