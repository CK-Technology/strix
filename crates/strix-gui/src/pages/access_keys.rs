//! Access Keys management page.

use leptos::ev;
use leptos::prelude::*;
use leptos_router::components::A;
use wasm_bindgen_futures::JsFuture;

use crate::api::{AccessKeyInfo, AccessKeyResponse, UserInfo};
use crate::components::{Card, ConfirmModal, Header, LoadingFallback, LoadingSize, Modal, Sidebar, Table, TableRow, ToastContainer};
use crate::state::{AppState, ToastKind};

/// Access Keys page.
#[component]
pub fn AccessKeys() -> impl IntoView {
    let app_state = expect_context::<AppState>();
    let api = app_state.api.clone();
    let users_keys_error = RwSignal::new(Option::<String>::None);

    // Version signal to trigger refetch after mutations
    let version = RwSignal::new(0u32);

    // Fetch all users and their access keys
    let users_and_keys = {
        let api = api.clone();
        let app_state = app_state.clone();
        LocalResource::new(move || {
            let api = api.clone();
            let app_state = app_state.clone();
            let _v = version.get(); // Depend on version to trigger refetch
            async move {
                match api.list_users().await {
                    Ok(users_resp) => {
                        let mut all_keys: Vec<(UserInfo, Vec<AccessKeyInfo>)> = Vec::new();
                        for user in users_resp.users {
                            match api.list_access_keys(&user.username).await {
                                Ok(r) => all_keys.push((user, r.access_keys)),
                                Err(e) => {
                                    app_state.handle_error(&e);
                                    let msg = format!("Failed loading keys for {}: {}", user.username, e);
                                    users_keys_error.set(Some(msg.clone()));
                                    return Err(msg);
                                }
                            }
                        }
                        users_keys_error.set(None);
                        Ok(all_keys)
                    }
                    Err(e) => {
                        app_state.handle_error(&e);
                        let msg = e.to_string();
                        users_keys_error.set(Some(msg.clone()));
                        Err(msg)
                    }
                }
            }
        })
    };

    // Create key modal state
    let show_create_modal = RwSignal::new(false);
    let selected_user = RwSignal::new(String::new());
    let new_key = RwSignal::new(Option::<AccessKeyResponse>::None);

    let create_key = {
        let api = api.clone();
        let app_state = app_state.clone();
        move |_| {
            let username = selected_user.get();
            if username.is_empty() {
                app_state.show_toast("Please select a user".to_string(), ToastKind::Error);
                return;
            }

            let api = api.clone();
            let app_state = app_state.clone();
            wasm_bindgen_futures::spawn_local(async move {
                match api.create_access_key(&username).await {
                    Ok(key) => {
                        new_key.set(Some(key));
                    }
                    Err(e) => {
                        app_state.handle_error(&e);
                    }
                }
            });
        }
    };

    view! {
        <div class="flex flex-col min-h-screen">
            <Header />
            <div class="flex flex-1">
                <Sidebar />
                <main class="flex-1 p-8 bg-slate-900">
                    <div class="max-w-7xl mx-auto">
                        <div class="flex justify-between items-center mb-8">
                            <h1 class="text-2xl font-semibold text-white">"Access Keys"</h1>
                            <button
                                on:click=move |_| {
                                    selected_user.set(String::new());
                                    new_key.set(None);
                                    show_create_modal.set(true);
                                }
                                class="inline-flex items-center px-4 py-2 border border-transparent text-sm font-medium rounded-md text-white bg-strix-600 hover:bg-strix-700"
                            >
                                "Create Access Key"
                            </button>
                        </div>

                        // Info card
                        <div class="mb-8">
                            <div class="bg-blue-900/30 border border-blue-700 rounded-lg p-4">
                                <div class="flex">
                                    <div class="flex-shrink-0">
                                        <svg class="w-5 h-5 text-blue-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"/>
                                        </svg>
                                    </div>
                                    <div class="ml-3">
                                        <p class="text-sm text-blue-300">
                                            "Access keys are used to authenticate S3 API requests. Each user can have up to 2 active access keys."
                                        </p>
                                    </div>
                                </div>
                            </div>
                        </div>

                        <Card>
                            <Suspense fallback=|| view! { <LoadingFallback size=LoadingSize::Small /> }>
                                {move || {
                                    users_and_keys.get().and_then(|data| {
                                        match &*data {
                                            Ok(items) => Some(view! {
                                                <AccessKeyList items=items.clone() />
                                            }),
                                            Err(_) => None,
                                        }
                                    })
                                }}
                            </Suspense>
                            {move || users_keys_error.get().map(|e| view! {
                                <div class="mt-3 rounded-md bg-red-900/40 border border-red-700 p-3 text-sm text-red-200">
                                    {format!("Failed to load access key inventory: {}", e)}
                                </div>
                            })}
                        </Card>
                    </div>
                </main>
            </div>
            <ToastContainer />

            // Create access key modal
            <Modal open=show_create_modal title="Create Access Key">
                {move || {
                    if let Some(key) = new_key.get() {
                        view! {
                            <div class="space-y-4">
                                <div class="bg-green-900/50 border border-green-700 p-4 rounded-md">
                                    <p class="text-green-300 font-medium">"Access key created successfully!"</p>
                                </div>

                                <div class="bg-yellow-900/30 border border-yellow-700 p-4 rounded-md">
                                    <p class="text-sm text-yellow-300">
                                        "Save these credentials now. The secret key will not be shown again."
                                    </p>
                                </div>

                                <div>
                                    <label class="block text-sm font-medium text-slate-300">"Access Key ID"</label>
                                    <div class="mt-1 flex">
                                        <p class="flex-1 text-sm font-mono bg-slate-700 text-white p-2 rounded-l">{key.access_key_id.clone()}</p>
                                        <CopyButton text=key.access_key_id.clone() />
                                    </div>
                                </div>

                                <div>
                                    <label class="block text-sm font-medium text-slate-300">"Secret Access Key"</label>
                                    <div class="mt-1 flex">
                                        <p class="flex-1 text-sm font-mono bg-slate-700 text-white p-2 rounded-l break-all">{key.secret_access_key.clone()}</p>
                                        <CopyButton text=key.secret_access_key.clone() />
                                    </div>
                                </div>

                                <div class="flex justify-end">
                                    <button
                                        on:click=move |_| {
                                            version.update(|v| *v += 1);
                                            show_create_modal.set(false);
                                        }
                                        class="px-4 py-2 text-sm font-medium text-white bg-strix-600 rounded-md hover:bg-strix-700"
                                    >
                                        "Done"
                                    </button>
                                </div>
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <CreateKeyForm
                                selected_user=selected_user
                                show_create_modal=show_create_modal
                                on_create=create_key.clone()
                            />
                        }.into_any()
                    }
                }}
            </Modal>

            // Confirm delete modal
            <ConfirmModal
                state=app_state.confirm.clone()
                on_confirm={
                    let api = api.clone();
                    let app_state = app_state.clone();
                    move |action: String| {
                        if let Some(key_id) = action.strip_prefix("delete-access-key:") {
                            let key_id = key_id.to_string();
                            let api = api.clone();
                            let app_state = app_state.clone();
                            wasm_bindgen_futures::spawn_local(async move {
                                match api.delete_access_key(&key_id).await {
                                    Ok(()) => {
                                        app_state.show_toast("Access key deleted".to_string(), ToastKind::Success);
                                        version.update(|v| *v += 1);
                                    }
                                    Err(e) => {
                                        app_state.handle_error(&e);
                                    }
                                }
                                app_state.confirm.done();
                            });
                        } else {
                            app_state.confirm.cancel();
                        }
                    }
                }
            />
        </div>
    }
}

