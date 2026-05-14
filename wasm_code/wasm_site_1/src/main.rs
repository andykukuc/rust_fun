mod linux;

use tiny_http::{Server, Response, Header};
use std::str::FromStr;
use std::path::Path;
use std::fs;

const STATIC_DIR: &str = "/mnt/samba_pool/samba/Coding/Rust/rust_fun/wasm_code/wasm_site_1";

fn content_type(path: &str) -> &'static str {
    if path.ends_with(".html") { "text/html" }
    else if path.ends_with(".js") { "application/javascript" }
    else if path.ends_with(".wasm") { "application/wasm" }
    else if path.ends_with(".json") { "application/json" }
    else { "text/plain" }
}

fn main() {
    let server = Server::http("0.0.0.0:1235").unwrap();
    println!("Server listening on http://0.0.0.0:1235");

    for request in server.incoming_requests() {
        let url = request.url().to_string();

        if url == "/sysinfo" || url.starts_with("/sysinfo?") {
            let data = linux::get_system_info();
            let body = data.to_string();
            let response = Response::from_string(body)
                .with_header(Header::from_str("Content-Type: application/json").unwrap())
                .with_header(Header::from_str("Access-Control-Allow-Origin: *").unwrap());
            let _ = request.respond(response);
        } else {
            let file_path = if url == "/" {
                format!("{}/index.html", STATIC_DIR)
            } else {
                format!("{}{}", STATIC_DIR, url)
            };

            let path = Path::new(&file_path);
            if path.exists() && path.is_file() {
                let ct = content_type(&file_path);
                match fs::read(&file_path) {
                    Ok(bytes) => {
                        let response = Response::from_data(bytes)
                            .with_header(Header::from_str(&format!("Content-Type: {}", ct)).unwrap());
                        let _ = request.respond(response);
                    }
                    Err(_) => {
                        let _ = request.respond(
                            Response::from_string("500 Internal Server Error").with_status_code(500)
                        );
                    }
                }
            } else {
                let _ = request.respond(
                    Response::from_string("404 Not Found").with_status_code(404)
                );
            }
        }
    }
}
