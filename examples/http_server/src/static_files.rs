//! Static file serving with MIME type detection

use http_body_util::Full;
use hyper::body::Bytes;
use hyper::{Response, StatusCode};
use percent_encoding::percent_decode_str;
use std::path::{Path, PathBuf};
use tokio::fs;

/// Result of attempting to serve a file
pub enum FileResponse {
    /// File found and served
    Found(Response<Full<Bytes>>),
    /// File not found
    NotFound,
    /// Directory found (may need autoindex)
    Directory(PathBuf),
    /// Error occurred
    Error(String),
}

/// Serve a static file
pub async fn serve_file(
    root: &str,
    path: &str,
    index_files: &[String],
) -> FileResponse {
    // Decode percent-encoded path
    let decoded_path = match percent_decode_str(path).decode_utf8() {
        Ok(p) => p.to_string(),
        Err(_) => return FileResponse::Error("Invalid path encoding".to_string()),
    };

    // Remove leading slash and sanitize
    let clean_path = decoded_path.trim_start_matches('/');
    
    // Prevent directory traversal
    if clean_path.contains("..") {
        return FileResponse::Error("Directory traversal not allowed".to_string());
    }

    let mut file_path = PathBuf::from(root);
    if !clean_path.is_empty() {
        file_path.push(clean_path);
    }

    // Check if path exists
    match fs::metadata(&file_path).await {
        Ok(meta) => {
            if meta.is_dir() {
                // Try index files
                for index in index_files {
                    let index_path = file_path.join(index);
                    if let Ok(index_meta) = fs::metadata(&index_path).await {
                        if index_meta.is_file() {
                            return serve_single_file(&index_path).await;
                        }
                    }
                }
                // Return directory for potential autoindex
                FileResponse::Directory(file_path)
            } else {
                serve_single_file(&file_path).await
            }
        }
        Err(_) => FileResponse::NotFound,
    }
}

/// Serve a single file
async fn serve_single_file(path: &Path) -> FileResponse {
    match fs::read(path).await {
        Ok(contents) => {
            let mime = mime_guess::from_path(path)
                .first_or_octet_stream()
                .to_string();

            let response = Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", mime)
                .header("Content-Length", contents.len())
                .body(Full::new(Bytes::from(contents)))
                .unwrap();

            FileResponse::Found(response)
        }
        Err(e) => FileResponse::Error(format!("Failed to read file: {}", e)),
    }
}

/// Generate directory listing HTML
pub async fn generate_autoindex(dir_path: &Path, request_path: &str) -> Result<String, std::io::Error> {
    let mut entries = Vec::new();
    let mut read_dir = fs::read_dir(dir_path).await?;

    while let Some(entry) = read_dir.next_entry().await? {
        let file_name = entry.file_name().to_string_lossy().to_string();
        let meta = entry.metadata().await?;
        let is_dir = meta.is_dir();
        let size = if is_dir { "-".to_string() } else { format_size(meta.len()) };
        
        entries.push((file_name, is_dir, size));
    }

    // Sort: directories first, then by name
    entries.sort_by(|a, b| {
        match (a.1, b.1) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.0.cmp(&b.0),
        }
    });

    let mut html = String::new();
    html.push_str("<!DOCTYPE html>\n<html>\n<head>\n");
    html.push_str(&format!("<title>Index of {}</title>\n", request_path));
    html.push_str("<style>\n");
    html.push_str("body { font-family: monospace; padding: 20px; }\n");
    html.push_str("table { border-collapse: collapse; }\n");
    html.push_str("th, td { padding: 5px 20px; text-align: left; }\n");
    html.push_str("a { text-decoration: none; color: #0066cc; }\n");
    html.push_str("a:hover { text-decoration: underline; }\n");
    html.push_str(".dir { font-weight: bold; }\n");
    html.push_str("</style>\n");
    html.push_str("</head>\n<body>\n");
    html.push_str(&format!("<h1>Index of {}</h1>\n", request_path));
    html.push_str("<hr>\n<table>\n");
    html.push_str("<tr><th>Name</th><th>Size</th></tr>\n");

    // Parent directory link
    if request_path != "/" {
        html.push_str("<tr><td><a href=\"..\">..</a></td><td>-</td></tr>\n");
    }

    for (name, is_dir, size) in entries {
        let display_name = if is_dir {
            format!("{}/", name)
        } else {
            name.clone()
        };
        let class = if is_dir { " class=\"dir\"" } else { "" };
        let href = if is_dir {
            format!("{}/", name)
        } else {
            name
        };
        html.push_str(&format!(
            "<tr><td><a href=\"{}\"{}>{}</a></td><td>{}</td></tr>\n",
            href, class, display_name, size
        ));
    }

    html.push_str("</table>\n<hr>\n");
    html.push_str("<p><em>Pulsive HTTP Server</em></p>\n");
    html.push_str("</body>\n</html>");

    Ok(html)
}

/// Format file size for display
fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1}G", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1}M", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1}K", bytes as f64 / KB as f64)
    } else {
        format!("{}", bytes)
    }
}

/// Create an error response
pub fn error_response(status: StatusCode, message: &str) -> Response<Full<Bytes>> {
    let body = format!(
        "<!DOCTYPE html>\n<html>\n<head><title>{} {}</title></head>\n\
         <body>\n<h1>{} {}</h1>\n<p>{}</p>\n<hr>\n\
         <p><em>Pulsive HTTP Server</em></p>\n</body>\n</html>",
        status.as_u16(),
        status.canonical_reason().unwrap_or("Error"),
        status.as_u16(),
        status.canonical_reason().unwrap_or("Error"),
        message
    );

    Response::builder()
        .status(status)
        .header("Content-Type", "text/html; charset=utf-8")
        .body(Full::new(Bytes::from(body)))
        .unwrap()
}

/// Create a redirect response
pub fn redirect_response(status: StatusCode, location: &str) -> Response<Full<Bytes>> {
    Response::builder()
        .status(status)
        .header("Location", location)
        .body(Full::new(Bytes::new()))
        .unwrap()
}