/// Create key form component.
#[component]
fn CreateKeyForm(
    selected_user: RwSignal<String>,
    show_create_modal: RwSignal<bool>,
    on_create: impl Fn(ev::MouseEvent) + 'static,
) -> impl IntoView {
    let app_state = expect_context::<AppState>();
    let api = app_state.api.clone();
    let users_error = RwSignal::new(Option::<String>::None);

    let users = {
        let api = api.clone();
        let app_state = app_state.clone();
        LocalResource::new(move || {
            let api = api.clone();
            let app_state = app_state.clone();
            async move {
                match api.list_users().await {
                    Ok(r) => {
                        users_error.set(None);
                        Ok(r.users)
                    }
                    Err(e) => {
                        app_state.handle_error(&e);
                        let msg = e.to_string();
                        users_error.set(Some(msg.clone()));
                        Err(msg)
                    }
                }
            }
        })
    };

    view! {
        <div class="space-y-4">
            <div>
                <label class="block text-sm font-medium text-slate-300">"Select User"</label>
                <Suspense fallback=|| view! { <LoadingFallback message="Loading users..." size=LoadingSize::Small /> }>
                    {move || {
                        users.get().and_then(|data| {
                            match &*data {
                                Ok(user_list) => Some(view! {
                                <select
                                    class="mt-1 block w-full px-3 py-2 bg-slate-700 border border-slate-600 rounded-md shadow-sm text-white focus:outline-none focus:ring-strix-500 focus:border-strix-500 sm:text-sm"
                                    on:change=move |ev| selected_user.set(event_target_value(&ev))
                                >
                                    <option value="">"-- Select a user --"</option>
                                    {user_list.iter().map(|user| {
                                        let username = user.username.clone();
                                        let username_display = username.clone();
                                        view! {
                                            <option value=username>{username_display}</option>
                                        }
                                    }).collect_view()}
                                </select>
                                }),
                                Err(_) => None,
                            }
                        })
                    }}
                </Suspense>
                {move || users_error.get().map(|e| view! {
                    <p class="mt-2 text-sm text-red-300">{format!("Failed to load users: {}", e)}</p>
                })}
            </div>

            <div class="flex justify-end space-x-3">
                <button
                    on:click=move |_| show_create_modal.set(false)
                    class="px-4 py-2 text-sm font-medium text-slate-300 bg-slate-700 border border-slate-600 rounded-md hover:bg-slate-600"
                >
                    "Cancel"
                </button>
                <button
                    on:click=on_create
                    class="px-4 py-2 text-sm font-medium text-white bg-strix-600 rounded-md hover:bg-strix-700"
                >
                    "Create"
                </button>
            </div>
        </div>
    }
}

