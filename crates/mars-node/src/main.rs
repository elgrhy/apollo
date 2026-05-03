use clap::{Parser, Subcommand};
use anyhow::Result;
use mars_runtime::process::ProcessRuntime;
use std::sync::Arc;
use std::path::PathBuf;
use mars_core::types::AgentSpec;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the MARS Node Agent daemon
    Run {
        #[arg(short, long, default_value = "0.0.0.0:8080")]
        listen: String,
        
        #[arg(short, long, default_value = ".mars/agents")]
        base_dir: PathBuf,
    },
    /// Install an agent locally (CLI helper)
    Install {
        #[arg(short, long)]
        config: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Run { listen, base_dir } => {
            println!("MARS Node Agent starting on {}", listen);
            println!("Storage directory: {:?}", base_dir);
            
            let runtime = Arc::new(ProcessRuntime::new(base_dir));
            
            // Start the HTTP server for the provider API
            run_api_server(&listen, runtime).await?;
        }
        Commands::Install { config } => {
            let content = std::fs::read_to_string(config)?;
            let spec: AgentSpec = serde_yaml::from_str(&content)?;
            println!("Installing agent: {}", spec.name);
            // ... implementation
        }
    }

    Ok(())
}

async fn run_api_server(listen: &str, runtime: Arc<ProcessRuntime>) -> Result<()> {
    let server = tiny_http::Server::http(listen).map_err(|e| anyhow::anyhow!(e))?;
    println!("API Server listening on http://{}", listen);

    for mut request in server.incoming_requests() {
        let runtime = Arc::clone(&runtime);
        match (request.method(), request.url()) {
            (&tiny_http::Method::Post, "/agents/install") => {
                let mut content = String::new();
                request.as_reader().read_to_string(&mut content)?;
                
                let spec: AgentSpec = match serde_json::from_str(&content) {
                    Ok(s) => s,
                    Err(_) => {
                        let response = tiny_http::Response::from_string("{\"error\": \"Invalid AgentSpec\"}").with_status_code(400);
                        request.respond(response)?;
                        continue;
                    }
                };

                match runtime.install(&spec).await {
                    Ok(_) => {
                        let response = tiny_http::Response::from_string("{\"status\": \"installed\"}");
                        request.respond(response)?;
                    }
                    Err(e) => {
                        let response = tiny_http::Response::from_string(format!("{{\"error\": \"{:?}\"}}", e)).with_status_code(500);
                        request.respond(response)?;
                    }
                }
            }
            (&tiny_http::Method::Get, "/agents/status") => {
                let response = tiny_http::Response::from_string("{\"status\": \"healthy\"}");
                request.respond(response)?;
            }
            _ => {
                let response = tiny_http::Response::from_string("Not Found").with_status_code(404);
                request.respond(response)?;
            }
        }
    }

    Ok(())
}
