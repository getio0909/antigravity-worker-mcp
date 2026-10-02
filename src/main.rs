use antigravity_worker_mcp::{
    jobs::Worker,
    model::{Config, Failure, Outcome},
    server::Server,
};
use rmcp::ServiceExt;

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("{}", serde_json::json!({"error":error}));
        std::process::exit(1);
    }
}
async fn run() -> Outcome<()> {
    let mut args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() == 1 && args[0] == "--help" {
        println!("Usage: antigravity-worker-mcp --config /absolute/path/config.json [--no-audit]");
        return Ok(());
    }
    if args.len() == 1 && args[0] == "--version" {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let no_audit = args.iter().any(|arg| arg == "--no-audit");
    args.retain(|arg| arg != "--no-audit");
    if args.len() != 2 || args[0] != "--config" {
        return Err(Failure::new(
            "CONFIG_REQUIRED",
            "Use --config with a configuration file path.",
        ));
    }
    let mut config = Config::load(std::path::Path::new(&args[1]))?;
    if no_audit {
        config.audit_logging = false;
    }
    let worker = Worker::new(config)?;
    let audit = worker.audit.clone();
    let service = Server::new(worker.clone())
        .serve((
            audit.reader(tokio::io::stdin()),
            audit.writer(tokio::io::stdout()),
        ))
        .await
        .map_err(|_| Failure::new("MCP_FAILED", "MCP initialization failed."))?;
    let termination = async {
        #[cfg(unix)]
        if let Ok(mut term) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            term.recv().await;
            return;
        }
        std::future::pending::<()>().await;
    };
    tokio::select! { _ = service.waiting() => {}, _ = tokio::signal::ctrl_c() => {}, _ = termination => {} }
    worker.shutdown().await;
    audit.record(
        "connection.close",
        serde_json::json!({"audit_failed":audit.has_failed()}),
    )?;
    audit.flush()?;
    Ok(())
}