/// Access key list component.
#[component]
fn AccessKeyList(items: Vec<(UserInfo, Vec<AccessKeyInfo>)>) -> impl IntoView {
    // Flatten into a single list of keys with user info
    let all_keys: Vec<(String, AccessKeyInfo)> = items
        .into_iter()
        .flat_map(|(user, keys)| {
            keys.into_iter().map(move |key| (user.username.clone(), key))
        })
        .collect();

    if all_keys.is_empty() {
        return view! {
            <div class="text-center py-12">
                <svg class="mx-auto h-12 w-12 text-slate-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 7a2 2 0 012 2m4 0a6 6 0 01-7.743 5.743L11 17H9v2H7v2H4a1 1 0 01-1-1v-2.586a1 1 0 01.293-.707l5.964-5.964A6 6 0 1121 9z"/>
                </svg>
                <h3 class="mt-2 text-sm font-medium text-white">"No access keys"</h3>
                <p class="mt-1 text-sm text-slate-400">"Create access keys to authenticate API requests."</p>
            </div>
        }.into_any();
    }

    let headers = vec!["Access Key ID", "User", "Status", "Created", "Actions"];

    view! {
        <Table headers=headers>
            {all_keys.iter().map(|(username, key)| {
                let key_id = key.access_key_id.clone();
                let key_id_display = key.access_key_id.clone();
                let key_id_delete = key.access_key_id.clone();
                let username = username.clone();
                let user_route = format!("/users/{}", username);
                let status = key.status.clone();
                let created = format_date(&key.created_at);

                let status_class = if status == "active" {
                    "bg-green-100 text-green-800"
                } else {
                    "bg-red-100 text-red-800"
                };

                view! {
                    <TableRow>
                        <td class="px-6 py-4 whitespace-nowrap text-sm font-mono text-white">
                            <div class="flex items-center">
                                <span>{key_id_display}</span>
                                <CopyButton text=key_id />
                            </div>
                        </td>
                        <td class="px-6 py-4 whitespace-nowrap text-sm">
                            <A href=user_route attr:class="text-strix-400 hover:text-strix-300">
                                {username.clone()}
                            </A>
                        </td>
                        <td class="px-6 py-4 whitespace-nowrap text-sm">
                            <span class=format!("px-2 inline-flex text-xs leading-5 font-semibold rounded-full {}", status_class)>
                                {status}
                            </span>
                        </td>
                        <td class="px-6 py-4 whitespace-nowrap text-sm text-slate-400">
                            {created}
                        </td>
                        <td class="px-6 py-4 whitespace-nowrap text-sm text-slate-400">
                            <DeleteKeyButton key_id=key_id_delete />
                        </td>
                    </TableRow>
                }
            }).collect_view()}
        </Table>
    }.into_any()
}

