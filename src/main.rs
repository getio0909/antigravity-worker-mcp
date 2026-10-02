use antigravity_worker_mcp::{
    jobs::Worker,
    model::{Config, Failure, Outcome},
    server::Server,
};
use rmcp::{ServiceExt, transport::stdio};

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("{}", serde_json::json!({"error":error}));
        std::process::exit(1);
    }
}
async fn run() -> Outcome<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() == 1 && args[0] == "--help" {
        println!("Usage: antigravity-worker-mcp --config /absolute/path/config.json");
        return Ok(());
    }
    if args.len() == 1 && args[0] == "--version" {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if args.len() != 2 || args[0] != "--config" {
        return Err(Failure::new(
            "CONFIG_REQUIRED",
            "Use --config with a configuration file path.",
        ));
    }
    let config = Config::load(std::path::Path::new(&args[1]))?;
    let worker = Worker::new(config);
    let service = Server::new(worker.clone())
        .serve(stdio())
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
    Ok(())
}
