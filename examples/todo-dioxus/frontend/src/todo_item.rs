use dioxus::prelude::*;

use crate::forge::{
    Todo, UpdateTodoInput, delete_todo, update_todo, use_forge_client,
};

#[component]
pub fn TodoItem(todo: Todo, mut error: Signal<Option<String>>) -> Element {
    let client = use_forge_client();
    let completed = todo.completed;
    let id = todo.id.clone();

    let toggle = {
        let client = client.clone();
        let id = id.clone();
        move |_| {
            error.set(None);
            let client = client.clone();
            let id = id.clone();
            spawn(async move {
                if let Err(err) = update_todo(
                    &client,
                    UpdateTodoInput { id, title: None, completed: Some(!completed) },
                ).await {
                    error.set(Some(err.message));
                }
            });
        }
    };

    let remove = move |_| {
        error.set(None);
        let client = client.clone();
        let id = id.clone();
        spawn(async move {
            if let Err(err) = delete_todo(&client, id).await {
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
