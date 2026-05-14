use clap::Parser;
use warp::Filter;
use warp::http::Method;
use serde::{Deserialize, Serialize};
use std::process::Command;
use std::env;
use warp::fs; // For serving static files

/// Command-line arguments for the server
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Port to bind the server to
    #[arg(short, long, default_value_t = 3030)]
    port: u16,
}

#[tokio::main]
async fn main() {
    // Parse command-line arguments
    let args = Args::parse();

    // Define CORS policy
    let cors = warp::cors()
        .allow_any_origin()
        .allow_method(Method::GET);

    // Define the route to execute a Linux command
    let execute_command_route = warp::path!("execute" / String)
        .map(|command: String| {
            let output = Command::new("sh")
                .arg("-c")
                .arg(command)
                .output();

            let response = match output {
                Ok(output) if output.status.success() => Response {
                    success: true,
                    message: String::from_utf8_lossy(&output.stdout).to_string(),
                },
                Ok(output) => Response {
                    success: false,
                    message: String::from_utf8_lossy(&output.stderr).to_string(),
                },
                Err(err) => Response {
                    success: false,
                    message: format!("Failed to execute command: {}", err),
                },
            };

            warp::reply::json(&response)
        });

    // Define the route to get the current working directory
    let current_dir_route = warp::path!("current_dir")
        .map(|| {
            let current_dir = env::current_dir()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|_| "Failed to get current directory".to_string());

            warp::reply::json(&Response {
                success: true,
                message: current_dir,
            })
        });

    // Serve static files from the "frontend" directory
    let static_files_route = warp::fs::dir("../frontend");

    // Combine routes and apply CORS
    let routes = execute_command_route
        .or(current_dir_route)
        .or(static_files_route)
        .with(cors);

    // Start the server on 0.0.0.0
    println!("Server running on http://0.0.0.0:{}", args.port);
    warp::serve(routes)
        .run(([0, 0, 0, 0], args.port))
        .await;
}

// Response structure for JSON
#[derive(Serialize, Deserialize)]
struct Response {
    success: bool,
    message: String,
}
