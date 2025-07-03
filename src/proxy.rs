use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use http::{header, Method, Request, Response, StatusCode};
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Client, Server};
use hyper_rustls::HttpsConnectorBuilder;
use tracing::{debug, error, info, warn};

use crate::config::SharedConfig;
use crate::recorder::{RequestInfo, RequestRecorder, ResponseInfo};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "ui/dist/"]
struct Assets;

pub struct DebugProxy {
    config: SharedConfig,
    recorder: RequestRecorder,
    upstream_address: String,
    client: Client<hyper_rustls::HttpsConnector<hyper::client::HttpConnector>>,
}

impl DebugProxy {
    pub fn new(config: SharedConfig, recorder: RequestRecorder, upstream_address: String) -> Self {
        let https = HttpsConnectorBuilder::new()
            .with_native_roots()
            .https_or_http()
            .enable_http1()
            .build();

        let client = Client::builder().build::<_, hyper::Body>(https);

        Self {
            config,
            recorder,
            upstream_address,
            client,
        }
    }

    pub async fn start_server(&self, listen_addr: SocketAddr) -> Result<()> {
        let proxy = Arc::new(self.clone());

        let make_svc = make_service_fn(move |_conn| {
            let proxy = Arc::clone(&proxy);
            async move {
                Ok::<_, Infallible>(service_fn(move |req| {
                    let proxy = Arc::clone(&proxy);
                    async move { proxy.handle_request(req).await }
                }))
            }
        });

        let server = Server::bind(&listen_addr).serve(make_svc);

        info!("Proxy server listening on {}", listen_addr);

        if let Err(e) = server.await {
            error!("Server error: {}", e);
        }

        Ok(())
    }

    async fn handle_request(&self, req: Request<Body>) -> Result<Response<Body>, Infallible> {
        let method = req.method().clone();
        let uri = req.uri().clone();
        let version = req.version();
        let headers = req.headers().clone();

        debug!("Incoming request: {} {}", method, uri.path());

        // Handle admin requests
        let is_admin_request = self.should_handle_admin_request(uri.path());
        debug!(
            "Should handle as admin request? {} for path: {}",
            is_admin_request,
            uri.path()
        );
        if is_admin_request {
            return Ok(self.handle_admin_request(req).await.unwrap_or_else(|e| {
                error!("Error handling admin request: {}", e);
                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::from("Internal Server Error"))
                    .unwrap()
            }));
        }

        // Handle proxy requests
        let client_addr = "unknown".to_string(); // In a real implementation, extract from connection
        let start_time = Instant::now();

        // Read request body
        let (_parts, body) = req.into_parts();
        let body_bytes = match hyper::body::to_bytes(body).await {
            Ok(bytes) => bytes.to_vec(),
            Err(e) => {
                error!("Error reading request body: {}", e);
                return Ok(Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(Body::from("Bad Request"))
                    .unwrap());
            }
        };

        // Record the request
        let (request_id, upstream_timeout) = {
            let config = self.config.read();
            let request_info = RequestInfo {
                method: &method,
                path: uri.path(),
                version,
                headers: &headers,
                body: &body_bytes,
                client_addr,
                truncate_at: config.truncate_body_at,
            };
            let request_id = self.recorder.record_request(request_info);
            let upstream_timeout = config.upstream_timeout;
            (request_id, upstream_timeout)
        };

        // Forward to upstream
        let upstream_uri = format!(
            "http://{}{}",
            self.upstream_address,
            uri.path_and_query().map(|pq| pq.as_str()).unwrap_or("")
        );

        let upstream_req = Request::builder()
            .method(&method)
            .uri(&upstream_uri)
            .version(version);

        let upstream_req = headers
            .into_iter()
            .fold(upstream_req, |req, (name, value)| {
                if let Some(name) = name {
                    req.header(name, value)
                } else {
                    req
                }
            });

        let upstream_req = upstream_req.body(Body::from(body_bytes)).unwrap();

        // Make upstream request with timeout
        let upstream_result =
            tokio::time::timeout(upstream_timeout, self.client.request(upstream_req)).await;

