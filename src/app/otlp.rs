use super::*;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError};
use std::thread::JoinHandle;
use std::time::Duration;

const OTLP_BATCH_SIZE: usize = 64;
const OTLP_QUEUE_CAPACITY: usize = 512;
const OTLP_BATCH_DELAY: Duration = Duration::from_millis(250);
const OTLP_DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) struct OtlpExporter {
    logs: Option<OtlpLogExporter>,
    traces: Option<OtlpSignalExporter>,
    metrics: Option<OtlpSignalExporter>,
}

impl OtlpExporter {
    pub(crate) fn from_environment() -> Result<Option<Self>> {
        let logs = OtlpLogExporter::from_environment()?;
        let traces = OtlpSignalExporter::from_environment(OtlpSignal::Traces)?;
        let metrics = OtlpSignalExporter::from_environment(OtlpSignal::Metrics)?;
        if logs.is_none() && traces.is_none() && metrics.is_none() {
            return Ok(None);
        }
        Ok(Some(Self {
            logs,
            traces,
            metrics,
        }))
    }

    pub(crate) fn emit_http_access(&self, event: &Value) {
        if let Some(logs) = &self.logs {
            logs.emit_http_access(event);
        }
        if let Some(traces) = &self.traces {
            traces.emit(otlp_span_record(event));
        }
        if let Some(metrics) = &self.metrics {
            metrics.emit(otlp_metric_record(event));
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum OtlpSignal {
    Traces,
    Metrics,
}

impl OtlpSignal {
    fn name(self) -> &'static str {
        match self {
            Self::Traces => "traces",
            Self::Metrics => "metrics",
        }
    }

    fn path(self) -> &'static str {
        match self {
            Self::Traces => "v1/traces",
            Self::Metrics => "v1/metrics",
        }
    }

    fn endpoint_env(self) -> &'static str {
        match self {
            Self::Traces => "OTEL_EXPORTER_OTLP_TRACES_ENDPOINT",
            Self::Metrics => "OTEL_EXPORTER_OTLP_METRICS_ENDPOINT",
        }
    }

    fn protocol_env(self) -> &'static str {
        match self {
            Self::Traces => "OTEL_EXPORTER_OTLP_TRACES_PROTOCOL",
            Self::Metrics => "OTEL_EXPORTER_OTLP_METRICS_PROTOCOL",
        }
    }

    fn headers_env(self) -> &'static str {
        match self {
            Self::Traces => "OTEL_EXPORTER_OTLP_TRACES_HEADERS",
            Self::Metrics => "OTEL_EXPORTER_OTLP_METRICS_HEADERS",
        }
    }

    fn timeout_env(self) -> &'static str {
        match self {
            Self::Traces => "OTEL_EXPORTER_OTLP_TRACES_TIMEOUT",
            Self::Metrics => "OTEL_EXPORTER_OTLP_METRICS_TIMEOUT",
        }
    }
}

