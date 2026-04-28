use dioxus::prelude::*;
use forge_dioxus::use_signals;
use serde_json::json;

use crate::forge::{CreateTodoInput, use_create_todo, use_list_todos_subscription};
use crate::todo_item::TodoItem;

#[component]
pub fn TodoApp() -> Element {
    let signals = use_signals();
    let create_todo = use_create_todo();
    let todo_state = use_list_todos_subscription();
    let mut new_title = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let mut adding = use_signal(|| false);

    let todo_items = todo_state.data.clone().unwrap_or_default();
    let remaining_count = todo_items.iter().filter(|t| !t.completed).count();

    rsx! {
        main {
            div {
                class: "shell",
                header {
                    class: "hero",
                    h1 { "Todos" }
                }

                section {
                    class: "input-panel",
                    div {
                        class: "input-row",
                        input {
                            r#type: "text",
                            placeholder: "What needs to be done?",
                            value: new_title(),
                            disabled: adding(),
                            oninput: move |event| new_title.set(event.value()),
                            onkeydown: {
                                let create_todo = create_todo.clone();
                                let signals = signals.clone();
                                move |event: KeyboardEvent| {
                                    if event.key().to_string() == "Enter" {
                                        let title = new_title().trim().to_string();
                                        if title.is_empty() || adding() {
                                            return;
                                        }
                                        error.set(None);
                                        adding.set(true);
                                        let create_todo = create_todo.clone();
                                        let signals = signals.clone();
                                        spawn(async move {
                                            match create_todo.call(CreateTodoInput::new(title.clone())).await {
                                                Ok(_) => {
                                                    signals.track_with_properties("todo_created", json!({"title": &title}));
                                                    new_title.set(String::new());
                                                }
                                                Err(err) => {
                                                    signals.track_with_properties("todo_create_error", json!({"error": &err.message}));
                                                    error.set(Some(err.message));
                                                }
                                            }
                                            adding.set(false);
                                        });
                                    }
                                }
                            },
                        }
                        button {
                            disabled: adding() || new_title().trim().is_empty(),
                            onclick: {
                                let create_todo = create_todo.clone();
                                let signals = signals.clone();
                                move |_| {
                                    let title = new_title().trim().to_string();
                                    if title.is_empty() || adding() {
                                        return;
                                    }
                                    error.set(None);
                                    adding.set(true);
                                    let create_todo = create_todo.clone();
                                    let signals = signals.clone();
                                    spawn(async move {
                                        match create_todo.call(CreateTodoInput::new(title.clone())).await {
                                            Ok(_) => {
                                                signals.track_with_properties("todo_created", json!({"title": &title}));
                                                new_title.set(String::new());
                                            }
                                            Err(err) => {
                                                signals.track_with_properties("todo_create_error", json!({"error": &err.message}));
                                                error.set(Some(err.message));
                                            }
                                        }
                                        adding.set(false);
                                    });
                                }
                            },
                            if adding() { "Adding..." } else { "Add" }
                        }
                    }

                    if let Some(message) = error() {
                        p { class: "error", "{message}" }
                    }
                }

                section {
                    class: "list-panel",
                    if !todo_items.is_empty() {
                        div {
                            class: "list-head",
                            span { class: "summary", "{remaining_count} remaining" }
                        }
                    }

                    if todo_state.loading {
                        p { class: "status", "Loading..." }
                    } else if let Some(todo_error) = todo_state.error.as_ref() {
                        p { class: "error", "{todo_error.message}" }
                    } else if todo_items.is_empty() {
                        p { class: "status", "No todos yet. Add one above!" }
                    } else {
                        ul {
                            for todo in todo_items {
                                TodoItem {
                                    key: "{todo.id}",
                                    todo: todo,
                                    error: error,
                                }
                            }
                        }
                        p { class: "count", "{remaining_count} remaining" }
                    }
                }
            }
        }
    }
}
