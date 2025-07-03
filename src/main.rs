use anyhow::{Context, Result};
use clap::Parser;
use std::process::exit;
use tracing::{error, info, warn};

mod config;
mod process;
mod proxy;
mod recorder;

use config::{ProxyConfig, SharedConfig};
use process::ProcessManager;
use proxy::DebugProxy;
use recorder::RequestRecorder;

#[derive(Parser)]
#[command(name = "debug-proxy")]
#[command(about = "HTTP debugging reverse proxy with timeout handling")]
struct Args {
    #[arg(help = "Upstream target in format host:port (e.g., 192.168.1.1:3000, localhost:3000)")]
    upstream: String,

    #[arg(short, long, default_value = "8080", help = "Local port to listen on")]
    port: u16,

    #[arg(long, default_value = "0.0.0.0", help = "Host address to bind to")]
    host: String,

    #[arg(
        short,
        long,
        default_value = "500",
        help = "Upstream timeout in milliseconds"
    )]
    upstream_timeout: u64,

    #[arg(
        short,
        long,
        default_value = "30000",
        help = "Client timeout in milliseconds"
    )]
    client_timeout: u64,

    #[arg(
        short,
        long,
        default_value = "100",
        help = "Maximum number of requests to keep in history"
    )]
    max_history: usize,

    #[arg(long, default_value = "1024", help = "Body truncation size in bytes")]
    truncate_body: usize,

    #[arg(
        last = true,
        help = "Command to run as upstream service (use -- before command)"
    )]
    command: Vec<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let args = Args::parse();

    // Parse upstream target
    let upstream_addr = parse_upstream_target(&args.upstream).context(
        "Invalid upstream target format. Use format: host:port (e.g., 192.168.1.1:3000)",
    )?;
    let local_port = args.port;

    // Create configuration
    let config = ProxyConfig {
        upstream_timeout: std::time::Duration::from_millis(args.upstream_timeout),
        client_timeout: std::time::Duration::from_millis(args.client_timeout),
        max_history_size: args.max_history,
        truncate_body_at: args.truncate_body,
        ..Default::default()
    };

    let shared_config = SharedConfig::new(config);
    let access_token = shared_config.get_access_token();

    // Create request recorder
    let recorder = RequestRecorder::new(args.max_history);

    // Create process manager and start upstream service
    let process_manager = if !args.command.is_empty() {
        let pm = ProcessManager::new(args.command.clone());
        pm.start()
            .with_context(|| format!("Failed to start upstream command: {:?}", args.command))?;
        Some(pm)
    } else {
        None
    };

    // Create proxy service
    let proxy = DebugProxy::new(shared_config, recorder, upstream_addr.clone());

    // Print startup information
    println!("🚀 DebugProxy started successfully!");
    println!();
    println!("📊 Proxy Configuration:");
    println!("  Listen Address:   {}:{}", args.host, local_port);
    println!("  Upstream Target:  {}", upstream_addr);
    println!("  Client Timeout:   {}ms", args.client_timeout);
    println!("  Upstream Timeout: {}ms", args.upstream_timeout);
    println!("  Max History:      {} requests", args.max_history);
    println!("  Body Truncation:  {} bytes", args.truncate_body);
    println!();
    println!("🌐 Web Interface:");
    let web_host = if args.host == "0.0.0.0" {
        "localhost"
    } else {
        &args.host
    };
    println!(
        "  URL: http://{}:{}/_proxy?token={}",
        web_host, local_port, access_token
    );
    println!();
    println!("🔧 Upstream Process:");
    if let Some(ref pm) = process_manager {
        if let Some(pid) = pm.get_pid() {
            println!("  Status: PID {} (running)", pid);
        } else {
            println!("  Status: Not running");
        }
    } else {
        println!("  Status: External (not managed)");
    }
    println!();
    println!("Ready to receive requests. Press Ctrl+C to stop.");

    // Set up signal handling
    // Clone process manager for signal handler if it exists
    let process_manager_for_signal = process_manager.clone();
    tokio::spawn(async move {
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to register SIGTERM handler");
        let mut sigint = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
            .expect("Failed to register SIGINT handler");

        tokio::select! {
            _ = sigterm.recv() => {
                info!("Received SIGTERM, shutting down gracefully...");
            }
            _ = sigint.recv() => {
                info!("Received SIGINT, shutting down gracefully...");
            }
        }

        if let Some(pm) = process_manager_for_signal {
            info!("Stopping upstream process...");
            if let Err(e) = pm.stop() {
                error!("Error stopping upstream process: {}", e);
            }
        }

        info!("Shutdown complete");
        exit(0);
    });

    // Start the proxy server (non-blocking)
    let host_addr: std::net::IpAddr = args
        .host
        .parse()
        .with_context(|| format!("Invalid host address: {}", args.host))?;
    let listen_addr = (host_addr, local_port).into();

    // Start the proxy server and monitor for failures
    let server_handle = tokio::spawn(async move {
        if let Err(e) = proxy.start_server(listen_addr).await {
            error!("Proxy server error: {}", e);
            std::process::exit(1);
        }
    });

    // Keep the main thread alive and monitor subprocess
    loop {
        // Check if server task has completed (which means it failed)
        if server_handle.is_finished() {
            error!("Proxy server has stopped unexpectedly");
            if let Some(ref pm) = process_manager {
                if let Err(e) = pm.stop() {
                    error!("Error stopping subprocess: {}", e);
                }
            }
            std::process::exit(1);
        }

        // Monitor subprocess if it exists
        if let Some(ref pm) = process_manager {
            if !pm.is_running() {
                warn!("Subprocess has exited unexpectedly, restarting...");
                if let Err(e) = pm.restart() {
                    error!("Failed to restart subprocess: {}", e);
                } else {
                    info!("Subprocess restarted successfully");
                }
            }
        }

        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}

fn parse_upstream_target(target: &str) -> Result<String> {
    // Validate the format host:port
    let parts: Vec<&str> = target.split(':').collect();
    if parts.len() != 2 {
        return Err(anyhow::anyhow!(
            "Upstream target must be in format host:port"
        ));
    }

    // Validate host part (not empty)
    if parts[0].is_empty() {
        return Err(anyhow::anyhow!("Host part cannot be empty"));
    }

    // Validate port part
    let _port = parts[1].parse::<u16>().context("Invalid port number")?;

    Ok(target.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_upstream_target() {
        assert_eq!(
            parse_upstream_target("localhost:3000").unwrap(),
            "localhost:3000"
        );
        assert_eq!(
            parse_upstream_target("192.168.1.1:8080").unwrap(),
            "192.168.1.1:8080"
        );

        assert!(parse_upstream_target("localhost").is_err());
        assert!(parse_upstream_target("localhost:3000:9000").is_err());
        assert!(parse_upstream_target(":3000").is_err());
        assert!(parse_upstream_target("localhost:invalid").is_err());
    }
}