        match upstream_result {
            Ok(Ok(upstream_response)) => {
                let (parts, body) = upstream_response.into_parts();
                let response_bytes = match hyper::body::to_bytes(body).await {
                    Ok(bytes) => bytes.to_vec(),
                    Err(e) => {
                        error!("Error reading response body: {e}");
                        self.recorder
                            .record_error(&request_id, format!("Error reading response: {e}"));
                        return Ok(Response::builder()
                            .status(StatusCode::BAD_GATEWAY)
                            .body(Body::from("Bad Gateway"))
                            .unwrap());
                    }
                };

                let duration = start_time.elapsed();
                let truncate_at = {
                    let config = self.config.read();
                    config.truncate_body_at
                };

                let response_info = ResponseInfo {
                    request_id: &request_id,
                    status: parts.status,
                    version: parts.version,
                    headers: &parts.headers,
                    body: &response_bytes,
                    duration_ms: duration.as_millis() as u64,
                    truncate_at,
                };
                self.recorder.record_response(response_info);

                let mut response = Response::builder()
                    .status(parts.status)
                    .version(parts.version);

                response = parts
                    .headers
                    .into_iter()
                    .fold(response, |resp, (name, value)| {
                        if let Some(name) = name {
                            resp.header(name, value)
                        } else {
                            resp
                        }
                    });

                Ok(response.body(Body::from(response_bytes)).unwrap())
            }
            Ok(Err(e)) => {
                error!("Upstream request failed: {}", e);
                self.recorder
                    .record_error(&request_id, format!("Upstream error: {e}"));
                Ok(Response::builder()
                    .status(StatusCode::BAD_GATEWAY)
                    .body(Body::from("Bad Gateway"))
                    .unwrap())
            }
            Err(_) => {
                // Timeout occurred
                warn!("Upstream request timed out after {:?}", upstream_timeout);
                self.recorder
                    .record_error(&request_id, "Upstream timeout".to_string());
                Ok(Response::builder()
                    .status(StatusCode::SERVICE_UNAVAILABLE)
                    .body(Body::from("Service Unavailable - Upstream Timeout"))
                    .unwrap())
            }
        }
    }

    fn should_handle_admin_request(&self, path: &str) -> bool {
        path.starts_with("/_proxy")
    }

    async fn handle_admin_request(&self, req: Request<Body>) -> Result<Response<Body>> {
        let method = req.method();
        let uri = req.uri();
        let path = uri.path();
        let query = uri.query().unwrap_or("");

        let query_params: std::collections::HashMap<String, String> =
            url::form_urlencoded::parse(query.as_bytes())
                .into_owned()
                .collect();

        // Check token authentication
        let is_static_asset = path.starts_with("/_proxy/assets/");
        if !is_static_asset {
            let expected_token = self.config.get_access_token();
            let provided_token = query_params.get("token");

            debug!(
                "Token check - expected: {}, provided: {:?}",
                expected_token, provided_token
            );

            if provided_token != Some(&expected_token) {
                return Ok(Response::builder()
                    .status(StatusCode::UNAUTHORIZED)
                    .body(Body::from("Unauthorized - Invalid or missing token"))
                    .unwrap());
            }
        }

        let path_without_query = path;
        debug!("Admin request routing: {} {}", method, path_without_query);

        match (method, path_without_query) {
            (&Method::GET, "/_proxy") | (&Method::GET, "/_proxy/") => self.serve_admin_ui().await,
            (&Method::GET, "/_proxy/api/config") => self.serve_config().await,
            (&Method::POST, "/_proxy/api/config") => {
                let body_bytes = hyper::body::to_bytes(req.into_body()).await?;
                self.update_config(&body_bytes).await
            }
            (&Method::GET, "/_proxy/api/logs") => self.serve_logs().await,
            (&Method::DELETE, "/_proxy/api/logs") => self.clear_logs().await,
            (&Method::GET, path) if path.starts_with("/_proxy/assets/") => {
                self.serve_static_asset(path).await
            }
            _ => Ok(Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from("Not Found"))
                .unwrap()),
        }
    }

    async fn serve_admin_ui(&self) -> Result<Response<Body>> {
        // Serve the embedded React app
        let body = match Assets::get("index.html") {
            Some(content) => Body::from(content.data.as_ref().to_vec()),
            None => {
                let fallback = "<!DOCTYPE html><html><head><title>Debug Proxy</title></head><body><h1>Admin Interface</h1><p>page not found</p></body></html>";
                Body::from(fallback)
            }
        };

        Ok(Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/html")
            .body(body)
            .unwrap())
    }

    async fn serve_config(&self) -> Result<Response<Body>> {
        let config = self.config.read();
        let config_json = serde_json::json!({
            "client_timeout_ms": config.client_timeout.as_millis(),
            "upstream_timeout_ms": config.upstream_timeout.as_millis(),
            "max_history_size": config.max_history_size,
            "max_body_size": config.max_body_size,
            "truncate_body_at": config.truncate_body_at,
        });

        let response_body = serde_json::to_string(&config_json)?;

        Ok(Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(response_body))
            .unwrap())
    }

    async fn update_config(&self, body: &[u8]) -> Result<Response<Body>> {
        match serde_json::from_slice::<crate::config::ConfigUpdate>(body) {
            Ok(update) => {
                self.config.update(|config| {
                    update.apply_to(config);
                });

                // Update recorder size if changed
                if let Some(new_size) = update.max_history_size {
                    self.recorder.resize(new_size);
                }

                Ok(Response::builder()
                    .status(StatusCode::OK)
                    .body(Body::from("Configuration updated"))
                    .unwrap())
            }
            Err(e) => {
                let error_msg = format!("Invalid configuration: {e}");
                Ok(Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(Body::from(error_msg))
                    .unwrap())
            }
        }
    }

    async fn serve_logs(&self) -> Result<Response<Body>> {
        let transactions = self.recorder.get_transactions();
        let response_body = serde_json::to_string(&transactions)?;

        Ok(Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(response_body))
            .unwrap())
    }

    async fn clear_logs(&self) -> Result<Response<Body>> {
        self.recorder.clear();
        Ok(Response::builder()
            .status(StatusCode::OK)
            .body(Body::from("Logs cleared"))
            .unwrap())
    }

    async fn serve_static_asset(&self, path: &str) -> Result<Response<Body>> {
        // Convert /_proxy/assets/... to relative path
        let asset_path = path.strip_prefix("/_proxy/").unwrap_or(path);
        debug!("Serving embedded asset: {}", asset_path);

        match Assets::get(asset_path) {
            Some(content) => {
                let content_type = content.metadata.mimetype();

                Ok(Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, content_type)
                    .body(Body::from(content.data.as_ref().to_vec()))
                    .unwrap())
            }
            None => {
                debug!("Embedded asset not found: {}", asset_path);
                Ok(Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(Body::from("Asset not found"))
                    .unwrap())
            }
        }
    }
}

impl Clone for DebugProxy {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            recorder: self.recorder.clone(),
            upstream_address: self.upstream_address.clone(),
            client: self.client.clone(),
        }
    }
}
