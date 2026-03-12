use dioxus::prelude::*;

use crate::forge::{
    CreateTodoInput, create_todo, use_forge_client, use_list_todos_subscription,
};
use crate::todo_item::TodoItem;

fn submit_todo(
    client: crate::forge::ForgeClient,
    mut new_title: Signal<String>,
    mut error: Signal<Option<String>>,
    mut adding: Signal<bool>,
) {
    let title = new_title().trim().to_string();
    if title.is_empty() || adding() {
        return;
    }
    error.set(None);
    adding.set(true);
    spawn(async move {
        match create_todo(&client, CreateTodoInput { title }).await {
            Ok(_) => new_title.set(String::new()),
            Err(err) => error.set(Some(err.message)),
        }
        adding.set(false);
    });
}

#[component]
pub fn TodoApp() -> Element {
    let client = use_forge_client();
    let todos = use_list_todos_subscription();
    let mut new_title = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let mut adding = use_signal(|| false);

    let todo_state = todos();
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
                                let client = client.clone();
                                move |event: KeyboardEvent| {
                                    if event.key().to_string() == "Enter" {
                                        submit_todo(client.clone(), new_title, error, adding);
                                    }
                                }
                            },
                        }
                        button {
                            disabled: adding() || new_title().trim().is_empty(),
                            onclick: {
                                let client = client.clone();
                                move |_| submit_todo(client.clone(), new_title, error, adding)
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
