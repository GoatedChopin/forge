use dioxus::prelude::*;
use forge_dioxus::use_signals;
use serde_json::json;

use crate::forge::{DeleteTodoParams, Todo, UpdateTodoInput, use_delete_todo, use_update_todo};

#[component]
pub fn TodoItem(todo: Todo, mut error: Signal<Option<String>>) -> Element {
    let signals = use_signals();
    let update_todo = use_update_todo();
    let delete_todo = use_delete_todo();
    let completed = todo.completed;
    let id = todo.id.clone();

    let toggle = {
        let update_todo = update_todo.clone();
        let id = id.clone();
        let signals = signals.clone();
        move |_| {
            error.set(None);
            let update_todo = update_todo.clone();
            let id = id.clone();
            let signals = signals.clone();
            spawn(async move {
                signals.track_with_properties("todo_toggled", json!({"id": &id, "completed": !completed}));
                if let Err(err) = update_todo
                    .call(UpdateTodoInput::new(id).completed(!completed))
                    .await
                {
                    signals.track_with_properties("todo_toggle_error", json!({"error": &err.message}));
                    error.set(Some(err.message));
                }
            });
        }
    };

    let remove = move |_| {
        error.set(None);
        let delete_todo = delete_todo.clone();
        let id = id.clone();
        let signals = signals.clone();
        spawn(async move {
            signals.track_with_properties("todo_deleted", json!({"id": &id}));
            if let Err(err) = delete_todo.call(DeleteTodoParams::new(id)).await {
                signals.track_with_properties("todo_delete_error", json!({"error": &err.message}));
                error.set(Some(err.message));
            }
        });
    };

    rsx! {
        li {
            class: if completed { "completed" } else { "" },
            label {
                onclick: toggle,
                button {
                    r#type: "button",
                    class: if completed { "toggle checked" } else { "toggle" },
                    aria_label: if completed { "Mark todo incomplete" } else { "Mark todo complete" },
                    aria_pressed: if completed { "true" } else { "false" },
                }
                span { class: "title", "{todo.title}" }
            }
            button {
                class: "delete",
                onclick: remove,
                "Delete"
            }
        }
    }
}
