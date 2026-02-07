#![allow(dead_code)]

use opentelemetry::{
    KeyValue, global,
    metrics::{Counter, Gauge, Histogram},
};
use std::sync::OnceLock;

const METER_NAME: &str = "forge.cluster";

static CLUSTER_METRICS: OnceLock<ClusterMetrics> = OnceLock::new();

pub struct ClusterMetrics {
    // Heartbeat metrics
    heartbeat_latency: Histogram<f64>,
    nodes_active: Gauge<i64>,
    nodes_dead: Gauge<i64>,

    // Leader election metrics
    leader_election_attempts: Counter<u64>,
    is_leader: Gauge<i64>,

    // Reactor metrics
    notifications_processed: Counter<u64>,
    notification_latency: Histogram<f64>,
}

impl ClusterMetrics {
    fn new() -> Self {
        let meter = global::meter(METER_NAME);

        let heartbeat_latency = meter
            .f64_histogram("cluster.heartbeat.latency")
            .with_description("Heartbeat round-trip latency in seconds")
            .with_unit("s")
            .build();

        let nodes_active = meter
            .i64_gauge("cluster.nodes.active")
            .with_description("Number of active nodes in the cluster")
            .build();

        let nodes_dead = meter
            .i64_gauge("cluster.nodes.dead")
            .with_description("Number of dead nodes in the cluster")
            .build();

        let leader_election_attempts = meter
            .u64_counter("cluster.leader.election_attempts")
            .with_description("Total leader election attempts")
            .build();

        let is_leader = meter
            .i64_gauge("cluster.leader.is_leader")
            .with_description("Whether this node is the leader (1) or not (0)")
            .build();

        let notifications_processed = meter
            .u64_counter("cluster.reactor.notifications_processed")
            .with_description("Total change notifications processed")
            .build();

        let notification_latency = meter
            .f64_histogram("cluster.reactor.notification_latency")
            .with_description("Change notification processing latency in seconds")
            .with_unit("s")
            .build();

        Self {
            heartbeat_latency,
            nodes_active,
            nodes_dead,
            leader_election_attempts,
            is_leader,
            notifications_processed,
            notification_latency,
        }
    }
}

fn metrics() -> &'static ClusterMetrics {
    CLUSTER_METRICS.get_or_init(ClusterMetrics::new)
}

pub fn record_heartbeat_latency(latency_secs: f64) {
    metrics().heartbeat_latency.record(latency_secs, &[]);
}

pub fn set_node_counts(active: i64, dead: i64) {
    metrics().nodes_active.record(active, &[]);
    metrics().nodes_dead.record(dead, &[]);
}

pub fn record_leader_election_attempt(role: &str, acquired: bool) {
    metrics().leader_election_attempts.add(
        1,
        &[
            KeyValue::new("role", role.to_string()),
            KeyValue::new("acquired", acquired),
        ],
    );
}

pub fn set_is_leader(role: &str, is_leader: bool) {
    metrics().is_leader.record(
        if is_leader { 1 } else { 0 },
        &[KeyValue::new("role", role.to_string())],
    );
}

pub fn record_notification_processed(table: &str) {
    metrics()
        .notifications_processed
        .add(1, &[KeyValue::new("table", table.to_string())]);
}

pub fn record_notification_latency(latency_secs: f64) {
    metrics().notification_latency.record(latency_secs, &[]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cluster_metrics_creation() {
        let _metrics = ClusterMetrics::new();
    }
}
