
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use dioxus::prelude::{Signal, WritableExt, dioxus_core::Task};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::types::{
    ConnectionState, ForgeClientError, ForgeError, RpcEnvelopeRaw, SseEnvelopeRaw, StreamEvent,
};

type TokenProvider = Rc<dyn Fn() -> Option<String>>;
type AuthErrorHandler = Rc<dyn Fn(ForgeError)>;

static NEXT_SUBSCRIPTION_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
pub struct ForgeClientConfig {
    pub url: String,
    pub get_token: Option<TokenProvider>,
    pub on_auth_error: Option<AuthErrorHandler>,
    pub(crate) connection_state: Option<Signal<ConnectionState>>,
}

impl ForgeClientConfig {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            get_token: None,
            on_auth_error: None,
            connection_state: None,
        }
    }

    pub fn with_token_provider(mut self, provider: impl Fn() -> Option<String> + 'static) -> Self {
        self.get_token = Some(Rc::new(provider));
        self
    }

    pub fn with_auth_error_handler(
        mut self,
        handler: impl Fn(ForgeError) + 'static,
    ) -> Self {
        self.on_auth_error = Some(Rc::new(handler));
        self
    }

    pub(crate) fn with_connection_state(mut self, state: Signal<ConnectionState>) -> Self {
        self.connection_state = Some(state);
        self
    }
}

#[derive(Clone)]
pub struct ForgeClient {
    inner: Rc<ForgeClientInner>,
}

struct ForgeClientInner {
    url: String,
    get_token: Option<TokenProvider>,
    on_auth_error: Option<AuthErrorHandler>,
    connection_state: Option<Signal<ConnectionState>>,
}

impl ForgeClient {
    pub fn new(config: ForgeClientConfig) -> Self {
        Self {
            inner: Rc::new(ForgeClientInner {
                url: config.url.trim_end_matches('/').to_string(),
                get_token: config.get_token,
                on_auth_error: config.on_auth_error,
                connection_state: config.connection_state,
            }),
        }
    }

