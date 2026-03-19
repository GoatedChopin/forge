use dioxus::prelude::*;

use crate::forge::{DeleteTodoParams, Todo, UpdateTodoInput, use_delete_todo, use_update_todo};

#[component]
pub fn TodoItem(todo: Todo, mut error: Signal<Option<String>>) -> Element {
    let update_todo = use_update_todo();
    let delete_todo = use_delete_todo();
    let completed = todo.completed;
    let id = todo.id.clone();

    let toggle = {
        let update_todo = update_todo.clone();
        let id = id.clone();
        move |_| {
            error.set(None);
            let update_todo = update_todo.clone();
            let id = id.clone();
            spawn(async move {
                if let Err(err) = update_todo.call(UpdateTodoInput::new(id).completed(!completed)).await
                {
                    error.set(Some(err.message));
                }
            });
        }
    };

    let remove = move |_| {
        error.set(None);
        let delete_todo = delete_todo.clone();
        let id = id.clone();
        spawn(async move {
            if let Err(err) = delete_todo.call(DeleteTodoParams::new(id)).await {
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
