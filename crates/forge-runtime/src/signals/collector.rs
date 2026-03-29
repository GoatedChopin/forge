//! Buffered batch event writer for the signals pipeline.
//!
//! Events are sent via an mpsc channel and flushed in batches using
//! PostgreSQL UNNEST for high-throughput INSERT. Uses the analytics
//! connection pool to avoid contention with user queries.

use std::sync::Arc;
use std::time::Duration;

use forge_core::signals::SignalEvent;
use sqlx::PgPool;
use tokio::sync::mpsc;
use tracing::{debug, error, warn};

/// Channel capacity before events start getting dropped.
const CHANNEL_CAPACITY: usize = 10_000;

/// Buffered signal event collector.
///
/// Clone-friendly (shares the mpsc sender). Send events from any async
/// context via [`try_send`] which never blocks the caller.
#[derive(Clone)]
pub struct SignalsCollector {
    tx: mpsc::Sender<SignalEvent>,
}

impl SignalsCollector {
    /// Create a new collector and spawn the background flush task.
    ///
    /// Returns the collector handle. The flush task runs until the last
    /// sender is dropped (i.e., all collector clones are gone).
    pub fn spawn(pool: Arc<PgPool>, batch_size: usize, flush_interval: Duration) -> Self {
        let (tx, rx) = mpsc::channel(CHANNEL_CAPACITY);
        tokio::spawn(flush_loop(rx, pool, batch_size, flush_interval));
        Self { tx }
    }

    /// Send an event without blocking. Drops the event if the channel is full.
    pub fn try_send(&self, event: SignalEvent) {
        if let Err(mpsc::error::TrySendError::Full(_)) = self.tx.try_send(event) {
            warn!("signals collector channel full, dropping event");
        }
    }
}

/// Background loop that drains the channel and performs batch inserts.
async fn flush_loop(
    mut rx: mpsc::Receiver<SignalEvent>,
    pool: Arc<PgPool>,
    batch_size: usize,
    flush_interval: Duration,
) {
    let mut buffer: Vec<SignalEvent> = Vec::with_capacity(batch_size);
    let mut interval = tokio::time::interval(flush_interval);
    interval.tick().await;

    loop {
        tokio::select! {
            event = rx.recv() => {
                match event {
                    Some(e) => {
                        buffer.push(e);
                        if buffer.len() >= batch_size {
                            flush_batch(&pool, &mut buffer).await;
                        }
                    }
                    None => {
                        // Channel closed, flush remaining and exit
                        if !buffer.is_empty() {
                            flush_batch(&pool, &mut buffer).await;
                        }
                        debug!("signals collector shutting down");
                        return;
                    }
                }
            }
            _ = interval.tick() => {
                if !buffer.is_empty() {
                    flush_batch(&pool, &mut buffer).await;
                }
            }
        }
    }
}

