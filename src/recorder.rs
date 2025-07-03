use http::{HeaderMap, Method, StatusCode, Version};
use mime::Mime;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct RequestInfo<'a> {
    pub method: &'a Method,
    pub path: &'a str,
    pub version: Version,
    pub headers: &'a HeaderMap,
    pub body: &'a [u8],
    pub client_addr: String,
    pub truncate_at: usize,
}

pub struct ResponseInfo<'a> {
    pub request_id: &'a str,
    pub status: StatusCode,
    pub version: Version,
    pub headers: &'a HeaderMap,
    pub body: &'a [u8],
    pub duration_ms: u64,
    pub truncate_at: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestRecord {
    pub id: String,
    pub timestamp: u64,
    pub method: String,
    pub path: String,
    pub version: String,
    pub headers: Vec<(String, String)>,
    pub body: BodyRecord,
    pub client_addr: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseRecord {
    pub id: String,
    pub timestamp: u64,
    pub status: u16,
    pub version: String,
    pub headers: Vec<(String, String)>,
    pub body: BodyRecord,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BodyRecord {
    pub content_type: Option<String>,
    pub size: usize,
    pub preview: String,
    pub is_binary: bool,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpTransaction {
    pub request: RequestRecord,
    pub response: Option<ResponseRecord>,
    pub error: Option<String>,
}

pub struct RequestRecorder {
    transactions: Arc<RwLock<VecDeque<HttpTransaction>>>,
    max_size: usize,
}

impl RequestRecorder {
    pub fn new(max_size: usize) -> Self {
        Self {
            transactions: Arc::new(RwLock::new(VecDeque::with_capacity(max_size))),
            max_size,
        }
    }

    pub fn record_request(&self, info: RequestInfo) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let body_record = Self::analyze_body(info.body, info.headers, info.truncate_at);

        let request = RequestRecord {
            id: id.clone(),
            timestamp,
            method: info.method.to_string(),
            path: info.path.to_string(),
            version: format!("{:?}", info.version),
            headers: info
                .headers
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("<invalid>").to_string()))
                .collect(),
            body: body_record,
            client_addr: info.client_addr,
        };

        let transaction = HttpTransaction {
            request,
            response: None,
            error: None,
        };

        let mut transactions = self.transactions.write();
        if transactions.len() >= self.max_size {
            transactions.pop_front();
        }
        transactions.push_back(transaction);

        id
    }

    pub fn record_response(&self, info: ResponseInfo) {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let body_record = Self::analyze_body(info.body, info.headers, info.truncate_at);

        let response = ResponseRecord {
            id: info.request_id.to_string(),
            timestamp,
            status: info.status.as_u16(),
            version: format!("{:?}", info.version),
            headers: info
                .headers
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("<invalid>").to_string()))
                .collect(),
            body: body_record,
            duration_ms: info.duration_ms,
        };

        let mut transactions = self.transactions.write();
        if let Some(transaction) = transactions
            .iter_mut()
            .find(|t| t.request.id == info.request_id)
        {
            transaction.response = Some(response);
        }
    }

    pub fn record_error(&self, request_id: &str, error: String) {
        let mut transactions = self.transactions.write();
        if let Some(transaction) = transactions.iter_mut().find(|t| t.request.id == request_id) {
            transaction.error = Some(error);
        }
    }

    pub fn get_transactions(&self) -> Vec<HttpTransaction> {
        self.transactions.read().iter().cloned().collect()
    }

    #[allow(dead_code)]
    pub fn get_recent_transactions(&self, count: usize) -> Vec<HttpTransaction> {
        let transactions = self.transactions.read();
        transactions
            .iter()
            .rev()
            .take(count)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }

    pub fn clear(&self) {
        self.transactions.write().clear();
    }

    pub fn resize(&self, new_size: usize) {
        let mut transactions = self.transactions.write();
        while transactions.len() > new_size {
            transactions.pop_front();
        }
        transactions.reserve(new_size);
    }

    fn analyze_body(body: &[u8], headers: &HeaderMap, truncate_at: usize) -> BodyRecord {
        let size = body.len();
        let content_type = headers
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let is_binary = Self::is_binary_content(body, content_type.as_deref());
        let truncated = size > truncate_at;

        let preview = if is_binary {
            if size == 0 {
                String::new()
            } else {
                format!("<binary data: {} bytes>", size)
            }
        } else {
            let preview_bytes = if truncated {
                &body[..truncate_at.min(size)]
            } else {
                body
            };

            match std::str::from_utf8(preview_bytes) {
                Ok(s) => s.to_string(),
                Err(_) => format!("<invalid UTF-8: {} bytes>", size),
            }
        };

        BodyRecord {
            content_type,
            size,
            preview,
            is_binary,
            truncated,
        }
    }

    fn is_binary_content(data: &[u8], content_type: Option<&str>) -> bool {
        if data.is_empty() {
            return false;
        }

        // Check content type first
        if let Some(ct) = content_type {
            if let Ok(mime) = ct.parse::<Mime>() {
                match (mime.type_(), mime.subtype()) {
                    (mime::TEXT, _) => return false,
                    (mime::APPLICATION, mime::JSON) => return false,
                    (mime::APPLICATION, mime::JAVASCRIPT) => return false,
                    (mime::APPLICATION, subtype) if subtype == "xml" => return false,
                    (mime::APPLICATION, subtype) if subtype.as_str().ends_with("+json") => {
                        return false
                    }
                    (mime::APPLICATION, subtype) if subtype.as_str().ends_with("+xml") => {
                        return false
                    }
                    _ => {}
                }
            }
        }

        // Heuristic: check for null bytes or high ratio of non-printable characters
        let null_count = data.iter().filter(|&&b| b == 0).count();
        if null_count > 0 {
            return true;
        }

        let non_printable_count = data
            .iter()
            .filter(|&&b| b < 32 && b != b'\t' && b != b'\n' && b != b'\r')
            .count();

        // If more than 30% non-printable, consider it binary
        non_printable_count * 100 / data.len() > 30
    }
}

impl Clone for RequestRecorder {
    fn clone(&self) -> Self {
        Self {
            transactions: Arc::clone(&self.transactions),
            max_size: self.max_size,
        }
    }
}