struct OtlpSignalExporter {
    signal: OtlpSignal,
    sender: Option<SyncSender<Value>>,
    shutdown: std::sync::Arc<std::sync::atomic::AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl OtlpSignalExporter {
    fn from_environment(signal: OtlpSignal) -> Result<Option<Self>> {
        let Some(endpoint) = otlp_signal_endpoint(signal)? else {
            return Ok(None);
        };
        let protocol = nonempty_env(signal.protocol_env())
            .or_else(|| nonempty_env("OTEL_EXPORTER_OTLP_PROTOCOL"))
            .unwrap_or_else(|| "http/json".to_string());
        if protocol != "http/json" {
            bail!(
                "DukeMemory's native {} exporter supports OTLP/HTTP JSON; set {}=http/json",
                signal.name(),
                signal.protocol_env()
            );
        }
        Self::new(
            signal,
            &endpoint,
            otlp_signal_timeout(signal)?,
            otlp_signal_headers(signal)?,
        )
        .map(Some)
    }

    fn new(
        signal: OtlpSignal,
        endpoint: &str,
        timeout: Duration,
        headers: HeaderMap,
    ) -> Result<Self> {
        let (client, endpoint) = egress::blocking_http_client(endpoint, timeout)
            .with_context(|| format!("invalid OTLP {} endpoint", signal.name()))?;
        let (sender, receiver) = std::sync::mpsc::sync_channel(OTLP_QUEUE_CAPACITY);
        let shutdown = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let worker_shutdown = std::sync::Arc::clone(&shutdown);
        let worker = std::thread::Builder::new()
            .name(format!("dukememory-otlp-{}", signal.name()))
            .spawn(move || {
                export_signal_batches(
                    signal,
                    receiver,
                    client,
                    endpoint,
                    headers,
                    &worker_shutdown,
                )
            })
            .with_context(|| format!("failed to start OTLP {} exporter", signal.name()))?;
        Ok(Self {
            signal,
            sender: Some(sender),
            shutdown,
            worker: Some(worker),
        })
    }

    fn emit(&self, record: Value) {
        let Some(sender) = &self.sender else {
            return;
        };
        match sender.try_send(record) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => eprintln!(
                "OTLP {} queue is full; dropping one HTTP record",
                self.signal.name()
            ),
            Err(TrySendError::Disconnected(_)) => eprintln!(
                "OTLP {} exporter stopped; dropping one HTTP record",
                self.signal.name()
            ),
        }
    }
}

