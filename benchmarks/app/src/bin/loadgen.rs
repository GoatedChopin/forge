#![allow(clippy::too_many_arguments, clippy::indexing_slicing)]

use std::env;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use reqwest::Client;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::task::JoinSet;
use uuid::Uuid;

type DynError = Box<dyn Error + Send + Sync>;
type DynResult<T> = Result<T, DynError>;

const DEFAULT_START_USERS: usize = 10;
const DEFAULT_RAMP_STEP: usize = 250;
const DEFAULT_COUNTER_COUNT: usize = 128;
const DEFAULT_SETUP_CONCURRENCY: usize = 64;
const DEFAULT_ACTION_INTERVAL_MS: u64 = 100;
const DEFAULT_LEVEL_HOLD_SECS: u64 = 60;
const DEFAULT_SETTLE_TIMEOUT_SECS: u64 = 20;
const DEFAULT_P90_LIMIT_MS: u32 = 2_000;
const DEFAULT_ERROR_THRESHOLD: f64 = 0.02;

#[derive(Clone, Debug)]
struct Options {
    forge_urls: Vec<String>,
    max_duration: Option<Duration>,
    start_users: usize,
    ramp_step: usize,
    counter_count: usize,
    setup_concurrency: usize,
    action_interval: Duration,
    level_hold: Duration,
    p90_limit_ms: u32,
    error_threshold: f64,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            forge_urls: vec![
                "http://127.0.0.1:9081".to_string(),
                "http://127.0.0.1:9082".to_string(),
                "http://127.0.0.1:9083".to_string(),
            ],
            max_duration: None,
            start_users: DEFAULT_START_USERS,
            ramp_step: DEFAULT_RAMP_STEP,
            counter_count: DEFAULT_COUNTER_COUNT,
            setup_concurrency: DEFAULT_SETUP_CONCURRENCY,
            action_interval: Duration::from_millis(DEFAULT_ACTION_INTERVAL_MS),
            level_hold: Duration::from_secs(DEFAULT_LEVEL_HOLD_SECS),
            p90_limit_ms: DEFAULT_P90_LIMIT_MS,
            error_threshold: DEFAULT_ERROR_THRESHOLD,
        }
    }
}

#[derive(Debug)]
struct ArgError(String);

impl Display for ArgError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for ArgError {}

#[derive(Default)]
struct Metrics {
    requests: AtomicU64,
    request_errors: AtomicU64,
    subscription_failures: AtomicU64,
    sse_disconnects: AtomicU64,
    subscribed_users: AtomicUsize,
    latencies_ms: Mutex<Vec<u32>>,
}

impl Metrics {
    fn record_latency(&self, latency_ms: u32) {
        self.latencies_ms
            .lock()
            .expect("latency buffer poisoned")
            .push(latency_ms);
    }

    fn take_latencies(&self) -> Vec<u32> {
        std::mem::take(&mut *self.latencies_ms.lock().expect("latency buffer poisoned"))
    }
}

#[derive(Clone, Debug)]
struct UserCredential {
    user_id: Uuid,
    token: String,
    home_url: String,
    home_counter_id: Uuid,
}

#[derive(Debug, Deserialize)]
struct RpcEnvelope<T> {
    success: bool,
    data: Option<T>,
}

#[derive(Debug, Deserialize)]
struct RegisterResponse {
    token: String,
    user_id: Uuid,
}

#[derive(Debug, Deserialize)]
struct CounterRecord {
    id: Uuid,
}

#[derive(Debug, Deserialize)]
struct ConnectedPayload {
    session_id: String,
    session_secret: String,
}

#[derive(Debug, Serialize)]
struct SubscribeRequest {
    session_id: String,
    session_secret: String,
    id: String,
    function: String,
    args: Value,
}

#[derive(Debug)]
struct SseConnection {
    session_id: String,
    session_secret: String,
    drain_handle: tokio::task::JoinHandle<()>,
}

