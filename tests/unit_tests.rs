use debug_proxy::{
    ProcessManager, ProxyConfig, RequestInfo, RequestRecorder, ResponseInfo, SharedConfig,
};
use http::{HeaderMap, Method, StatusCode, Version};
use std::time::Duration;

#[test]
fn test_proxy_config_default() {
    let config = ProxyConfig::default();

    assert_eq!(config.client_timeout, Duration::from_secs(30));
    assert_eq!(config.upstream_timeout, Duration::from_millis(500));
    assert_eq!(config.max_history_size, 100);
    assert_eq!(config.max_body_size, 1024 * 1024);
    assert_eq!(config.truncate_body_at, 1024);
    assert!(!config.access_token.is_empty());
}

#[test]
fn test_shared_config_operations() {
    let config = ProxyConfig {
        upstream_timeout: Duration::from_millis(200),
        ..Default::default()
    };

    let shared_config = SharedConfig::new(config);

    // Test read access
    assert_eq!(
        shared_config.get_upstream_timeout(),
        Duration::from_millis(200)
    );

    // Test update
    shared_config.update(|c| {
        c.upstream_timeout = Duration::from_millis(300);
        c.max_history_size = 50;
    });

    assert_eq!(
        shared_config.get_upstream_timeout(),
        Duration::from_millis(300)
    );
    assert_eq!(shared_config.read().max_history_size, 50);
}

#[test]
fn test_config_update() {
    let update = debug_proxy::ConfigUpdate {
        client_timeout_ms: Some(5000),
        upstream_timeout_ms: Some(800),
        max_history_size: Some(200),
        max_body_size: None,
        truncate_body_at: Some(2048),
    };

    let mut config = ProxyConfig::default();
    update.apply_to(&mut config);

    assert_eq!(config.client_timeout, Duration::from_millis(5000));
    assert_eq!(config.upstream_timeout, Duration::from_millis(800));
    assert_eq!(config.max_history_size, 200);
    assert_eq!(config.truncate_body_at, 2048);
    // max_body_size should remain unchanged
    assert_eq!(config.max_body_size, 1024 * 1024);
}

#[test]
fn test_request_recorder() {
    let recorder = RequestRecorder::new(3);

    // Record first request
    let headers = HeaderMap::new();
    let body = b"test body";
    let request_info = RequestInfo {
        method: &Method::GET,
        path: "/test",
        version: Version::HTTP_11,
        headers: &headers,
        body,
        client_addr: "127.0.0.1:12345".to_string(),
        truncate_at: 100,
    };
    let request_id = recorder.record_request(request_info);

    // Record response
    let mut response_headers = HeaderMap::new();
    response_headers.insert("content-type", "text/plain".parse().unwrap());
    let response_info = ResponseInfo {
        request_id: &request_id,
        status: StatusCode::OK,
        version: Version::HTTP_11,
        headers: &response_headers,
        body: b"response body",
        duration_ms: 150,
        truncate_at: 100,
    };
    recorder.record_response(response_info);

    let transactions = recorder.get_transactions();
    assert_eq!(transactions.len(), 1);

    let transaction = &transactions[0];
    assert_eq!(transaction.request.method, "GET");
    assert_eq!(transaction.request.path, "/test");
    assert_eq!(transaction.request.body.size, 9);
    assert_eq!(transaction.request.body.preview, "test body");
    assert!(!transaction.request.body.is_binary);

    assert!(transaction.response.is_some());
    let response = transaction.response.as_ref().unwrap();
    assert_eq!(response.status, 200);
    assert_eq!(response.duration_ms, 150);
    assert_eq!(response.body.size, 13);
    assert_eq!(response.body.preview, "response body");
}

#[test]
fn test_request_recorder_binary_detection() {
    let recorder = RequestRecorder::new(10);

    // Test with binary data (contains null bytes)
    let binary_data = vec![0x89, 0x50, 0x4E, 0x47, 0x00, 0x0D, 0x0A, 0x1A];
    let headers = HeaderMap::new();

    let request_info = RequestInfo {
        method: &Method::POST,
        path: "/upload",
        version: Version::HTTP_11,
        headers: &headers,
        body: &binary_data,
        client_addr: "127.0.0.1:12345".to_string(),
        truncate_at: 100,
    };
    let _request_id = recorder.record_request(request_info);

    let transactions = recorder.get_transactions();
    let transaction = &transactions[0];

    assert!(transaction.request.body.is_binary);
    assert_eq!(transaction.request.body.preview, "<binary data: 8 bytes>");
}