impl Drop for OtlpSignalExporter {
    fn drop(&mut self) {
        self.shutdown
            .store(true, std::sync::atomic::Ordering::Release);
        self.sender.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

pub(crate) struct OtlpLogExporter {
    sender: Option<SyncSender<Value>>,
    shutdown: std::sync::Arc<std::sync::atomic::AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl OtlpLogExporter {
    pub(crate) fn from_environment() -> Result<Option<Self>> {
        let Some(endpoint) = otlp_logs_endpoint()? else {
            return Ok(None);
        };
        let protocol = std::env::var("OTEL_EXPORTER_OTLP_LOGS_PROTOCOL")
            .ok()
            .or_else(|| std::env::var("OTEL_EXPORTER_OTLP_PROTOCOL").ok())
            .unwrap_or_else(|| "http/json".to_string());
        if protocol.trim() != "http/json" {
            bail!(
                "DukeMemory's native logs exporter supports OTLP/HTTP JSON; set OTEL_EXPORTER_OTLP_LOGS_PROTOCOL=http/json"
            );
        }
        Self::new(&endpoint, otlp_timeout()?, otlp_headers()?).map(Some)
    }

    fn new(endpoint: &str, timeout: Duration, headers: HeaderMap) -> Result<Self> {
        let (client, endpoint) = egress::blocking_http_client(endpoint, timeout)
            .context("invalid OTLP logs endpoint")?;
        let (sender, receiver) = std::sync::mpsc::sync_channel(OTLP_QUEUE_CAPACITY);
        let shutdown = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let worker_shutdown = std::sync::Arc::clone(&shutdown);
        let worker = std::thread::Builder::new()
            .name("dukememory-otlp-logs".to_string())
            .spawn(move || export_batches(receiver, client, endpoint, headers, &worker_shutdown))
            .context("failed to start OTLP logs exporter")?;
        Ok(Self {
            sender: Some(sender),
            shutdown,
            worker: Some(worker),
        })
    }

    pub(crate) fn emit_http_access(&self, event: &Value) {
        let Some(sender) = &self.sender else {
            return;
        };
        let record = otlp_log_record(event);
        match sender.try_send(record) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                eprintln!("OTLP logs queue is full; dropping one access record")
            }
            Err(TrySendError::Disconnected(_)) => {
                eprintln!("OTLP logs exporter stopped; dropping one access record")
            }
        }
    }
}

impl Drop for OtlpLogExporter {
    fn drop(&mut self) {
        self.shutdown
            .store(true, std::sync::atomic::Ordering::Release);
        self.sender.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

pub(crate) fn environment_status() -> &'static str {
    if nonempty_env("OTEL_EXPORTER_OTLP_ENDPOINT").is_some() {
        return "otlp_http_json_logs_traces_metrics";
    }
    let configured = [
        "OTEL_EXPORTER_OTLP_LOGS_ENDPOINT",
        "OTEL_EXPORTER_OTLP_TRACES_ENDPOINT",
        "OTEL_EXPORTER_OTLP_METRICS_ENDPOINT",
    ]
    .into_iter()
    .filter(|name| nonempty_env(name).is_some())
    .count();
    match configured {
        0 => "disabled",
        3 => "otlp_http_json_logs_traces_metrics",
        _ => "otlp_http_json_partial",
    }
}

fn otlp_logs_endpoint() -> Result<Option<String>> {
    if let Some(endpoint) = nonempty_env("OTEL_EXPORTER_OTLP_LOGS_ENDPOINT") {
        return Ok(Some(endpoint));
    }
    let Some(base) = nonempty_env("OTEL_EXPORTER_OTLP_ENDPOINT") else {
        return Ok(None);
    };
    let mut url = reqwest::Url::parse(&base).context("invalid OTEL_EXPORTER_OTLP_ENDPOINT")?;
    let path = format!("{}/v1/logs", url.path().trim_end_matches('/'));
    url.set_path(&path);
    Ok(Some(url.to_string()))
}

fn otlp_signal_endpoint(signal: OtlpSignal) -> Result<Option<String>> {
    if let Some(endpoint) = nonempty_env(signal.endpoint_env()) {
        return Ok(Some(endpoint));
    }
    let Some(base) = nonempty_env("OTEL_EXPORTER_OTLP_ENDPOINT") else {
        return Ok(None);
    };
    let mut url = reqwest::Url::parse(&base).context("invalid OTEL_EXPORTER_OTLP_ENDPOINT")?;
    let path = format!("{}/{}", url.path().trim_end_matches('/'), signal.path());
    url.set_path(&path);
    Ok(Some(url.to_string()))
}

fn otlp_timeout() -> Result<Duration> {
    let raw = nonempty_env("OTEL_EXPORTER_OTLP_LOGS_TIMEOUT")
        .or_else(|| nonempty_env("OTEL_EXPORTER_OTLP_TIMEOUT"));
    let Some(raw) = raw else {
        return Ok(OTLP_DEFAULT_TIMEOUT);
    };
    let milliseconds = raw
        .parse::<u64>()
        .context("OTLP timeout must be an integer number of milliseconds")?;
    if !(1..=60_000).contains(&milliseconds) {
        bail!("OTLP timeout must be between 1 and 60000 milliseconds");
    }
    Ok(Duration::from_millis(milliseconds))
}

fn otlp_headers() -> Result<HeaderMap> {
    parse_otlp_headers([
        nonempty_env("OTEL_EXPORTER_OTLP_HEADERS"),
        nonempty_env("OTEL_EXPORTER_OTLP_LOGS_HEADERS"),
    ])
}

fn otlp_signal_headers(signal: OtlpSignal) -> Result<HeaderMap> {
    parse_otlp_headers([
        nonempty_env("OTEL_EXPORTER_OTLP_HEADERS"),
        nonempty_env(signal.headers_env()),
    ])
}

fn parse_otlp_headers<const N: usize>(values: [Option<String>; N]) -> Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    for raw in values.into_iter().flatten() {
        for pair in raw
            .split(',')
            .map(str::trim)
            .filter(|pair| !pair.is_empty())
        {
            let (name, value) = pair
                .split_once('=')
                .context("OTLP headers must use key=value pairs")?;
            let name = HeaderName::from_bytes(name.trim().as_bytes())
                .context("invalid OTLP header name")?;
            let value = HeaderValue::from_str(value.trim()).context("invalid OTLP header value")?;
            headers.insert(name, value);
        }
    }
    Ok(headers)
}

fn otlp_signal_timeout(signal: OtlpSignal) -> Result<Duration> {
    let raw =
        nonempty_env(signal.timeout_env()).or_else(|| nonempty_env("OTEL_EXPORTER_OTLP_TIMEOUT"));
    let Some(raw) = raw else {
        return Ok(OTLP_DEFAULT_TIMEOUT);
    };
    let milliseconds = raw
        .parse::<u64>()
        .context("OTLP timeout must be an integer number of milliseconds")?;
    if !(1..=60_000).contains(&milliseconds) {
        bail!("OTLP timeout must be between 1 and 60000 milliseconds");
    }
    Ok(Duration::from_millis(milliseconds))
}

fn nonempty_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn export_batches(
    receiver: Receiver<Value>,
    client: reqwest::blocking::Client,
    endpoint: reqwest::Url,
    headers: HeaderMap,
    shutdown: &std::sync::atomic::AtomicBool,
) {
    loop {
        let first = match receiver.recv_timeout(OTLP_BATCH_DELAY) {
            Ok(record) => record,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
        };
        let mut records = vec![first];
        while records.len() < OTLP_BATCH_SIZE {
            match receiver.try_recv() {
                Ok(record) => records.push(record),
                Err(_) => break,
            }
        }
        let body = otlp_logs_payload(records);
        match client
            .post(endpoint.clone())
            .headers(headers.clone())
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .json(&body)
            .send()
        {
            Ok(response) if response.status().is_success() => {}
            Ok(response) => eprintln!(
                "OTLP logs export failed with collector status {}",
                response.status().as_u16()
            ),
            Err(err) => eprintln!("OTLP logs export failed: {err}"),
        }
        if shutdown.load(std::sync::atomic::Ordering::Acquire) {
            return;
        }
    }
}

fn export_signal_batches(
    signal: OtlpSignal,
    receiver: Receiver<Value>,
    client: reqwest::blocking::Client,
    endpoint: reqwest::Url,
    headers: HeaderMap,
    shutdown: &std::sync::atomic::AtomicBool,
) {
    loop {
        let first = match receiver.recv_timeout(OTLP_BATCH_DELAY) {
            Ok(record) => record,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
        };
        let mut records = vec![first];
        while records.len() < OTLP_BATCH_SIZE {
            match receiver.try_recv() {
                Ok(record) => records.push(record),
                Err(_) => break,
            }
        }
        let body = match signal {
            OtlpSignal::Traces => otlp_traces_payload(records),
            OtlpSignal::Metrics => otlp_metrics_payload(records),
        };
        match client
            .post(endpoint.clone())
            .headers(headers.clone())
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .json(&body)
            .send()
        {
            Ok(response) if response.status().is_success() => {}
            Ok(response) => eprintln!(
                "OTLP {} export failed with collector status {}",
                signal.name(),
                response.status().as_u16()
            ),
            Err(err) => eprintln!("OTLP {} export failed: {err}", signal.name()),
        }
        if shutdown.load(std::sync::atomic::Ordering::Acquire) {
            return;
        }
    }
}

fn unix_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

fn otlp_span_record(event: &Value) -> Value {
    let request_id = event["request_id"].as_str().unwrap_or("unknown");
    let digest = format!("{:x}", Sha256::digest(request_id.as_bytes()));
    let end = unix_nanos();
    let elapsed_ms = event["elapsed_ms"].as_u64().unwrap_or_default() as u128;
    let start = end.saturating_sub(elapsed_ms.saturating_mul(1_000_000));
    let status = event["status"].as_u64().unwrap_or_default();
    let method = event["method"].as_str().unwrap_or("UNKNOWN");
    let path = event["path"].as_str().unwrap_or("/");
    json!({
        "traceId": &digest[..32],
        "spanId": &digest[32..48],
        "name": format!("{method} {path}"),
        "kind": 2,
        "startTimeUnixNano": start.to_string(),
        "endTimeUnixNano": end.to_string(),
        "attributes": [
            {"key": "http.request.method", "value": {"stringValue": method}},
            {"key": "url.path", "value": {"stringValue": path}},
            {"key": "http.response.status_code", "value": {"intValue": status.to_string()}},
            {"key": "dukememory.request_id", "value": {"stringValue": request_id}}
        ],
        "status": {
            "code": if status >= 500 { 2 } else { 1 }
        }
    })
}

fn otlp_metric_record(event: &Value) -> Value {
    let now = unix_nanos().to_string();
    let method = event["method"].as_str().unwrap_or("UNKNOWN");
    let status = event["status"].as_u64().unwrap_or_default();
    let attributes = json!([
        {"key": "http.request.method", "value": {"stringValue": method}},
        {"key": "http.response.status_code", "value": {"intValue": status.to_string()}}
    ]);
    json!({
        "timeUnixNano": now,
        "attributes": attributes,
        "elapsedMs": event["elapsed_ms"].as_f64().unwrap_or_default()
    })
}

fn otlp_traces_payload(spans: Vec<Value>) -> Value {
    json!({
        "resourceSpans": [{
            "resource": otlp_resource(),
            "scopeSpans": [{
                "scope": {"name": "dukememory.http", "version": env!("CARGO_PKG_VERSION")},
                "spans": spans
            }]
        }]
    })
}

fn otlp_metrics_payload(records: Vec<Value>) -> Value {
    let count_points = records
        .iter()
        .map(|record| {
            json!({
                "timeUnixNano": record["timeUnixNano"],
                "attributes": record["attributes"],
                "asInt": "1"
            })
        })
        .collect::<Vec<_>>();
    let duration_points = records
        .iter()
        .map(|record| {
            json!({
                "timeUnixNano": record["timeUnixNano"],
                "attributes": record["attributes"],
                "asDouble": record["elapsedMs"]
            })
        })
        .collect::<Vec<_>>();
    json!({
        "resourceMetrics": [{
            "resource": otlp_resource(),
            "scopeMetrics": [{
                "scope": {"name": "dukememory.http", "version": env!("CARGO_PKG_VERSION")},
                "metrics": [
                    {
                        "name": "http.server.request.count",
                        "unit": "{request}",
                        "sum": {
                            "aggregationTemporality": 1,
                            "isMonotonic": true,
                            "dataPoints": count_points
                        }
                    },
                    {
                        "name": "http.server.request.duration",
                        "unit": "ms",
                        "gauge": {"dataPoints": duration_points}
                    }
                ]
            }]
        }]
    })
}

fn otlp_resource() -> Value {
    json!({"attributes": [
        {"key": "service.name", "value": {"stringValue": "dukememory"}},
        {"key": "service.version", "value": {"stringValue": env!("CARGO_PKG_VERSION")}}
    ]})
}

fn otlp_log_record(event: &Value) -> Value {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .to_string();
    let mut attributes = Vec::new();
    if let Some(object) = event.as_object() {
        for (key, value) in object {
            if key == "event" {
                continue;
            }
            let value = match value {
                Value::String(value) => json!({"stringValue": value}),
                Value::Number(value) => json!({"intValue": value.to_string()}),
                Value::Bool(value) => json!({"boolValue": value}),
                other => json!({"stringValue": other.to_string()}),
            };
            attributes.push(json!({"key": key, "value": value}));
        }
    }
    json!({
        "timeUnixNano": now,
        "observedTimeUnixNano": now,
        "severityNumber": 9,
        "severityText": "INFO",
        "body": {"stringValue": event["event"].as_str().unwrap_or("http_access")},
        "attributes": attributes
    })
}

fn otlp_logs_payload(records: Vec<Value>) -> Value {
    json!({
        "resourceLogs": [{
            "resource": otlp_resource(),
            "scopeLogs": [{
                "scope": {"name": "dukememory.http", "version": env!("CARGO_PKG_VERSION")},
                "logRecords": records
            }]
        }]
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    #[test]
    fn exporter_batches_valid_otlp_json_to_logs_endpoint() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut bytes = Vec::new();
            let mut buffer = [0_u8; 1024];
            loop {
                let count = stream.read(&mut buffer).unwrap();
                bytes.extend_from_slice(&buffer[..count]);
                let Some(header_end) = bytes.windows(4).position(|window| window == b"\r\n\r\n")
                else {
                    continue;
                };
                let headers = String::from_utf8_lossy(&bytes[..header_end]);
                let content_length = headers
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().ok())
                            .flatten()
                    })
                    .unwrap();
                if bytes.len() >= header_end + 4 + content_length {
                    break;
                }
            }
            let request = String::from_utf8_lossy(&bytes);
            assert!(request.starts_with("POST /v1/logs HTTP/1.1"));
            let body = request.split_once("\r\n\r\n").unwrap().1;
            let body: Value = serde_json::from_str(body).unwrap();
            assert_eq!(
                body["resourceLogs"][0]["scopeLogs"][0]["logRecords"][0]["body"]["stringValue"],
                "http_access"
            );
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}")
                .unwrap();
        });
        let exporter = OtlpLogExporter::new(
            &format!("http://{address}/v1/logs"),
            Duration::from_secs(2),
            HeaderMap::new(),
        )
        .unwrap();
        exporter.emit_http_access(&json!({
            "event": "http_access",
            "request_id": "abc",
            "status": 200
        }));
        drop(exporter);
        server.join().unwrap();
    }

    #[test]
    fn otlp_payload_uses_protobuf_json_field_shapes() {
        let record = otlp_log_record(&json!({
            "event": "http_access",
            "elapsed_ms": 12,
            "authenticated": true
        }));
        assert!(record["timeUnixNano"].is_string());
        assert_eq!(record["severityNumber"], 9);
        assert!(
            record["attributes"]
                .as_array()
                .unwrap()
                .iter()
                .any(|attribute| {
                    attribute["key"] == "elapsed_ms" && attribute["value"]["intValue"] == "12"
                })
        );
    }

    #[test]
    fn trace_and_metric_payloads_use_bounded_http_semantic_fields() {
        let event = json!({
            "event": "http_access",
            "request_id": "request-123",
            "method": "GET",
            "path": "/memory",
            "status": 200,
            "elapsed_ms": 12
        });
        let span = otlp_span_record(&event);
        assert_eq!(span["traceId"].as_str().unwrap().len(), 32);
        assert_eq!(span["spanId"].as_str().unwrap().len(), 16);
        assert_eq!(span["name"], "GET /memory");
        assert_eq!(span["status"]["code"], 1);

        let traces = otlp_traces_payload(vec![span]);
        assert_eq!(
            traces["resourceSpans"][0]["scopeSpans"][0]["spans"][0]["name"],
            "GET /memory"
        );
        let metrics = otlp_metrics_payload(vec![otlp_metric_record(&event)]);
        assert_eq!(
            metrics["resourceMetrics"][0]["scopeMetrics"][0]["metrics"][0]["name"],
            "http.server.request.count"
        );
        assert_eq!(
            metrics["resourceMetrics"][0]["scopeMetrics"][0]["metrics"][1]["name"],
            "http.server.request.duration"
        );
        assert_eq!(
            metrics["resourceMetrics"][0]["scopeMetrics"][0]["metrics"][1]["gauge"]["dataPoints"]
                [0]["asDouble"],
            12.0
        );
        let metric_attributes =
            metrics["resourceMetrics"][0]["scopeMetrics"][0]["metrics"][0]["sum"]["dataPoints"][0]
                ["attributes"]
                .as_array()
                .unwrap();
        assert!(
            metric_attributes
                .iter()
                .all(|attribute| attribute["key"] != "url.path")
        );
    }
}