#[tokio::main]
async fn main() -> DynResult<()> {
    let options = parse_args(env::args().skip(1))?;
    let run_id = format!(
        "bench-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    );

    let rpc_client = Arc::new(
        Client::builder()
            .pool_max_idle_per_host(512)
            .tcp_nodelay(true)
            .build()?,
    );
    let sse_client = Arc::new(
        Client::builder()
            .pool_max_idle_per_host(0)
            .tcp_nodelay(true)
            .build()?,
    );
    let metrics = Arc::new(Metrics::default());

    for url in &options.forge_urls {
        ensure_healthy(rpc_client.as_ref(), url).await?;
    }

    println!("Target URLs: {}", options.forge_urls.join(", "));
    println!("Ramp: +{} active SSE users", options.ramp_step);
    println!("Hold: {}s at each level", options.level_hold.as_secs());
    println!(
        "Stop: p90 > {}ms or err > {:.0}%\n",
        options.p90_limit_ms,
        options.error_threshold * 100.0
    );

    let counters = setup_counters(&options, rpc_client.as_ref(), &run_id).await?;
    let canary = setup_canary_user(&options, rpc_client.as_ref(), &run_id).await?;
    wait_for_counter_visibility(&options, rpc_client.as_ref(), &canary, &counters).await?;

    run_controller(options, run_id, counters, rpc_client, sse_client, metrics).await
}

async fn run_controller(
    options: Options,
    run_id: String,
    counters: Vec<Uuid>,
    rpc_client: Arc<Client>,
    sse_client: Arc<Client>,
    metrics: Arc<Metrics>,
) -> DynResult<()> {
    let started_at = Instant::now();
    let mut launched_users = 0usize;
    let mut next_user_index = 0usize;
    let mut prev_requests = 0u64;
    let mut prev_errors = 0u64;
    let mut prev_subscription_failures = 0u64;
    let mut prev_disconnects = 0u64;
    let mut prev_time = Instant::now();
    let mut peak_rps = 0.0f64;
    let mut peak_users = 0usize;
    let mut tasks = Vec::new();

    spawn_user_batch(
        options.start_users,
        &mut launched_users,
        &mut next_user_index,
        &run_id,
        &counters,
        rpc_client.clone(),
        sse_client.clone(),
        metrics.clone(),
        &mut tasks,
        &options,
    )
    .await?;

    wait_for_subscribed_users(metrics.as_ref(), launched_users, settle_timeout(&options)).await;
    println!("  starting at {} launched users\n", launched_users);

    loop {
        tokio::time::sleep(options.level_hold).await;

        let now = Instant::now();
        let dt = now.duration_since(prev_time).as_secs_f64();
        if dt <= 0.0 {
            continue;
        }

        let requests = metrics.requests.load(Ordering::Relaxed);
        let errors = metrics.request_errors.load(Ordering::Relaxed);
        let subscription_failures = metrics.subscription_failures.load(Ordering::Relaxed);
        let disconnects = metrics.sse_disconnects.load(Ordering::Relaxed);
        let subscribed = metrics.subscribed_users.load(Ordering::Relaxed);

        let delta_requests = requests.saturating_sub(prev_requests);
        let delta_errors = errors.saturating_sub(prev_errors);
        let delta_subscription_failures =
            subscription_failures.saturating_sub(prev_subscription_failures);
        let delta_disconnects = disconnects.saturating_sub(prev_disconnects);

        prev_requests = requests;
        prev_errors = errors;
        prev_subscription_failures = subscription_failures;
        prev_disconnects = disconnects;
        prev_time = now;

        let rps = delta_requests as f64 / dt;
        peak_rps = peak_rps.max(rps);
        peak_users = peak_users.max(subscribed);

        let p90_latency_ms = percentile_90(metrics.take_latencies());
        let total_error_events = delta_errors + delta_subscription_failures + delta_disconnects;
        let err_rate = if delta_requests == 0 {
            if total_error_events == 0 { 0.0 } else { 1.0 }
        } else {
            total_error_events as f64 / delta_requests as f64
        };

        println!(
            "  {:>5} launched | {:>5} active | {:>7.0} req/s | p90={:>6}ms | err={:.2}%",
            launched_users,
            subscribed,
            rps,
            p90_latency_ms,
            err_rate * 100.0
        );

        if p90_latency_ms > options.p90_limit_ms {
            println!("\n  stopped: p90={}ms", p90_latency_ms);
            println!("  launched users: {}", launched_users);
            println!("  concurrent users: {}", subscribed);
            println!("  peak concurrent users: {}", peak_users);
            println!("  peak throughput: {:.0} req/s", peak_rps);
            break;
        }

        if err_rate > options.error_threshold {
            println!("\n  stopped: err={:.2}%", err_rate * 100.0);
            println!("  launched users: {}", launched_users);
            println!("  concurrent users: {}", subscribed);
            println!("  peak concurrent users: {}", peak_users);
            println!("  peak throughput: {:.0} req/s", peak_rps);
            break;
        }

        if let Some(max_duration) = options.max_duration
            && started_at.elapsed() >= max_duration
        {
            println!("\n  stopped: max duration reached");
            println!("  launched users: {}", launched_users);
            println!("  concurrent users: {}", subscribed);
            println!("  peak concurrent users: {}", peak_users);
            println!("  peak throughput: {:.0} req/s", peak_rps);
            break;
        }

        if subscribed < launched_users {
            continue;
        }

        spawn_user_batch(
            options.ramp_step,
            &mut launched_users,
            &mut next_user_index,
            &run_id,
            &counters,
            rpc_client.clone(),
            sse_client.clone(),
            metrics.clone(),
            &mut tasks,
            &options,
        )
        .await?;

        wait_for_subscribed_users(metrics.as_ref(), launched_users, settle_timeout(&options)).await;
    }

    for task in tasks {
        task.abort();
    }

    Ok(())
}

async fn spawn_user_batch(
    count: usize,
    launched_users: &mut usize,
    next_user_index: &mut usize,
    run_id: &str,
    counters: &[Uuid],
    rpc_client: Arc<Client>,
    sse_client: Arc<Client>,
    metrics: Arc<Metrics>,
    tasks: &mut Vec<tokio::task::JoinHandle<()>>,
    options: &Options,
) -> DynResult<()> {
    let mut remaining = count;

    while remaining > 0 {
        let batch_size = remaining.min(options.setup_concurrency.max(1));
        let mut set = JoinSet::new();

        for _ in 0..batch_size {
            let index = *next_user_index;
            *next_user_index += 1;

            let rpc_client = rpc_client.clone();
            let home_url = options.forge_urls[index % options.forge_urls.len()].clone();
            let home_counter_id = counters[index % counters.len()];
            let name = format!("{run_id}-user-{index:05}");

            set.spawn(async move {
                register_user(&rpc_client, &home_url, &name)
                    .await
                    .map(|registered| {
                        (
                            index,
                            UserCredential {
                                user_id: registered.user_id,
                                token: registered.token,
                                home_url,
                                home_counter_id,
                            },
                        )
                    })
            });
        }

        while let Some(result) = set.join_next().await {
            let (user_index, credential) = result??;
            *launched_users += 1;
            tasks.push(tokio::spawn(run_user(
                user_index,
                rpc_client.clone(),
                sse_client.clone(),
                metrics.clone(),
                credential,
                options.forge_urls.clone(),
                options.action_interval,
            )));
        }

        remaining -= batch_size;
    }

    Ok(())
}

async fn run_user(
    user_index: usize,
    rpc_client: Arc<Client>,
    sse_client: Arc<Client>,
    metrics: Arc<Metrics>,
    credential: UserCredential,
    forge_urls: Vec<String>,
    action_interval: Duration,
) {
    let sse = match open_sse(sse_client.as_ref(), &credential.home_url, &credential.token).await {
        Ok(conn) => conn,
        Err(_) => {
            metrics
                .subscription_failures
                .fetch_add(1, Ordering::Relaxed);
            return;
        }
    };

    let subscribe_body = SubscribeRequest {
        session_id: sse.session_id.clone(),
        session_secret: sse.session_secret.clone(),
        id: format!("sub-{}", user_index),
        function: "get_counter".to_string(),
        args: json!({
            "user_id": credential.user_id,
            "id": credential.home_counter_id,
        }),
    };

    if post_json::<_, RpcEnvelope<Value>>(
        rpc_client.as_ref(),
        &format!("{}/_api/subscribe", credential.home_url),
        &subscribe_body,
        Some(&credential.token),
    )
    .await
    .map(|resp| resp.success)
    .unwrap_or(false)
    {
        metrics.subscribed_users.fetch_add(1, Ordering::Relaxed);
    } else {
        sse.drain_handle.abort();
        metrics
            .subscription_failures
            .fetch_add(1, Ordering::Relaxed);
        return;
    }

    let action_interval_ms = action_interval.as_millis() as u64;
    if action_interval_ms > 0 {
        let phase_offset_ms = user_index as u64 % action_interval_ms.max(1);
        if phase_offset_ms > 0 {
            tokio::time::sleep(Duration::from_millis(phase_offset_ms)).await;
        }
    }

    let mut iteration = 0usize;
    loop {
        if sse.drain_handle.is_finished() {
            let _ = sse.drain_handle.await;
            metrics.sse_disconnects.fetch_add(1, Ordering::Relaxed);
            break;
        }

        let action_url = &forge_urls[(iteration + user_index) % forge_urls.len()];
        let payload = match iteration % 10 {
            0..=3 => (
                "get_counter",
                json!({ "user_id": credential.user_id, "id": credential.home_counter_id }),
            ),
            4..=6 => ("list_counters", json!({ "user_id": credential.user_id })),
            _ => (
                "increment",
                json!({ "user_id": credential.user_id, "id": credential.home_counter_id }),
            ),
        };

        let started = Instant::now();
        let ok = rpc_call(
            rpc_client.as_ref(),
            action_url,
            payload.0,
            payload.1,
            Some(&credential.token),
        )
        .await
        .unwrap_or(false);

        metrics.requests.fetch_add(1, Ordering::Relaxed);
        metrics.record_latency(started.elapsed().as_millis() as u32);
        if !ok {
            metrics.request_errors.fetch_add(1, Ordering::Relaxed);
        }

        iteration += 1;
        tokio::time::sleep(action_interval).await;
    }

    metrics.subscribed_users.fetch_sub(1, Ordering::Relaxed);
}

async fn setup_counters(options: &Options, client: &Client, run_id: &str) -> DynResult<Vec<Uuid>> {
    let mut counters = vec![Uuid::nil(); options.counter_count];
    let mut next_index = 0usize;

    while next_index < options.counter_count {
        let mut set = JoinSet::new();
        let batch_end = (next_index + options.setup_concurrency).min(options.counter_count);

        for index in next_index..batch_end {
            let client = client.clone();
            let url = options.forge_urls[index % options.forge_urls.len()].clone();
            let name = format!("{run_id}-counter-{index:05}");
            set.spawn(async move {
                create_counter(&client, &url, &name)
                    .await
                    .map(|counter| (index, counter.id))
            });
        }

        while let Some(result) = set.join_next().await {
            let (index, counter_id) = result??;
            counters[index] = counter_id;
        }

        next_index = batch_end;
    }

    Ok(counters)
}

async fn setup_canary_user(
    options: &Options,
    client: &Client,
    run_id: &str,
) -> DynResult<UserCredential> {
    let home_url = options.forge_urls.first().cloned().ok_or_else(|| {
        Box::new(ArgError("at least one forge URL is required".into())) as DynError
    })?;
    let registered = register_user(client, &home_url, &format!("{run_id}-canary")).await?;

    Ok(UserCredential {
        user_id: registered.user_id,
        token: registered.token,
        home_url,
        home_counter_id: Uuid::nil(),
    })
}

async fn wait_for_counter_visibility(
    options: &Options,
    client: &Client,
    canary: &UserCredential,
    counters: &[Uuid],
) -> DynResult<()> {
    let deadline = Instant::now() + Duration::from_secs(20);

    for chunk in counters.chunks(options.setup_concurrency.max(1)) {
        let mut set = JoinSet::new();

        for &counter_id in chunk {
            for url in &options.forge_urls {
                let client = client.clone();
                let url = url.clone();
                let token = canary.token.clone();
                let user_id = canary.user_id;
                set.spawn(async move {
                    wait_for_counter_visible(&client, &url, &token, user_id, counter_id, deadline)
                        .await
                });
            }
        }

        while let Some(result) = set.join_next().await {
            result??;
        }
    }

    Ok(())
}

async fn wait_for_counter_visible(
    client: &Client,
    base_url: &str,
    token: &str,
    user_id: Uuid,
    counter_id: Uuid,
    deadline: Instant,
) -> DynResult<()> {
    loop {
        if rpc_call(
            client,
            base_url,
            "get_counter",
            json!({ "user_id": user_id, "id": counter_id }),
            Some(token),
        )
        .await
        .unwrap_or(false)
        {
            return Ok(());
        }

        if Instant::now() >= deadline {
            return Err(Box::new(ArgError(format!(
                "counter {counter_id} did not become visible on {base_url} before setup timeout"
            ))));
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn ensure_healthy(client: &Client, base_url: &str) -> DynResult<()> {
    let response = client.get(format!("{base_url}/_api/health")).send().await?;
    if response.status().is_success() {
        Ok(())
    } else {
        Err(Box::new(ArgError(format!(
            "health check failed for {base_url}: {}",
            response.status()
        ))))
    }
}

async fn register_user(client: &Client, base_url: &str, name: &str) -> DynResult<RegisterResponse> {
    let response =
        rpc_json::<RegisterResponse>(client, base_url, "register", json!({ "name": name }), None)
            .await?;

    response
        .data
        .ok_or_else(|| Box::new(ArgError("register returned no data".into())) as DynError)
}

async fn create_counter(client: &Client, base_url: &str, name: &str) -> DynResult<CounterRecord> {
    let response = rpc_json::<CounterRecord>(
        client,
        base_url,
        "create_counter",
        json!({ "name": name }),
        None,
    )
    .await?;

    response
        .data
        .ok_or_else(|| Box::new(ArgError("create_counter returned no data".into())) as DynError)
}

async fn rpc_call(
    client: &Client,
    base_url: &str,
    function: &str,
    args: Value,
    token: Option<&str>,
) -> DynResult<bool> {
    let response = rpc_json::<Value>(client, base_url, function, args, token).await?;
    Ok(response.success)
}

async fn rpc_json<T: DeserializeOwned>(
    client: &Client,
    base_url: &str,
    function: &str,
    args: Value,
    token: Option<&str>,
) -> DynResult<RpcEnvelope<T>> {
    post_json::<_, RpcEnvelope<T>>(
        client,
        &format!("{base_url}/_api/rpc/{function}"),
        &json!({ "args": args }),
        token,
    )
    .await
}

async fn post_json<B: Serialize, T: DeserializeOwned>(
    client: &Client,
    url: &str,
    body: &B,
    token: Option<&str>,
) -> DynResult<T> {
    let mut request = client.post(url).json(body);
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }

    let response = request.send().await?;
    let status = response.status();
    if !status.is_success() && status != reqwest::StatusCode::BAD_REQUEST {
        return Err(Box::new(ArgError(format!(
            "request to {url} failed with {status}"
        ))));
    }

    Ok(response.json().await?)
}

async fn open_sse(client: &Client, base_url: &str, token: &str) -> DynResult<SseConnection> {
    let mut response = client
        .get(format!("{base_url}/_api/events"))
        .query(&[("token", token)])
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(Box::new(ArgError(format!(
            "failed to open sse stream on {base_url}: {}",
            response.status()
        ))));
    }

    let mut buffer = String::new();
    let connected = loop {
        let chunk = response.chunk().await?.ok_or_else(|| {
            Box::new(ArgError("sse stream closed before connect".into())) as DynError
        })?;
        buffer.push_str(&String::from_utf8_lossy(&chunk).replace('\r', ""));
        if let Some(payload) = pop_connected_event(&mut buffer)? {
            break payload;
        }
    };

    let drain_handle = tokio::spawn(async move {
        let mut local_buffer = buffer;
        while let Ok(Some(chunk)) = response.chunk().await {
            local_buffer.push_str(&String::from_utf8_lossy(&chunk).replace('\r', ""));
            let _ = pop_events(&mut local_buffer);
        }
    });

    Ok(SseConnection {
        session_id: connected.session_id,
        session_secret: connected.session_secret,
        drain_handle,
    })
}

fn pop_connected_event(buffer: &mut String) -> DynResult<Option<ConnectedPayload>> {
    let mut events = pop_raw_events(buffer);
    for raw in events.drain(..) {
        let (event, data) = parse_sse_event(&raw);
        if event == "connected" {
            return Ok(Some(serde_json::from_str(&data)?));
        }
    }
    Ok(None)
}

fn pop_events(buffer: &mut String) -> Vec<String> {
    pop_raw_events(buffer)
        .into_iter()
        .map(|raw| parse_sse_event(&raw).0)
        .collect()
}

fn pop_raw_events(buffer: &mut String) -> Vec<String> {
    let mut events = Vec::new();
    while let Some(idx) = buffer.find("\n\n") {
        let raw = buffer[..idx].to_string();
        buffer.drain(..idx + 2);
        if !raw.trim().is_empty() {
            events.push(raw);
        }
    }
    events
}

fn parse_sse_event(raw: &str) -> (String, String) {
    let mut event = "message".to_string();
    let mut data_lines = Vec::new();

    for line in raw.lines() {
        if let Some(rest) = line.strip_prefix("event:") {
            event = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix("data:") {
            data_lines.push(rest.trim_start().to_string());
        }
    }

    (event, data_lines.join("\n"))
}

async fn wait_for_subscribed_users(
    metrics: &Metrics,
    target_users: usize,
    timeout: Duration,
) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if metrics.subscribed_users.load(Ordering::Relaxed) >= target_users {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    metrics.subscribed_users.load(Ordering::Relaxed) >= target_users
}

fn settle_timeout(options: &Options) -> Duration {
    let _ = options;
    Duration::from_secs(DEFAULT_SETTLE_TIMEOUT_SECS)
}

fn percentile_90(mut samples: Vec<u32>) -> u32 {
    if samples.is_empty() {
        return 0;
    }
    samples.sort_unstable();
    let idx = samples
        .len()
        .saturating_mul(90)
        .div_ceil(100)
        .saturating_sub(1);
    samples[idx]
}

fn parse_args(args: impl Iterator<Item = String>) -> DynResult<Options> {
    let mut options = Options::default();
    let mut explicit_urls = Vec::new();
    let mut args = args.peekable();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--max-duration" => {
                let value = args.next().ok_or_else(|| {
                    Box::new(ArgError("missing value for --max-duration".into())) as DynError
                })?;
                options.max_duration = Some(parse_duration(&value)?);
            }
            "--help" => {
                print_help();
                std::process::exit(0);
            }
            value if value.starts_with("http://") || value.starts_with("https://") => {
                explicit_urls.push(value.to_string());
            }
            _ => {
                return Err(Box::new(ArgError(format!(
                    "unknown argument: {arg}. Pass Forge URLs as bare arguments."
                ))));
            }
        }
    }

    if !explicit_urls.is_empty() {
        options.forge_urls = explicit_urls;
    }

    if options.forge_urls.is_empty() {
        return Err(Box::new(ArgError(
            "at least one Forge URL is required".into(),
        )));
    }

    Ok(options)
}

fn print_help() {
    println!("Usage:");
    println!("  loadgen [--max-duration 30m] [forge-url ...]");
    println!();
    println!("Examples:");
    println!("  loadgen");
    println!("  loadgen --max-duration 30m");
    println!("  loadgen http://127.0.0.1:9081 http://127.0.0.1:9082 http://127.0.0.1:9083");
    println!("  loadgen https://forge-bench.example.com");
}

fn parse_duration(value: &str) -> DynResult<Duration> {
    if let Some(raw) = value.strip_suffix('s') {
        return Ok(Duration::from_secs(parse_u64(raw, "duration")?));
    }
    if let Some(raw) = value.strip_suffix('m') {
        return Ok(Duration::from_secs(parse_u64(raw, "duration")? * 60));
    }
    if let Some(raw) = value.strip_suffix('h') {
        return Ok(Duration::from_secs(parse_u64(raw, "duration")? * 60 * 60));
    }
    Ok(Duration::from_secs(parse_u64(value, "duration")?))
}

fn parse_u64(value: &str, flag: &str) -> DynResult<u64> {
    value
        .parse::<u64>()
        .map_err(|_| Box::new(ArgError(format!("invalid value for {flag}: {value}"))) as DynError)
}
