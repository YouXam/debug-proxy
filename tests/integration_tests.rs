use std::time::Duration;
use tokio::time::sleep;
use reqwest::Client;
use debug_proxy::{ProxyConfig, SharedConfig, RequestRecorder, DebugProxy};

#[tokio::test]
async fn test_proxy_basic_functionality() {
    // Start a simple upstream server
    let upstream_server = start_test_server(3001).await;
    
    // Create proxy configuration
    let config = ProxyConfig {
        client_timeout: Duration::from_secs(30),
        upstream_timeout: Duration::from_millis(500),
        max_history_size: 10,
        max_body_size: 1024,
        truncate_body_at: 256,
        access_token: "test-token".to_string(),
    };
    
    let shared_config = SharedConfig::new(config);
    let recorder = RequestRecorder::new(10);
    
    // Create proxy
    let proxy = DebugProxy::new(shared_config, recorder.clone(), "127.0.0.1:3001".to_string());
    
    // Start proxy server
    let proxy_server = start_proxy_server(proxy, 8081).await;
    
    // Wait for servers to be ready
    sleep(Duration::from_millis(100)).await;
    
    // Test basic request forwarding
    let client = Client::new();
    let response = client
        .get("http://localhost:8081/test")
        .send()
        .await
        .expect("Failed to send request");
    
    assert_eq!(response.status(), 200);
    let body = response.text().await.expect("Failed to read response body");
    assert_eq!(body, "Hello from test server");
    
    // Check that request was recorded
    let transactions = recorder.get_transactions();
    assert_eq!(transactions.len(), 1);
    assert_eq!(transactions[0].request.path, "/test");
    
    // Cleanup
    upstream_server.abort();
    proxy_server.abort();
}

#[tokio::test]
async fn test_upstream_timeout() {
    // Start a slow upstream server
    let upstream_server = start_slow_test_server(3002, Duration::from_secs(1)).await;
    
    // Create proxy configuration with short timeout
    let config = ProxyConfig {
        upstream_timeout: Duration::from_millis(100),
        ..Default::default()
    };
    
    let shared_config = SharedConfig::new(config);
    let recorder = RequestRecorder::new(10);
    let proxy = DebugProxy::new(shared_config, recorder.clone(), "127.0.0.1:3002".to_string());
    
    let proxy_server = start_proxy_server(proxy, 8082).await;
    
    // Wait for servers to be ready
    sleep(Duration::from_millis(100)).await;
    
    // Test timeout behavior
    let client = Client::new();
    let response = client
        .get("http://localhost:8082/slow")
        .send()
        .await
        .expect("Failed to send request");
    
    assert_eq!(response.status(), 503); // Service Unavailable due to timeout
    
    // Check that error was recorded
    let transactions = recorder.get_transactions();
    assert_eq!(transactions.len(), 1);
    assert!(transactions[0].error.is_some());
    
    // Cleanup
    upstream_server.abort();
    proxy_server.abort();
}

#[tokio::test]
async fn test_admin_interface() {
    let config = ProxyConfig {
        access_token: "test-admin-token".to_string(),
        ..Default::default()
    };
    
    let shared_config = SharedConfig::new(config);
    let recorder = RequestRecorder::new(10);
    let proxy = DebugProxy::new(shared_config, recorder, "127.0.0.1:3003".to_string());
    
    let proxy_server = start_proxy_server(proxy, 8083).await;
    
    // Wait for servers to be ready
    sleep(Duration::from_millis(100)).await;
    
    let client = Client::new();
    
    // Test admin interface access without token
    let response = client
        .get("http://localhost:8083/_proxy")
        .send()
        .await
        .expect("Failed to send request");
    
    assert_eq!(response.status(), 401); // Unauthorized
    
    // Test admin interface access with valid token
    let response = client
        .get("http://localhost:8083/_proxy?token=test-admin-token")
        .send()
        .await
        .expect("Failed to send request");
    
    assert_eq!(response.status(), 200);
    
    // Test config API
    let response = client
        .get("http://localhost:8083/_proxy/api/config?token=test-admin-token")
        .send()
        .await
        .expect("Failed to send request");
    
    assert_eq!(response.status(), 200);
    let config_json: serde_json::Value = response.json().await.expect("Failed to parse JSON");
    assert!(config_json["upstream_timeout_ms"].is_number());
    
    proxy_server.abort();
}

#[tokio::test]
async fn test_configuration_updates() {
    let shared_config = SharedConfig::default();
    let recorder = RequestRecorder::new(10);
    let proxy = DebugProxy::new(shared_config.clone(), recorder, "127.0.0.1:3004".to_string());
    
    let proxy_server = start_proxy_server(proxy, 8084).await;
    
    // Wait for servers to be ready
    sleep(Duration::from_millis(100)).await;
    
    let client = Client::new();
    let token = shared_config.get_access_token();
    
    // Update configuration
    let update_payload = serde_json::json!({
        "upstream_timeout_ms": 1000,
        "max_history_size": 50
    });
    
    let response = client
        .post(format!("http://localhost:8084/_proxy/api/config?token={}", token))
        .json(&update_payload)
        .send()
        .await
        .expect("Failed to send request");
    
    assert_eq!(response.status(), 200);
    
    // Verify configuration was updated
    let config = shared_config.read();
    assert_eq!(config.upstream_timeout.as_millis(), 1000);
    assert_eq!(config.max_history_size, 50);
    
    proxy_server.abort();
}

async fn start_test_server(port: u16) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        use std::convert::Infallible;
        use hyper::service::{make_service_fn, service_fn};
        use hyper::{Body, Request, Response, Server};
        
        let make_svc = make_service_fn(|_conn| async {
            Ok::<_, Infallible>(service_fn(|_req: Request<Body>| async {
                Ok::<_, Infallible>(Response::new(Body::from("Hello from test server")))
            }))
        });
        
        let addr = ([127, 0, 0, 1], port).into();
        let server = Server::bind(&addr).serve(make_svc);
        
        if let Err(e) = server.await {
            eprintln!("Test server error: {}", e);
        }
    })
}

async fn start_slow_test_server(port: u16, delay: Duration) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        use std::convert::Infallible;
        use hyper::service::{make_service_fn, service_fn};
        use hyper::{Body, Request, Response, Server};
        
        let make_svc = make_service_fn(move |_conn| {
            let delay = delay;
            async move {
                Ok::<_, Infallible>(service_fn(move |_req: Request<Body>| {
                    let delay = delay;
                    async move {
                        sleep(delay).await;
                        Ok::<_, Infallible>(Response::new(Body::from("Slow response")))
                    }
                }))
            }
        });
        
        let addr = ([127, 0, 0, 1], port).into();
        let server = Server::bind(&addr).serve(make_svc);
        
        if let Err(e) = server.await {
            eprintln!("Slow test server error: {}", e);
        }
    })
}

async fn start_proxy_server(proxy: DebugProxy, port: u16) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let addr = ([127, 0, 0, 1], port).into();
        if let Err(e) = proxy.start_server(addr).await {
            eprintln!("Proxy server error: {}", e);
        }
    })
}