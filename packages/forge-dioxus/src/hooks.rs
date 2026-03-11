
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use dioxus::dioxus_core::use_drop;
use dioxus::prelude::*;
use futures_timer::Delay;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::{
    ConnectionState, JobExecutionState, QueryState, StreamEvent, SubscriptionHandle, SubscriptionState,
    WorkflowExecutionState, use_forge_client,
};

#[derive(Debug, Clone, serde::Deserialize)]
struct JobStartResponse {
    job_id: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct WorkflowStartResponse {
    workflow_id: String,
}

pub fn use_forge_query<TArgs, TResult>(
    function_name: &'static str,
    args: TArgs,
) -> Signal<QueryState<TResult>>
where
    TArgs: Serialize + Clone + PartialEq + 'static,
    TResult: DeserializeOwned + Clone + 'static,
{
    let client = use_forge_client();
    let state = use_signal(QueryState::<TResult>::default);
    let request_id = use_hook(|| Rc::new(Cell::new(0_u64)));

    use_effect(use_reactive!(|(args,)| {
        let client = client.clone();
        let mut state = state;
        let request_id = request_id.clone();
        let current_id = request_id.get() + 1;
        request_id.set(current_id);

        state.set(QueryState {
            loading: true,
            data: None,
            error: None,
        });

        spawn(async move {
            match client.call::<_, TResult>(function_name, args).await {
                Ok(data) if request_id.get() == current_id => {
                    state.set(QueryState {
                        loading: false,
                        data: Some(data),
                        error: None,
                    });
                }
                Err(err) if request_id.get() == current_id => {
                    state.set(QueryState {
                        loading: false,
                        data: None,
                        error: Some(err.as_forge_error()),
                    });
                }
                _ => {}
            }
        });
    }));

    state
}

pub fn use_forge_subscription<TArgs, TResult>(
    function_name: &'static str,
    args: TArgs,
) -> Signal<SubscriptionState<TResult>>
where
    TArgs: Serialize + Clone + PartialEq + 'static,
    TResult: DeserializeOwned + Clone + 'static,
{
    let client = use_forge_client();
    let state = use_signal(SubscriptionState::<TResult>::default);
    let handle = use_hook(|| Rc::new(RefCell::new(None::<SubscriptionHandle>)));
    let generation = use_hook(|| Rc::new(Cell::new(0_u64)));
    let reconnect_nonce = use_signal(|| 0_u64);
    let effect_handle = handle.clone();
    let reconnect_key = reconnect_nonce();

    use_effect(use_reactive!(|(args, reconnect_key)| {
        let _ = reconnect_key;
        let current_generation = generation.get() + 1;
        generation.set(current_generation);

        if let Some(existing) = effect_handle.borrow_mut().take() {
            existing.close();
        }

        let mut state = state;
        state.set(SubscriptionState::default());
        let reconnect_generation = generation.clone();

        let subscription = client.subscribe_query(function_name, args, move |event| match event {
            StreamEvent::Connection(connection_state) => {
                let mut next = state();
                next.connection_state = connection_state;
                next.stale = connection_state != ConnectionState::Connected;
                state.set(next);

                if connection_state == ConnectionState::Disconnected
                    && reconnect_generation.get() == current_generation
                {
                    let mut reconnect_nonce = reconnect_nonce;
                    spawn(async move {
                        Delay::new(Duration::from_millis(350)).await;
                        reconnect_nonce += 1;
                    });
                }
            }
            StreamEvent::Data(data) => {
                state.set(SubscriptionState {
                    loading: false,
                    data: Some(data),
                    error: None,
                    stale: false,
                    connection_state: state().connection_state,
                });
            }
            StreamEvent::Error(err) => {
                let mut next = state();
                next.loading = false;
                next.error = Some(err.as_forge_error());
                next.stale = true;
                state.set(next);
            }
        });

        *effect_handle.borrow_mut() = Some(subscription);
    }));

    use_drop({
        let handle = handle.clone();
        move || {
            if let Some(existing) = handle.borrow_mut().take() {
                existing.close();
            }
        }
    });

    state
}

pub fn use_forge_job<TArgs, TResult>(
    function_name: &'static str,
    args: TArgs,
) -> Signal<JobExecutionState<TResult>>
where
    TArgs: Serialize + Clone + PartialEq + 'static,
    TResult: DeserializeOwned + Clone + 'static,
{
    let client = use_forge_client();
    let state = use_signal(JobExecutionState::<TResult>::default);
    let handle = use_hook(|| Rc::new(RefCell::new(None::<SubscriptionHandle>)));
    let effect_handle = handle.clone();

    use_effect(use_reactive!(|(args,)| {
        if let Some(existing) = effect_handle.borrow_mut().take() {
            existing.close();
        }

        let client = client.clone();
        let handle = effect_handle.clone();
        let mut state = state;
        state.set(JobExecutionState::default());

        spawn(async move {
            match client.call::<_, JobStartResponse>(function_name, args).await {
                Ok(started) => {
                    let subscription = client.subscribe_job(started.job_id.clone(), move |event| {
                        match event {
                            StreamEvent::Connection(connection_state) => {
                                let mut next = state();
                                next.connection_state = connection_state;
                                state.set(next);
                            }
                            StreamEvent::Data(job_state) => {
                                state.set(JobExecutionState {
                                    loading: false,
                                    connection_state: state().connection_state,
                                    state: job_state,
                                });
                            }
                            StreamEvent::Error(err) => {
                                let mut next = state();
                                next.loading = false;
                                next.state.error = Some(err.message);
                                state.set(next);
                            }
                        }
                    });
                    *handle.borrow_mut() = Some(subscription);
                }
                Err(err) => {
                    let mut next = state();
                    next.loading = false;
                    next.state.error = Some(err.message);
                    state.set(next);
                }
            }
        });
    }));

    use_drop({
        let handle = handle.clone();
        move || {
            if let Some(existing) = handle.borrow_mut().take() {
                existing.close();
            }
        }
    });

    state
}

pub fn use_forge_workflow<TArgs, TResult>(
    function_name: &'static str,
    args: TArgs,
) -> Signal<WorkflowExecutionState<TResult>>
where
    TArgs: Serialize + Clone + PartialEq + 'static,
    TResult: DeserializeOwned + Clone + 'static,
{
    let client = use_forge_client();
    let state = use_signal(WorkflowExecutionState::<TResult>::default);
    let handle = use_hook(|| Rc::new(RefCell::new(None::<SubscriptionHandle>)));
    let effect_handle = handle.clone();

    use_effect(use_reactive!(|(args,)| {
        if let Some(existing) = effect_handle.borrow_mut().take() {
            existing.close();
        }

        let client = client.clone();
        let handle = effect_handle.clone();
        let mut state = state;
        state.set(WorkflowExecutionState::default());

        spawn(async move {
            match client
                .call::<_, WorkflowStartResponse>(function_name, args)
                .await
            {
                Ok(started) => {
                    let subscription =
                        client.subscribe_workflow(started.workflow_id.clone(), move |event| {
                            match event {
                                StreamEvent::Connection(connection_state) => {
                                    let mut next = state();
                                    next.connection_state = connection_state;
                                    state.set(next);
                                }
                                StreamEvent::Data(workflow_state) => {
                                    state.set(WorkflowExecutionState {
                                        loading: false,
                                        connection_state: state().connection_state,
                                        state: workflow_state,
                                    });
                                }
                                StreamEvent::Error(err) => {
                                    let mut next = state();
                                    next.loading = false;
                                    next.state.error = Some(err.message);
                                    state.set(next);
                                }
                            }
                        });
                    *handle.borrow_mut() = Some(subscription);
                }
                Err(err) => {
                    let mut next = state();
                    next.loading = false;
                    next.state.error = Some(err.message);
                    state.set(next);
                }
            }
        });
    }));

    use_drop({
        let handle = handle.clone();
        move || {
            if let Some(existing) = handle.borrow_mut().take() {
                existing.close();
            }
        }
    });

    state
}
