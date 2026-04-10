
use dioxus::prelude::*;

use crate::ForgeClientError;
use crate::signals::{ForgeSignals, SignalsConfig, setup_auto_capture};
use crate::{ConnectionState, ForgeClient, ForgeClientConfig};

#[component]
pub fn ForgeProvider(
    url: String,
    #[props(default)] signals: Option<SignalsConfig>,
    #[props(default)] on_mutation_error: Option<EventHandler<ForgeClientError>>,
    children: Element,
) -> Element {
    let connection_state = use_context_provider(|| Signal::new(ConnectionState::Disconnected));
    let client = use_context_provider(|| {
        let mut config = ForgeClientConfig::new(url).with_connection_state(connection_state);
        if let Some(handler) = on_mutation_error {
            config = config.with_mutation_error_handler(move |err| handler.call(err));
        }
        ForgeClient::new(config)
    });

    let signals_config = signals.unwrap_or_default();
    let signals_instance = use_context_provider(|| {
        let s = ForgeSignals::new(client.clone(), signals_config);
        client.set_signals(s.clone());
        s
    });

    use_hook(|| {
        setup_auto_capture(signals_instance);
    });

    rsx! { {children} }
}

pub fn use_forge_client() -> ForgeClient {
    use_context::<ForgeClient>()
}

pub fn use_connection_state() -> Signal<ConnectionState> {
    use_context::<Signal<ConnectionState>>()
}
