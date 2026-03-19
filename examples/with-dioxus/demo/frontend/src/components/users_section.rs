use dioxus::prelude::*;

use crate::forge::{
    CreateUserParams, DeleteUserParams, UpdateUserParams, User, use_create_user, use_delete_user,
    use_get_users_live, use_update_user,
};

#[component]
pub fn UsersSection(selected_user: Signal<Option<User>>) -> Element {
    let create_user = use_create_user();
    let update_user = use_update_user();
    let delete_user = use_delete_user();
    let state = use_get_users_live();
    let users = state.data.clone().unwrap_or_default();

    let mut name = use_signal(String::new);
    let mut email = use_signal(String::new);
    let mut is_submitting = use_signal(|| false);
    let mut editing_user_id = use_signal(|| None::<String>);
    let mut edit_name = use_signal(String::new);
    let mut edit_email = use_signal(String::new);
    let mut is_editing = use_signal(|| false);
    let mut delete_popover_id = use_signal(|| None::<String>);
    let mut crud_error = use_signal(|| None::<String>);
    let mut selected_user = selected_user;

    let submit_create = {
        let create_user = create_user.clone();
        move |event: FormEvent| {
            event.prevent_default();
            let n = name().trim().to_string();
            let e = email().trim().to_string();
            if n.is_empty() || e.is_empty() || is_submitting() {
                return;
            }
            crud_error.set(None);
            is_submitting.set(true);
            let create_user = create_user.clone();
            spawn(async move {
                match create_user.call(CreateUserParams::new(e, n)).await {
                    Ok(_) => {
                        name.set(String::new());
                        email.set(String::new());
                    }
                    Err(err) => crud_error.set(Some(err.message)),
                }
                is_submitting.set(false);
            });
        }
    };

    rsx! {
        section { class: "card",
            h2 { "Users " span { class: "badge green", "crud + subscribe" } }
            form { class: "form-row", onsubmit: submit_create,
                input {
                    r#type: "text", required: true, placeholder: "Name",
                    value: name(), oninput: move |e| name.set(e.value()),
                }
                input {
                    r#type: "email", required: true, placeholder: "Email",
                    value: email(), oninput: move |e| email.set(e.value()),
                }
                button { r#type: "submit", disabled: is_submitting(),
                    if is_submitting() { "..." } else { "Create" }
                }
            }

            if let Some(msg) = crud_error() {
                p { class: "hint warning", "{msg}" }
            }

            if !users.is_empty() {
                div { class: "table-wrap",
                    table {
                        thead { tr { th { "Name" } th { "Email" } th { "" } } }
                        tbody {
                            for user in &users {
                                if editing_user_id().as_deref() == Some(user.id.as_str()) {
                                    tr { key: "{user.id}", class: "editing",
                                        td {
                                            input {
                                                r#type: "text", value: edit_name(),
                                                oninput: move |e| edit_name.set(e.value()),
                                            }
                                        }
                                        td {
                                            input {
                                                r#type: "email", value: edit_email(),
                                                oninput: move |e| edit_email.set(e.value()),
                                            }
                                        }
                                        td {
                                            button { class: "small", disabled: is_editing(),
                                                onclick: {
                                                    let update_user = update_user.clone();
                                                    let uid = user.id.clone();
                                                    move |_| {
                                                        if is_editing() { return; }
                                                        is_editing.set(true);
                                                        crud_error.set(None);
                                                        let update_user = update_user.clone();
                                                        let uid = uid.clone();
                                                        let n = edit_name();
                                                        let e = edit_email();
                                                        spawn(async move {
                                                            match update_user.call(
                                                                UpdateUserParams::new(uid)
                                                                    .email(e)
                                                                    .name(n),
                                                            )
                                                            .await {
                                                                Ok(_) => editing_user_id.set(None),
                                                                Err(err) => crud_error.set(Some(err.message)),
                                                            }
                                                            is_editing.set(false);
                                                        });
                                                    }
                                                },
                                                "Save"
                                            }
                                            button { class: "small secondary",
                                                onclick: move |_| editing_user_id.set(None),
                                                "Cancel"
                                            }
                                        }
                                    }
                                } else {
                                    tr { key: "{user.id}",
                                        td { "{user.name}" }
                                        td { "{user.email}" }
                                        td {
                                            div { class: "action-cell",
                                                button { class: "small",
                                                    onclick: {
                                                        let user = user.clone();
                                                        move |_| {
                                                            selected_user.set(Some(user.clone()));
                                                            editing_user_id.set(Some(user.id.clone()));
                                                            edit_name.set(user.name.clone());
                                                            edit_email.set(user.email.clone());
                                                            delete_popover_id.set(None);
                                                        }
                                                    },
                                                    "Edit"
                                                }
                                                button { class: "small danger",
                                                    onclick: {
                                                        let uid = user.id.clone();
                                                        move |_| delete_popover_id.set(Some(uid.clone()))
                                                    },
                                                    "Delete"
                                                }
                                                if delete_popover_id().as_deref() == Some(user.id.as_str()) {
                                                    div { class: "popover",
                                                        button { class: "small danger",
                                                            onclick: {
                                                                let delete_user = delete_user.clone();
                                                                let uid = user.id.clone();
                                                                move |_| {
                                                                    delete_popover_id.set(None);
                                                                    crud_error.set(None);
                                                                    let delete_user = delete_user.clone();
                                                                    let uid = uid.clone();
                                                                    spawn(async move {
                                                                        match delete_user.call(DeleteUserParams::new(uid.clone())).await {
                                                                            Ok(_) => {
                                                                                if selected_user().as_ref().is_some_and(|s| s.id == uid) {
                                                                                    selected_user.set(None);
                                                                                }
                                                                            }
                                                                            Err(err) => crud_error.set(Some(err.message)),
                                                                        }
                                                                    });
                                                                }
                                                            },
                                                            "Confirm"
                                                        }
                                                        button { class: "small",
                                                            onclick: move |_| delete_popover_id.set(None),
                                                            "Cancel"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            } else if !state.loading {
                p { class: "muted", "No users yet. Create one above." }
            }
        }
    }
}
