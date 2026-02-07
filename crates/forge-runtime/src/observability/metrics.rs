use opentelemetry::{
    KeyValue, global,
    metrics::{Counter, Histogram, UpDownCounter},
};
use std::sync::OnceLock;

const METER_NAME: &str = "forge-runtime";

static HTTP_METRICS: OnceLock<HttpMetrics> = OnceLock::new();
static JOB_METRICS: OnceLock<JobMetrics> = OnceLock::new();
static CONNECTIONS_GAUGE: OnceLock<ActiveConnectionsGauge> = OnceLock::new();

pub struct HttpMetrics {
    requests_total: Counter<u64>,
    request_duration: Histogram<f64>,
}

impl HttpMetrics {
    fn new() -> Self {
        let meter = global::meter(METER_NAME);

        let requests_total = meter
            .u64_counter("http_requests_total")
            .with_description("Total number of HTTP requests")
            .with_unit("requests")
            .build();

        let request_duration = meter
            .f64_histogram("http_request_duration_seconds")
            .with_description("HTTP request duration in seconds")
            .with_unit("s")
            .build();

        Self {
            requests_total,
            request_duration,
        }
    }

    pub fn record(&self, method: &str, path: &str, status: u16, duration_secs: f64) {
        let attributes = [
            KeyValue::new("method", method.to_string()),
            KeyValue::new("path", path.to_string()),
            KeyValue::new("status", i64::from(status)),
        ];

        self.requests_total.add(1, &attributes);
        self.request_duration.record(duration_secs, &attributes);
    }
}

pub struct JobMetrics {
    executions_total: Counter<u64>,
    duration: Histogram<f64>,
}

impl JobMetrics {
    fn new() -> Self {
        let meter = global::meter(METER_NAME);

        let executions_total = meter
            .u64_counter("job_executions_total")
            .with_description("Total number of job executions")
            .with_unit("executions")
            .build();

        let duration = meter
            .f64_histogram("job_duration_seconds")
            .with_description("Job execution duration in seconds")
            .with_unit("s")
            .build();

        Self {
            executions_total,
            duration,
        }
    }

    pub fn record(&self, job_type: &str, status: &str, duration_secs: f64) {
        let attributes = [
            KeyValue::new("job_type", job_type.to_string()),
            KeyValue::new("status", status.to_string()),
        ];

        self.executions_total.add(1, &attributes);
        self.duration.record(duration_secs, &attributes);
    }
}

pub struct ActiveConnectionsGauge {
    gauge: UpDownCounter<i64>,
}

impl ActiveConnectionsGauge {
    fn new() -> Self {
        let meter = global::meter(METER_NAME);

        let gauge = meter
            .i64_up_down_counter("active_connections")
            .with_description("Number of active connections")
            .with_unit("connections")
            .build();

        Self { gauge }
    }

    pub fn increment(&self, connection_type: &str) {
        self.gauge
            .add(1, &[KeyValue::new("type", connection_type.to_string())]);
    }

    pub fn decrement(&self, connection_type: &str) {
        self.gauge
            .add(-1, &[KeyValue::new("type", connection_type.to_string())]);
    }

    pub fn set(&self, connection_type: &str, delta: i64) {
        self.gauge
            .add(delta, &[KeyValue::new("type", connection_type.to_string())]);
    }
}

fn http_metrics() -> &'static HttpMetrics {
    HTTP_METRICS.get_or_init(HttpMetrics::new)
}

fn job_metrics() -> &'static JobMetrics {
    JOB_METRICS.get_or_init(JobMetrics::new)
}

fn connections_gauge() -> &'static ActiveConnectionsGauge {
    CONNECTIONS_GAUGE.get_or_init(ActiveConnectionsGauge::new)
}

pub fn record_http_request(method: &str, path: &str, status: u16, duration_secs: f64) {
    http_metrics().record(method, path, status, duration_secs);
}

pub fn record_job_execution(job_type: &str, status: &str, duration_secs: f64) {
    job_metrics().record(job_type, status, duration_secs);
}

pub fn set_active_connections(connection_type: &str, delta: i64) {
    connections_gauge().set(connection_type, delta);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_metrics_creation() {
        let _metrics = HttpMetrics::new();
    }

    #[test]
    fn test_job_metrics_creation() {
        let _metrics = JobMetrics::new();
    }

    #[test]
    fn test_connections_gauge_creation() {
        let _gauge = ActiveConnectionsGauge::new();
    }
}