    pub async fn call<TArgs, TResult>(
        &self,
        function_name: &str,
        args: TArgs,
    ) -> Result<TResult, ForgeClientError>
    where
        TArgs: Serialize,
        TResult: DeserializeOwned,
    {
        let body = serde_json::json!({ "args": args });
        let envelope = platform::request_json(
            self,
            &format!("{}/_api/rpc/{}", self.inner.url, function_name),
            body,
        )
        .await?;
        self.decode_envelope(envelope)
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn call_multipart<TResult>(
        &self,
        function_name: &str,
        form: web_sys::FormData,
    ) -> Result<TResult, ForgeClientError>
    where
        TResult: DeserializeOwned,
    {
        let envelope = platform::request_multipart(
            self,
            &format!("{}/_api/rpc/{}/upload", self.inner.url, function_name),
            form,
        )
        .await?;
        self.decode_envelope(envelope)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub async fn call_multipart<TResult>(
        &self,
        function_name: &str,
        form: reqwest::multipart::Form,
    ) -> Result<TResult, ForgeClientError>
    where
        TResult: DeserializeOwned,
    {
        let envelope = platform::request_multipart(
            self,
            &format!("{}/_api/rpc/{}/upload", self.inner.url, function_name),
            form,
        )
        .await?;
        self.decode_envelope(envelope)
    }

    pub fn subscribe_query<TArgs, TResult, F>(
        &self,
        function_name: &str,
        args: TArgs,
        callback: F,
    ) -> SubscriptionHandle
    where
        TArgs: Serialize + Clone + 'static,
        TResult: DeserializeOwned + Clone + 'static,
        F: FnMut(StreamEvent<TResult>) + 'static,
    {
        platform::subscribe_query(self.clone(), function_name.to_string(), args, callback)
    }

    pub fn subscribe_job<TResult, F>(&self, job_id: String, callback: F) -> SubscriptionHandle
    where
        TResult: DeserializeOwned + Clone + 'static,
        F: FnMut(StreamEvent<TResult>) + 'static,
    {
        self.subscribe_tracker(
            "job",
            serde_json::json!({ "job_id": job_id }),
            "/_api/subscribe-job",
            callback,
        )
    }

    pub fn subscribe_workflow<TResult, F>(
        &self,
        workflow_id: String,
        callback: F,
    ) -> SubscriptionHandle
    where
        TResult: DeserializeOwned + Clone + 'static,
        F: FnMut(StreamEvent<TResult>) + 'static,
    {
        self.subscribe_tracker(
            "wf",
            serde_json::json!({ "workflow_id": workflow_id }),
            "/_api/subscribe-workflow",
            callback,
        )
    }

    fn subscribe_tracker<TResult, F>(
        &self,
        prefix: &str,
        payload: serde_json::Value,
        endpoint: &str,
        callback: F,
    ) -> SubscriptionHandle
    where
        TResult: DeserializeOwned + Clone + 'static,
        F: FnMut(StreamEvent<TResult>) + 'static,
    {
        platform::subscribe_tracker(
            self.clone(),
            prefix.to_string(),
            payload,
            endpoint.to_string(),
            callback,
        )
    }

    fn get_token(&self) -> Option<String> {
        self.inner
            .get_token
            .as_ref()
            .and_then(|provider| provider())
            .filter(|t| !t.is_empty())
    }

    fn emit_connection<TValue, T>(&self, callback: &Rc<RefCell<T>>, state: ConnectionState)
    where
        T: FnMut(StreamEvent<TValue>),
    {
        if let Some(mut signal) = self.inner.connection_state {
            signal.set(state);
        }
        (callback.borrow_mut())(StreamEvent::Connection(state));
    }

    fn emit_error<TValue, T>(&self, callback: &Rc<RefCell<T>>, error: ForgeClientError)
    where
        T: FnMut(StreamEvent<TValue>),
    {
        if error.code == "UNAUTHORIZED" {
            if let Some(handler) = &self.inner.on_auth_error {
                handler(error.as_forge_error());
            }
        }
        (callback.borrow_mut())(StreamEvent::Error(error));
    }

    fn decode_envelope<TResult>(
        &self,
        envelope: RpcEnvelopeRaw,
    ) -> Result<TResult, ForgeClientError>
    where
        TResult: DeserializeOwned,
    {
        if !envelope.success {
            let error = envelope.error.unwrap_or(ForgeError {
                code: "UNKNOWN".to_string(),
                message: "Unknown error".to_string(),
                details: None,
            });
            return Err(ForgeClientError::new(error.code, error.message, error.details));
        }

        let data = envelope.data.ok_or_else(|| {
            ForgeClientError::new("EMPTY_RESPONSE", "Server returned no data", None)
        })?;
        serde_json::from_value(data)
            .map_err(|err| ForgeClientError::new("DESERIALIZATION_ERROR", err.to_string(), None))
    }

    fn random_id(&self, prefix: &str) -> String {
        let id = NEXT_SUBSCRIPTION_ID.fetch_add(1, Ordering::Relaxed);
        format!("{prefix}-{id}")
    }
}

#[derive(Clone)]
pub struct SubscriptionHandle {
    closed: Rc<Cell<bool>>,
    task: Rc<RefCell<Option<Task>>>,
}

impl SubscriptionHandle {
    fn new() -> Self {
        Self {
            closed: Rc::new(Cell::new(false)),
            task: Rc::new(RefCell::new(None)),
        }
    }

    fn set_task(&self, task: Task) {
        *self.task.borrow_mut() = Some(task);
    }

    fn finish(&self) {
        self.closed.set(true);
        self.task.borrow_mut().take();
    }

    pub fn close(&self) {
        self.closed.set(true);
        if let Some(task) = self.task.borrow_mut().take() {
            task.cancel();
        }
    }

    pub fn is_closed(&self) -> bool {
        self.closed.get()
    }
}

impl Drop for SubscriptionHandle {
    fn drop(&mut self) {
        self.close();
    }
}

fn parse_json_str<T>(raw: &str) -> Result<T, ForgeClientError>
where
    T: DeserializeOwned,
{
    serde_json::from_str(raw)
        .map_err(|err| ForgeClientError::new("INVALID_SSE_PAYLOAD", err.to_string(), None))
}

fn emit_sse_error<TValue, T>(
    client: &ForgeClient,
    callback: &Rc<RefCell<T>>,
    envelope: SseEnvelopeRaw,
) where
    T: FnMut(StreamEvent<TValue>),
{
    client.emit_error(
        callback,
        ForgeClientError::new(
            envelope.code.unwrap_or_else(|| "SSE_ERROR".to_string()),
            envelope
                .message
                .unwrap_or_else(|| "Subscription error".to_string()),
            None,
        ),
    );
}

#[cfg(target_arch = "wasm32")]
mod platform {
    use std::cell::RefCell;
    use std::rc::Rc;

    use dioxus::prelude::spawn;
    use futures_util::{StreamExt, stream};
    use gloo_net::eventsource::futures::{EventSource, EventSourceSubscription};
    use gloo_net::http::Request;
    use js_sys::encode_uri_component;
    use serde::Serialize;
    use serde::de::DeserializeOwned;

    use super::{ForgeClient, SubscriptionHandle, emit_sse_error, parse_json_str};
    use crate::types::{
        ConnectedEvent, ConnectionState, ForgeClientError, RpcEnvelopeRaw, SseEnvelopeRaw,
        StreamEvent,
    };

    pub(super) async fn request_json(
        client: &ForgeClient,
        url: &str,
        body: serde_json::Value,
    ) -> Result<RpcEnvelopeRaw, ForgeClientError> {
        let mut request = Request::post(url).header("Content-Type", "application/json");
        if let Some(token) = client.get_token() {
            request = request.header("Authorization", &format!("Bearer {token}"));
        }

        let request = request.body(body.to_string()).map_err(request_error)?;
        request
            .send()
            .await
            .map_err(request_error)?
            .json()
            .await
            .map_err(request_error)
    }

    pub(super) async fn request_multipart(
        client: &ForgeClient,
        url: &str,
        form: web_sys::FormData,
    ) -> Result<RpcEnvelopeRaw, ForgeClientError> {
        let mut request = Request::post(url);
        if let Some(token) = client.get_token() {
            request = request.header("Authorization", &format!("Bearer {token}"));
        }

        let response = request.body(form).map_err(request_error)?;
        response
            .send()
            .await
            .map_err(request_error)?
            .json()
            .await
            .map_err(request_error)
    }

    struct SseConnection {
        event_source: EventSource,
        update_stream: EventSourceSubscription,
        error_stream: EventSourceSubscription,
    }

    async fn open_sse_connection<TValue, F>(
        client: &ForgeClient,
        callback: &Rc<RefCell<F>>,
        handle_task: &SubscriptionHandle,
    ) -> Option<(SseConnection, ConnectedEvent)>
    where
        F: FnMut(StreamEvent<TValue>),
    {
        let mut event_source = match EventSource::new(&events_url(client)) {
            Ok(source) => source,
            Err(err) => {
                client.emit_error(
                    callback,
                    ForgeClientError::new("SSE_CONNECTION_FAILED", err.to_string(), None),
                );
                client.emit_connection(callback, ConnectionState::Disconnected);
                handle_task.finish();
                return None;
            }
        };

        macro_rules! subscribe_or_bail {
            ($event_type:expr) => {
                match event_source.subscribe($event_type) {
                    Ok(stream) => stream,
                    Err(err) => {
                        client.emit_error(
                            callback,
                            ForgeClientError::new(
                                "SSE_SUBSCRIBE_FAILED",
                                err.to_string(),
                                None,
                            ),
                        );
                        client.emit_connection(callback, ConnectionState::Disconnected);
                        handle_task.finish();
                        return None;
                    }
                }
            };
        }

        let mut connected_stream = subscribe_or_bail!("connected");
        let update_stream = subscribe_or_bail!("update");
        let error_stream = subscribe_or_bail!("error");

        let connected_event = match connected_stream.next().await {
            Some(Ok((_kind, message))) => {
                let Some(raw) = message.data().as_string() else {
                    client.emit_error(
                        callback,
                        ForgeClientError::new(
                            "INVALID_SSE_PAYLOAD",
                            "SSE payload was not a string",
                            None,
                        ),
                    );
                    client.emit_connection(callback, ConnectionState::Disconnected);
                    handle_task.finish();
                    return None;
                };
                match parse_json_str::<ConnectedEvent>(&raw) {
                    Ok(event) => event,
                    Err(err) => {
                        client.emit_error(callback, err);
                        client.emit_connection(callback, ConnectionState::Disconnected);
                        handle_task.finish();
                        return None;
                    }
                }
            }
            Some(Err(err)) => {
                client.emit_error(
                    callback,
                    ForgeClientError::new("SSE_CONNECTION_FAILED", err.to_string(), None),
                );
                client.emit_connection(callback, ConnectionState::Disconnected);
                handle_task.finish();
                return None;
            }
            None => {
                client.emit_connection(callback, ConnectionState::Disconnected);
                handle_task.finish();
                return None;
            }
        };

        if handle_task.is_closed() {
            event_source.close();
            handle_task.finish();
            return None;
        }

        Some((SseConnection { event_source, update_stream, error_stream }, connected_event))
    }

    async fn process_sse_events<TResult, F>(
        update_stream: EventSourceSubscription,
        error_stream: EventSourceSubscription,
        client: &ForgeClient,
        callback: &Rc<RefCell<F>>,
        handle_task: &SubscriptionHandle,
    ) where
        TResult: DeserializeOwned + 'static,
        F: FnMut(StreamEvent<TResult>),
    {
        let mut events = stream::select(update_stream, error_stream);
        while !handle_task.is_closed() {
            let Some(event) = events.next().await else {
                break;
            };

            match event {
                Ok((kind, message)) if kind == "update" => {
                    let Some(raw) = message.data().as_string() else {
                        client.emit_error(
                            callback,
                            ForgeClientError::new(
                                "INVALID_SSE_PAYLOAD",
                                "SSE payload was not a string",
                                None,
                            ),
                        );
                        continue;
                    };
                    let envelope = match parse_json_str::<SseEnvelopeRaw>(&raw) {
                        Ok(value) => value,
                        Err(err) => {
                            client.emit_error(callback, err);
                            continue;
                        }
                    };
                    if let Some(data) = envelope.payload {
                        let parsed = match serde_json::from_value::<TResult>(data) {
                            Ok(value) => value,
                            Err(err) => {
                                client.emit_error(
                                    callback,
                                    ForgeClientError::new(
                                        "INVALID_SSE_PAYLOAD",
                                        err.to_string(),
                                        None,
                                    ),
                                );
                                continue;
                            }
                        };
                        (callback.borrow_mut())(StreamEvent::Data(parsed));
                    }
                }
                Ok((_kind, message)) => {
                    let Some(raw) = message.data().as_string() else {
                        client.emit_error(
                            callback,
                            ForgeClientError::new(
                                "INVALID_SSE_PAYLOAD",
                                "SSE payload was not a string",
                                None,
                            ),
                        );
                        continue;
                    };
                    let envelope = match parse_json_str::<SseEnvelopeRaw>(&raw) {
                        Ok(value) => value,
                        Err(err) => {
                            client.emit_error(callback, err);
                            continue;
                        }
                    };
                    emit_sse_error(client, callback, envelope);
                }
                Err(err) => {
                    client.emit_error(
                        callback,
                        ForgeClientError::new("SSE_CONNECTION_FAILED", err.to_string(), None),
                    );
                    break;
                }
            }
        }
    }

    pub(super) fn subscribe_query<TArgs, TResult, F>(
        client: ForgeClient,
        function_name: String,
        args: TArgs,
        callback: F,
    ) -> SubscriptionHandle
    where
        TArgs: Serialize + Clone + 'static,
        TResult: DeserializeOwned + Clone + 'static,
        F: FnMut(StreamEvent<TResult>) + 'static,
    {
        let handle = SubscriptionHandle::new();
        let handle_task = handle.clone();
        let callback = Rc::new(RefCell::new(callback));

        let task = spawn(async move {
            client.emit_connection(&callback, ConnectionState::Connecting);

            let args_value = match serde_json::to_value(args) {
                Ok(value) => value,
                Err(err) => {
                    client.emit_error(
                        &callback,
                        ForgeClientError::new("SERIALIZATION_ERROR", err.to_string(), None),
                    );
                    client.emit_connection(&callback, ConnectionState::Disconnected);
                    handle_task.finish();
                    return;
                }
            };

            let Some((sse, connected)) =
                open_sse_connection(&client, &callback, &handle_task).await
            else {
                return;
            };

            let register_payload = serde_json::json!({
                "session_id": connected.session_id,
                "session_secret": connected.session_secret,
                "id": client.random_id("sub"),
                "function": function_name,
                "args": args_value,
            });

            match request_json(
                &client,
                &format!("{}/_api/subscribe", client.inner.url),
                register_payload,
            )
            .await
            {
                Ok(envelope) => match client.decode_envelope::<TResult>(envelope) {
                    Ok(data) => {
                        client.emit_connection(&callback, ConnectionState::Connected);
                        (callback.borrow_mut())(StreamEvent::Data(data));
                    }
                    Err(err) => {
                        client.emit_error(&callback, err);
                        client.emit_connection(&callback, ConnectionState::Disconnected);
                        handle_task.finish();
                        return;
                    }
                },
                Err(err) => {
                    client.emit_error(&callback, err);
                    client.emit_connection(&callback, ConnectionState::Disconnected);
                    handle_task.finish();
                    return;
                }
            }

            process_sse_events::<TResult, _>(
                sse.update_stream,
                sse.error_stream,
                &client,
                &callback,
                &handle_task,
            )
            .await;

            sse.event_source.close();
            client.emit_connection(&callback, ConnectionState::Disconnected);
            handle_task.finish();
        });

        handle.set_task(task);
        handle
    }

    pub(super) fn subscribe_tracker<TResult, F>(
        client: ForgeClient,
        prefix: String,
        payload: serde_json::Value,
        endpoint: String,
        callback: F,
    ) -> SubscriptionHandle
    where
        TResult: DeserializeOwned + Clone + 'static,
        F: FnMut(StreamEvent<TResult>) + 'static,
    {
        let handle = SubscriptionHandle::new();
        let handle_task = handle.clone();
        let callback = Rc::new(RefCell::new(callback));

        let task = spawn(async move {
            client.emit_connection(&callback, ConnectionState::Connecting);

            let Some((sse, connected)) =
                open_sse_connection(&client, &callback, &handle_task).await
            else {
                return;
            };

            let mut register_payload = payload;
            let register_object = register_payload
                .as_object_mut()
                .expect("tracker payload must be an object");
            register_object.insert(
                "session_id".to_string(),
                serde_json::Value::String(connected.session_id.unwrap_or_default()),
            );
            register_object.insert(
                "session_secret".to_string(),
                serde_json::Value::String(connected.session_secret.unwrap_or_default()),
            );
            register_object.insert(
                "id".to_string(),
                serde_json::Value::String(client.random_id(&prefix)),
            );

            match request_json(
                &client,
                &format!("{}{}", client.inner.url, endpoint),
                register_payload,
            )
            .await
            {
                Ok(envelope) => {
                    client.emit_connection(&callback, ConnectionState::Connected);
                    if envelope.success {
                        if let Some(data) = envelope.data {
                            if let Ok(parsed) = serde_json::from_value::<TResult>(data) {
                                (callback.borrow_mut())(StreamEvent::Data(parsed));
                            }
                        }
                    }
                }
                Err(err) => {
                    client.emit_error(&callback, err);
                    client.emit_connection(&callback, ConnectionState::Disconnected);
                    handle_task.finish();
                    return;
                }
            }

            process_sse_events::<TResult, _>(
                sse.update_stream,
                sse.error_stream,
                &client,
                &callback,
                &handle_task,
            )
            .await;

            sse.event_source.close();
            client.emit_connection(&callback, ConnectionState::Disconnected);
            handle_task.finish();
        });

        handle.set_task(task);
        handle
    }

    fn events_url(client: &ForgeClient) -> String {
        match client.get_token() {
            Some(token) => format!(
                "{}/_api/events?token={}",
                client.inner.url,
                encode_uri_component(&token)
            ),
            None => format!("{}/_api/events", client.inner.url),
        }
    }

    fn request_error(err: gloo_net::Error) -> ForgeClientError {
        ForgeClientError::new("REQUEST_FAILED", err.to_string(), None)
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod platform {
    use std::cell::RefCell;
    use std::rc::Rc;

    use dioxus::prelude::spawn;
    use futures_util::StreamExt;
    use reqwest::Client;
    use reqwest_eventsource::{Event, EventSource};
    use serde::Serialize;
    use serde::de::DeserializeOwned;

    use super::{ForgeClient, SubscriptionHandle, emit_sse_error, parse_json_str};
    use crate::types::{
        ConnectedEvent, ConnectionState, ForgeClientError, RpcEnvelopeRaw, SseEnvelopeRaw,
        StreamEvent,
    };

    pub(super) async fn request_json(
        client: &ForgeClient,
        url: &str,
        body: serde_json::Value,
    ) -> Result<RpcEnvelopeRaw, ForgeClientError> {
        let mut request = Client::new().post(url).json(&body);
        if let Some(token) = client.get_token() {
            request = request.bearer_auth(token);
        }

        request
            .send()
            .await
            .map_err(request_error)?
            .json()
            .await
            .map_err(request_error)
    }

    pub(super) async fn request_multipart(
        client: &ForgeClient,
        url: &str,
        form: reqwest::multipart::Form,
    ) -> Result<RpcEnvelopeRaw, ForgeClientError> {
        let mut request = Client::new().post(url).multipart(form);
        if let Some(token) = client.get_token() {
            request = request.bearer_auth(token);
        }

        request
            .send()
            .await
            .map_err(request_error)?
            .json()
            .await
            .map_err(request_error)
    }

    async fn process_sse_events<TResult, F>(
        event_source: &mut EventSource,
        client: &ForgeClient,
        callback: &Rc<RefCell<F>>,
        handle_task: &SubscriptionHandle,
    ) where
        TResult: DeserializeOwned + 'static,
        F: FnMut(StreamEvent<TResult>),
    {
        while !handle_task.is_closed() {
            let Some(event) = event_source.next().await else {
                break;
            };

            match event {
                Ok(Event::Open) => {}
                Ok(Event::Message(message)) if message.event == "update" => {
                    let envelope = match parse_json_str::<SseEnvelopeRaw>(&message.data) {
                        Ok(value) => value,
                        Err(err) => {
                            client.emit_error(callback, err);
                            continue;
                        }
                    };
                    if let Some(data) = envelope.payload {
                        let parsed = match serde_json::from_value::<TResult>(data) {
                            Ok(value) => value,
                            Err(err) => {
                                client.emit_error(
                                    callback,
                                    ForgeClientError::new(
                                        "INVALID_SSE_PAYLOAD",
                                        err.to_string(),
                                        None,
                                    ),
                                );
                                continue;
                            }
                        };
                        (callback.borrow_mut())(StreamEvent::Data(parsed));
                    }
                }
                Ok(Event::Message(message)) if message.event == "error" => {
                    let envelope = match parse_json_str::<SseEnvelopeRaw>(&message.data) {
                        Ok(value) => value,
                        Err(err) => {
                            client.emit_error(callback, err);
                            continue;
                        }
                    };
                    emit_sse_error(client, callback, envelope);
                }
                Ok(Event::Message(_)) => {}
                Err(err) => {
                    client.emit_error(
                        callback,
                        ForgeClientError::new("SSE_CONNECTION_FAILED", err.to_string(), None),
                    );
                    break;
                }
            }
        }
    }

    async fn open_and_connect<TValue, F>(
        client: &ForgeClient,
        callback: &Rc<RefCell<F>>,
        handle_task: &SubscriptionHandle,
    ) -> Option<(EventSource, ConnectedEvent)>
    where
        F: FnMut(StreamEvent<TValue>),
    {
        let mut event_source = match open_event_source(client) {
            Ok(source) => source,
            Err(err) => {
                client.emit_error(callback, err);
                client.emit_connection(callback, ConnectionState::Disconnected);
                handle_task.finish();
                return None;
            }
        };

        let connected_event =
            match next_connected_event(&mut event_source, client, callback).await {
                Ok(Some(event)) => event,
                Ok(None) => {
                    client.emit_connection(callback, ConnectionState::Disconnected);
                    handle_task.finish();
                    return None;
                }
                Err(err) => {
                    client.emit_error(callback, err);
                    client.emit_connection(callback, ConnectionState::Disconnected);
                    handle_task.finish();
                    return None;
                }
            };

        if handle_task.is_closed() {
            event_source.close();
            handle_task.finish();
            return None;
        }

        Some((event_source, connected_event))
    }

    pub(super) fn subscribe_query<TArgs, TResult, F>(
        client: ForgeClient,
        function_name: String,
        args: TArgs,
        callback: F,
    ) -> SubscriptionHandle
    where
        TArgs: Serialize + Clone + 'static,
        TResult: DeserializeOwned + Clone + 'static,
        F: FnMut(StreamEvent<TResult>) + 'static,
    {
        let handle = SubscriptionHandle::new();
        let handle_task = handle.clone();
        let callback = Rc::new(RefCell::new(callback));

        let task = spawn(async move {
            client.emit_connection(&callback, ConnectionState::Connecting);

            let args_value = match serde_json::to_value(args) {
                Ok(value) => value,
                Err(err) => {
                    client.emit_error(
                        &callback,
                        ForgeClientError::new("SERIALIZATION_ERROR", err.to_string(), None),
                    );
                    client.emit_connection(&callback, ConnectionState::Disconnected);
                    handle_task.finish();
                    return;
                }
            };

            let Some((mut event_source, connected)) =
                open_and_connect(&client, &callback, &handle_task).await
            else {
                return;
            };

            let register_payload = serde_json::json!({
                "session_id": connected.session_id,
                "session_secret": connected.session_secret,
                "id": client.random_id("sub"),
                "function": function_name,
                "args": args_value,
            });

            match request_json(
                &client,
                &format!("{}/_api/subscribe", client.inner.url),
                register_payload,
            )
            .await
            {
                Ok(envelope) => match client.decode_envelope::<TResult>(envelope) {
                    Ok(data) => {
                        client.emit_connection(&callback, ConnectionState::Connected);
                        (callback.borrow_mut())(StreamEvent::Data(data));
                    }
                    Err(err) => {
                        client.emit_error(&callback, err);
                        client.emit_connection(&callback, ConnectionState::Disconnected);
                        handle_task.finish();
                        return;
                    }
                },
                Err(err) => {
                    client.emit_error(&callback, err);
                    client.emit_connection(&callback, ConnectionState::Disconnected);
                    handle_task.finish();
                    return;
                }
            }

            process_sse_events::<TResult, _>(
                &mut event_source,
                &client,
                &callback,
                &handle_task,
            )
            .await;

            event_source.close();
            client.emit_connection(&callback, ConnectionState::Disconnected);
            handle_task.finish();
        });

        handle.set_task(task);
        handle
    }

    pub(super) fn subscribe_tracker<TResult, F>(
        client: ForgeClient,
        prefix: String,
        payload: serde_json::Value,
        endpoint: String,
        callback: F,
    ) -> SubscriptionHandle
    where
        TResult: DeserializeOwned + Clone + 'static,
        F: FnMut(StreamEvent<TResult>) + 'static,
    {
        let handle = SubscriptionHandle::new();
        let handle_task = handle.clone();
        let callback = Rc::new(RefCell::new(callback));

        let task = spawn(async move {
            client.emit_connection(&callback, ConnectionState::Connecting);

            let Some((mut event_source, connected)) =
                open_and_connect(&client, &callback, &handle_task).await
            else {
                return;
            };

            let mut register_payload = payload;
            let register_object = register_payload
                .as_object_mut()
                .expect("tracker payload must be an object");
            register_object.insert(
                "session_id".to_string(),
                serde_json::Value::String(connected.session_id.unwrap_or_default()),
            );
            register_object.insert(
                "session_secret".to_string(),
                serde_json::Value::String(connected.session_secret.unwrap_or_default()),
            );
            register_object.insert(
                "id".to_string(),
                serde_json::Value::String(client.random_id(&prefix)),
            );

            match request_json(
                &client,
                &format!("{}{}", client.inner.url, endpoint),
                register_payload,
            )
            .await
            {
                Ok(envelope) => {
                    client.emit_connection(&callback, ConnectionState::Connected);
                    if envelope.success {
                        if let Some(data) = envelope.data {
                            if let Ok(parsed) = serde_json::from_value::<TResult>(data) {
                                (callback.borrow_mut())(StreamEvent::Data(parsed));
                            }
                        }
                    }
                }
                Err(err) => {
                    client.emit_error(&callback, err);
                    client.emit_connection(&callback, ConnectionState::Disconnected);
                    handle_task.finish();
                    return;
                }
            }

            process_sse_events::<TResult, _>(
                &mut event_source,
                &client,
                &callback,
                &handle_task,
            )
            .await;

            event_source.close();
            client.emit_connection(&callback, ConnectionState::Disconnected);
            handle_task.finish();
        });

        handle.set_task(task);
        handle
    }

    fn open_event_source(client: &ForgeClient) -> Result<EventSource, ForgeClientError> {
        let mut request = Client::new().get(format!("{}/_api/events", client.inner.url));
        if let Some(token) = client.get_token() {
            request = request.bearer_auth(token);
        }

        EventSource::new(request)
            .map_err(|err| ForgeClientError::new("SSE_CONNECTION_FAILED", err.to_string(), None))
    }

    async fn next_connected_event<TValue, T>(
        event_source: &mut EventSource,
        client: &ForgeClient,
        callback: &Rc<RefCell<T>>,
    ) -> Result<Option<ConnectedEvent>, ForgeClientError>
    where
        T: FnMut(StreamEvent<TValue>),
    {
        while let Some(event) = event_source.next().await {
            match event {
                Ok(Event::Open) => continue,
                Ok(Event::Message(message)) if message.event == "connected" => {
                    return parse_json_str::<ConnectedEvent>(&message.data).map(Some);
                }
                Ok(Event::Message(message)) if message.event == "error" => {
                    let envelope = parse_json_str::<SseEnvelopeRaw>(&message.data)?;
                    emit_sse_error(client, callback, envelope);
                }
                Ok(Event::Message(_)) => {}
                Err(err) => {
                    return Err(ForgeClientError::new(
                        "SSE_CONNECTION_FAILED",
                        err.to_string(),
                        None,
                    ));
                }
            }
        }

        Ok(None)
    }

    fn request_error(err: reqwest::Error) -> ForgeClientError {
        ForgeClientError::new("REQUEST_FAILED", err.to_string(), None)
    }
}