#[test]
fn test_request_recorder_truncation() {
    let recorder = RequestRecorder::new(10);

    // Test with data longer than truncation limit
    let long_data = "x".repeat(200);
    let headers = HeaderMap::new();

    let request_info = RequestInfo {
        method: &Method::POST,
        path: "/long",
        version: Version::HTTP_11,
        headers: &headers,
        body: long_data.as_bytes(),
        client_addr: "127.0.0.1:12345".to_string(),
        truncate_at: 50, // Truncate at 50 bytes
    };
    let _request_id = recorder.record_request(request_info);

    let transactions = recorder.get_transactions();
    let transaction = &transactions[0];

    assert_eq!(transaction.request.body.size, 200);
    assert!(transaction.request.body.truncated);
    assert_eq!(transaction.request.body.preview.len(), 50);
}

#[test]
fn test_request_recorder_circular_buffer() {
    let recorder = RequestRecorder::new(2); // Only keep 2 transactions

    let headers = HeaderMap::new();

    // Add 3 requests
    for i in 0..3 {
        let request_info = RequestInfo {
            method: &Method::GET,
            path: &format!("/test{}", i),
            version: Version::HTTP_11,
            headers: &headers,
            body: b"body",
            client_addr: "127.0.0.1:12345".to_string(),
            truncate_at: 100,
        };
        recorder.record_request(request_info);
    }

    let transactions = recorder.get_transactions();
    assert_eq!(transactions.len(), 2); // Should only keep the last 2
    assert_eq!(transactions[0].request.path, "/test1");
    assert_eq!(transactions[1].request.path, "/test2");
}

#[test]
fn test_request_recorder_resize() {
    let recorder = RequestRecorder::new(5);

    let headers = HeaderMap::new();

    // Add 5 requests
    for i in 0..5 {
        let request_info = RequestInfo {
            method: &Method::GET,
            path: &format!("/test{}", i),
            version: Version::HTTP_11,
            headers: &headers,
            body: b"body",
            client_addr: "127.0.0.1:12345".to_string(),
            truncate_at: 100,
        };
        recorder.record_request(request_info);
    }

    assert_eq!(recorder.get_transactions().len(), 5);

    // Resize to smaller capacity
    recorder.resize(3);

    let transactions = recorder.get_transactions();
    assert_eq!(transactions.len(), 3);
    // Should keep the most recent 3
    assert_eq!(transactions[0].request.path, "/test2");
    assert_eq!(transactions[1].request.path, "/test3");
    assert_eq!(transactions[2].request.path, "/test4");
}

#[test]
fn test_request_recorder_error_handling() {
    let recorder = RequestRecorder::new(10);

    let headers = HeaderMap::new();
    let request_info = RequestInfo {
        method: &Method::GET,
        path: "/error",
        version: Version::HTTP_11,
        headers: &headers,
        body: b"body",
        client_addr: "127.0.0.1:12345".to_string(),
        truncate_at: 100,
    };
    let request_id = recorder.record_request(request_info);

    // Record an error
    recorder.record_error(&request_id, "Connection timeout".to_string());

    let transactions = recorder.get_transactions();
    assert_eq!(transactions.len(), 1);

    let transaction = &transactions[0];
    assert!(transaction.response.is_none());
    assert_eq!(transaction.error.as_ref().unwrap(), "Connection timeout");
}

#[test]
fn test_process_manager_creation() {
    let command = vec!["echo".to_string(), "hello".to_string()];
    let process_manager = ProcessManager::new(command.clone());

    assert!(!process_manager.is_running());
    assert!(process_manager.get_pid().is_none());
}

#[cfg(unix)]
#[test]
fn test_process_manager_lifecycle() {
    let command = vec!["sleep".to_string(), "0.1".to_string()];
    let process_manager = ProcessManager::new(command);

    // Start the process
    assert!(process_manager.start().is_ok());
    assert!(process_manager.is_running());
    assert!(process_manager.get_pid().is_some());

    // Stop the process
    assert!(process_manager.stop().is_ok());

    // Give it a moment to actually stop
    std::thread::sleep(Duration::from_millis(50));
    assert!(!process_manager.is_running());
}