/// Copy button component.
#[component]
fn CopyButton(text: String) -> impl IntoView {
    let app_state = expect_context::<AppState>();
    let copied = RwSignal::new(false);
    let text_clone = text.clone();

    let on_copy = move |_| {
        let text = text_clone.clone();
        let app_state = app_state.clone();

        wasm_bindgen_futures::spawn_local(async move {
            let write_result = async {
                let window = web_sys::window()
                    .ok_or_else(|| "Clipboard unavailable: no browser window".to_string())?;
                let clipboard = window
                    .navigator()
                    .clipboard();
                let promise = clipboard.write_text(&text);
                JsFuture::from(promise)
                    .await
                    .map_err(|_| "Clipboard write rejected".to_string())?;
                Ok::<(), String>(())
            }
            .await;

            match write_result {
                Ok(()) => {
                    copied.set(true);
                    gloo_timers::future::TimeoutFuture::new(2000).await;
                    copied.set(false);
                }
                Err(msg) => {
                    app_state.show_toast(msg, ToastKind::Error);
                }
            }
        });
    };

    view! {
        <button
            on:click=on_copy
            class="px-2 py-2 bg-slate-600 hover:bg-slate-500 rounded-r text-slate-300"
            title="Copy to clipboard"
        >
            {move || if copied.get() {
                view! {
                    <svg class="w-4 h-4 text-green-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7"/>
                    </svg>
                }.into_any()
            } else {
                view! {
                    <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z"/>
                    </svg>
                }.into_any()
            }}
        </button>
    }
}

/// Delete key button component.
#[component]
fn DeleteKeyButton(key_id: String) -> impl IntoView {
    let app_state = expect_context::<AppState>();
    let key_id_clone = key_id.clone();

    let on_delete = move |_| {
        let key_id = key_id_clone.clone();
        app_state.confirm.show(
            "Delete Access Key",
            format!("Delete access key '{}'? This cannot be undone.", key_id),
            format!("delete-access-key:{}", key_id),
        );
    };

    view! {
        <button
            class="text-red-600 hover:text-red-900"
            on:click=on_delete
        >
            "Delete"
        </button>
    }
}

fn format_date(date_str: &str) -> String {
    date_str.split('T').next().unwrap_or(date_str).to_string()
}