/// Flush a batch of events into PostgreSQL using UNNEST for single-roundtrip INSERT.
/// Uses runtime sqlx::query() because UNNEST with typed arrays is not supported by
/// the compile-time sqlx::query!() macro.
async fn flush_batch(pool: &PgPool, buffer: &mut Vec<SignalEvent>) {
    let count = buffer.len();

    let mut ids = Vec::with_capacity(count);
    let mut event_types = Vec::with_capacity(count);
    let mut event_names: Vec<Option<String>> = Vec::with_capacity(count);
    let mut correlation_ids: Vec<Option<String>> = Vec::with_capacity(count);
    let mut session_ids: Vec<Option<uuid::Uuid>> = Vec::with_capacity(count);
    let mut visitor_ids: Vec<Option<String>> = Vec::with_capacity(count);
    let mut user_ids: Vec<Option<uuid::Uuid>> = Vec::with_capacity(count);
    let mut tenant_ids: Vec<Option<uuid::Uuid>> = Vec::with_capacity(count);
    let mut properties_list: Vec<serde_json::Value> = Vec::with_capacity(count);
    let mut page_urls: Vec<Option<String>> = Vec::with_capacity(count);
    let mut referrers: Vec<Option<String>> = Vec::with_capacity(count);
    let mut function_names: Vec<Option<String>> = Vec::with_capacity(count);
    let mut function_kinds: Vec<Option<String>> = Vec::with_capacity(count);
    let mut duration_ms_list: Vec<Option<i32>> = Vec::with_capacity(count);
    let mut statuses: Vec<Option<String>> = Vec::with_capacity(count);
    let mut error_messages: Vec<Option<String>> = Vec::with_capacity(count);
    let mut error_stacks: Vec<Option<String>> = Vec::with_capacity(count);
    let mut error_contexts: Vec<Option<serde_json::Value>> = Vec::with_capacity(count);
    let mut client_ips: Vec<Option<String>> = Vec::with_capacity(count);
    let mut user_agents: Vec<Option<String>> = Vec::with_capacity(count);
    let mut device_types: Vec<Option<String>> = Vec::with_capacity(count);
    let mut browsers: Vec<Option<String>> = Vec::with_capacity(count);
    let mut oses: Vec<Option<String>> = Vec::with_capacity(count);
    let mut utm_sources: Vec<Option<String>> = Vec::with_capacity(count);
    let mut utm_mediums: Vec<Option<String>> = Vec::with_capacity(count);
    let mut utm_campaigns: Vec<Option<String>> = Vec::with_capacity(count);
    let mut utm_terms: Vec<Option<String>> = Vec::with_capacity(count);
    let mut utm_contents: Vec<Option<String>> = Vec::with_capacity(count);
    let mut is_bots: Vec<bool> = Vec::with_capacity(count);
    let mut timestamps: Vec<chrono::DateTime<chrono::Utc>> = Vec::with_capacity(count);

    for event in buffer.drain(..) {
        ids.push(uuid::Uuid::new_v4());
        event_types.push(event.event_type.to_string());
        event_names.push(event.event_name);
        correlation_ids.push(event.correlation_id);
        session_ids.push(event.session_id);
        visitor_ids.push(event.visitor_id);
        user_ids.push(event.user_id);
        tenant_ids.push(event.tenant_id);
        properties_list.push(event.properties);
        page_urls.push(event.page_url);
        referrers.push(event.referrer);
        function_names.push(event.function_name);
        function_kinds.push(event.function_kind);
        duration_ms_list.push(event.duration_ms);
        statuses.push(event.status);
        error_messages.push(event.error_message);
        error_stacks.push(event.error_stack);
        error_contexts.push(event.error_context);
        client_ips.push(event.client_ip);
        user_agents.push(event.user_agent);
        device_types.push(event.device_type);
        browsers.push(event.browser);
        oses.push(event.os);
        let (src, med, camp, term, content) = match event.utm {
            Some(utm) => (utm.source, utm.medium, utm.campaign, utm.term, utm.content),
            None => (None, None, None, None, None),
        };
        utm_sources.push(src);
        utm_mediums.push(med);
        utm_campaigns.push(camp);
        utm_terms.push(term);
        utm_contents.push(content);
        is_bots.push(event.is_bot);
        timestamps.push(event.timestamp);
    }

    let result = sqlx::query(
        "INSERT INTO forge_signals_events (
            id, event_type, event_name, correlation_id,
            session_id, visitor_id, user_id, tenant_id,
            properties, page_url, referrer,
            function_name, function_kind, duration_ms, status,
            error_message, error_stack, error_context,
            client_ip, user_agent,
            device_type, browser, os,
            utm_source, utm_medium, utm_campaign, utm_term, utm_content,
            is_bot, timestamp
        )
        SELECT * FROM UNNEST(
            $1::uuid[], $2::varchar[], $3::varchar[], $4::varchar[],
            $5::uuid[], $6::varchar[], $7::uuid[], $8::uuid[],
            $9::jsonb[], $10::text[], $11::text[],
            $12::varchar[], $13::varchar[], $14::int[], $15::varchar[],
            $16::text[], $17::text[], $18::jsonb[],
            $19::text[], $20::text[],
            $21::varchar[], $22::varchar[], $23::varchar[],
            $24::varchar[], $25::varchar[], $26::varchar[], $27::varchar[], $28::varchar[],
            $29::bool[], $30::timestamptz[]
        )",
    )
    .bind(&ids)
    .bind(&event_types)
    .bind(&event_names)
    .bind(&correlation_ids)
    .bind(&session_ids)
    .bind(&visitor_ids)
    .bind(&user_ids)
    .bind(&tenant_ids)
    .bind(&properties_list)
    .bind(&page_urls)
    .bind(&referrers)
    .bind(&function_names)
    .bind(&function_kinds)
    .bind(&duration_ms_list)
    .bind(&statuses)
    .bind(&error_messages)
    .bind(&error_stacks)
    .bind(&error_contexts)
    .bind(&client_ips)
    .bind(&user_agents)
    .bind(&device_types)
    .bind(&browsers)
    .bind(&oses)
    .bind(&utm_sources)
    .bind(&utm_mediums)
    .bind(&utm_campaigns)
    .bind(&utm_terms)
    .bind(&utm_contents)
    .bind(&is_bots)
    .bind(&timestamps)
    .execute(pool)
    .await;

    match result {
        Ok(_) => debug!(count, "flushed signal events"),
        Err(e) => error!(count, error = %e, "failed to flush signal events"),
    }
}
