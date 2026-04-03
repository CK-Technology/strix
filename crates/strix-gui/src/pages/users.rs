//! Users pages.

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;

use crate::api::{AccessKeyInfo, AccessKeyResponse, CreateUserResponse, UserInfo};
use crate::components::{Card, ConfirmModal, Header, LoadingFallback, LoadingSize, Modal, Sidebar, Table, TableRow, ToastContainer};
use crate::state::{AppState, ToastKind};

/// Users list page.
#[component]
pub fn Users() -> impl IntoView {
    let app_state = expect_context::<AppState>();
    let api = app_state.api.clone();
    let users_error = RwSignal::new(Option::<String>::None);

    // Version signal to trigger refetch after mutations
    let version = RwSignal::new(0u32);

    // Search filter
    let search_query = RwSignal::new(String::new());

    let users = {
        let api = api.clone();
        let app_state = app_state.clone();
        LocalResource::new(move || {
            let api = api.clone();
            let app_state = app_state.clone();
            let _v = version.get(); // Depend on version to trigger refetch
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

    // Filter users based on search
    let filtered_users = move || {
        users.get().and_then(|data| {
            match &*data {
                Ok(user_list) => Some(user_list.clone()),
                Err(_) => None,
            }.map(|user_list| {
                let query = search_query.get().to_lowercase();
                if query.is_empty() {
                    user_list
                } else {
                    user_list.into_iter().filter(|u| {
                        u.username.to_lowercase().contains(&query)
                    }).collect()
                }
            })
        })
    };

    // Create user modal state
    let show_create_modal = RwSignal::new(false);
    let new_username = RwSignal::new(String::new());
    let created_user = RwSignal::new(Option::<CreateUserResponse>::None);

    let open_create_modal = move |_| {
        new_username.set(String::new());
        created_user.set(None);
        show_create_modal.set(true);
    };

    // Clone for use in different closures
    let confirm_state = app_state.confirm.clone();
    let app_state_modal = app_state.clone();
    let app_state_confirm = app_state.clone();

    view! {
        <div class="flex flex-col min-h-screen">
            <Header />
            <div class="flex flex-1">
                <Sidebar />
                <main class="flex-1 p-8 bg-slate-900">
                    <div class="max-w-7xl mx-auto">
                        <div class="flex justify-between items-center mb-6">
                            <h1 class="text-2xl font-semibold text-white">"Users"</h1>
                            <button
                                on:click=open_create_modal
                                class="inline-flex items-center px-4 py-2 border border-transparent text-sm font-medium rounded-md text-white bg-strix-600 hover:bg-strix-700"
                            >
                                <svg class="w-5 h-5 mr-2" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 6v6m0 0v6m0-6h6m-6 0H6"/>
                                </svg>
                                "Create User"
                            </button>
                        </div>

                        // Search bar
                        <div class="mb-6">
                            <div class="relative max-w-md">
                                <div class="absolute inset-y-0 left-0 pl-3 flex items-center pointer-events-none">
                                    <svg class="h-5 w-5 text-slate-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"/>
                                    </svg>
                                </div>
                                <input
                                    type="text"
                                    placeholder="Search users..."
                                    class="block w-full pl-10 pr-3 py-2 border border-slate-600 rounded-md bg-slate-800 text-white placeholder-slate-400 focus:outline-none focus:ring-1 focus:ring-strix-500 focus:border-strix-500 sm:text-sm"
                                    prop:value=move || search_query.get()
                                    on:input=move |ev| search_query.set(event_target_value(&ev))
                                />
                            </div>
                        </div>

                        <Card>
                            <Suspense fallback=|| view! { <LoadingFallback size=LoadingSize::Small /> }>
                                {move || {
                                    filtered_users().map(|user_list| {
                                        let total_count = users.get()
                                            .and_then(|d| match &*d {
                                                Ok(l) => Some(l.len()),
                                                Err(_) => None,
                                            })
                                            .unwrap_or(0);
                                        let filtered_count = user_list.len();
                                        let is_filtered = !search_query.get().is_empty();

                                        view! {
                                            <>
                                                {is_filtered.then(|| view! {
                                                    <div class="mb-4 text-sm text-slate-400">
                                                        "Showing " {filtered_count} " of " {total_count} " users"
                                                    </div>
                                                })}
                                                <UserTable users=user_list />
                                            </>
                                        }
                                    })
                                }}
                            </Suspense>
                            {move || users_error.get().map(|e| view! {
                                <div class="mt-3 rounded-md bg-red-900/40 border border-red-700 p-3 text-sm text-red-200">
                                    {format!("Failed to load users: {}", e)}
                                </div>
                            })}
                        </Card>
                    </div>
                </main>
            </div>
            <ToastContainer />

            // Create user modal
            <Modal open=show_create_modal title="Create User">
                <CreateUserModalContent
                    created_user=created_user
                    new_username=new_username
                    show_create_modal=show_create_modal
                    app_state=app_state_modal
                    on_created=move || version.update(|v| *v += 1)
                />
            </Modal>

            // Confirm delete modal
            <ConfirmModal
                state=confirm_state
                on_confirm={
                    let api = api.clone();
                    let app_state = app_state_confirm.clone();
                    move |action: String| {
                        if let Some(username) = action.strip_prefix("delete-user:") {
                            let username = username.to_string();
                            let api = api.clone();
                            let app_state = app_state.clone();
                            wasm_bindgen_futures::spawn_local(async move {
                                match api.delete_user(&username).await {
                                    Ok(()) => {
                                        app_state.show_toast("User deleted".to_string(), ToastKind::Success);
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

/// Create user modal content component.
#[component]
fn CreateUserModalContent(
    created_user: RwSignal<Option<CreateUserResponse>>,
    new_username: RwSignal<String>,
    show_create_modal: RwSignal<bool>,
    app_state: AppState,
    on_created: impl Fn() + Clone + Send + Sync + 'static,
) -> impl IntoView {
    // If user was created, show success view
    let on_created_clone = on_created.clone();
    if let Some(user) = created_user.get() {
        return view! {
            <div class="space-y-4">
                <div class="bg-green-900/50 border border-green-700 p-4 rounded-md">
                    <p class="text-green-300 font-medium">"User created successfully!"</p>
                </div>

                <div>
                    <label class="block text-sm font-medium text-slate-300">"Username"</label>
                    <p class="mt-1 text-sm text-white">{user.user.username.clone()}</p>
                </div>

                {user.access_key.as_ref().map(|key| view! {
                    <div class="space-y-4 p-4 bg-yellow-900/30 border border-yellow-700 rounded-md">
                        <p class="text-sm text-yellow-300">
                            "Save these credentials now. The secret key will not be shown again."
                        </p>
                        <div>
                            <label class="block text-sm font-medium text-slate-300">"Access Key"</label>
                            <p class="mt-1 text-sm font-mono bg-slate-700 text-white p-2 rounded">{key.access_key_id.clone()}</p>
                        </div>
                        <div>
                            <label class="block text-sm font-medium text-slate-300">"Secret Key"</label>
                            <p class="mt-1 text-sm font-mono bg-slate-700 text-white p-2 rounded break-all">
                                {key.secret_access_key.clone().unwrap_or_default()}
                            </p>
                        </div>
                    </div>
                })}

                <div class="flex justify-end">
                    <button
                        on:click=move |_| {
                            on_created_clone();
                            show_create_modal.set(false);
                        }
                        class="px-4 py-2 text-sm font-medium text-white bg-strix-600 rounded-md hover:bg-strix-700"
                    >
                        "Done"
                    </button>
                </div>
            </div>
        }.into_any();
    }

    // Otherwise show create form
    let on_create = {
        let api = app_state.api.clone();
        let app_state_clone = app_state.clone();
        move |_| {
            let username = new_username.get();
            if username.is_empty() {
                return;
            }

            let api = api.clone();
            let app_state_clone = app_state_clone.clone();

            wasm_bindgen_futures::spawn_local(async move {
                match api.create_user(&username).await {
                    Ok(response) => {
                        created_user.set(Some(response));
                        app_state_clone.show_toast("User created successfully".to_string(), ToastKind::Success);
                    }
                    Err(e) => {
                        app_state_clone.show_toast(e.to_string(), ToastKind::Error);
                    }
                }
            });
        }
    };

    view! {
        <div class="space-y-4">
            <div>
                <label class="block text-sm font-medium text-slate-300">"Username"</label>
                <input
                    type="text"
                    class="mt-1 block w-full px-3 py-2 bg-slate-700 border border-slate-600 rounded-md shadow-sm text-white placeholder-slate-400 focus:outline-none focus:ring-strix-500 focus:border-strix-500 sm:text-sm"
                    placeholder="newuser"
                    prop:value=move || new_username.get()
                    on:input=move |ev| new_username.set(event_target_value(&ev))
                />
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
    }.into_any()
}

/// User table component.
#[component]
fn UserTable(users: Vec<UserInfo>) -> impl IntoView {
    // Early return for empty state
    if users.is_empty() {
        return view! {
            <div class="text-center py-12">
                <svg class="mx-auto h-12 w-12 text-slate-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4.354a4 4 0 110 5.292M15 21H3v-1a6 6 0 0112 0v1zm0 0h6v-1a6 6 0 00-9-5.197M13 7a4 4 0 11-8 0 4 4 0 018 0z"/>
                </svg>
                <h3 class="mt-2 text-sm font-medium text-white">"No users"</h3>
                <p class="mt-1 text-sm text-slate-400">"Get started by creating a new user."</p>
            </div>
        }.into_any();
    }

    let headers = vec!["Username", "Status", "Policies", "Created", "Actions"];

    view! {
        <Table headers=headers>
            {users.iter().map(|user| {
                let username = user.username.clone();
                let user_route = format!("/users/{}", user.username);
                let username_delete = user.username.clone();
                let status = user.status.clone();
                let policies = if user.policies.is_empty() {
                    "None".to_string()
                } else {
                    user.policies.join(", ")
                };
                let created = format_date(&user.created_at);

                let (status_class, status_icon) = if status == "active" {
                    (
                        "bg-green-900/50 text-green-400 border border-green-700/50",
                        view! {
                            <svg class="w-3 h-3 mr-1" fill="currentColor" viewBox="0 0 20 20">
                                <path fill-rule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zm3.707-9.293a1 1 0 00-1.414-1.414L9 10.586 7.707 9.293a1 1 0 00-1.414 1.414l2 2a1 1 0 001.414 0l4-4z" clip-rule="evenodd"/>
                            </svg>
                        }.into_any()
                    )
                } else {
                    (
                        "bg-red-900/50 text-red-400 border border-red-700/50",
                        view! {
                            <svg class="w-3 h-3 mr-1" fill="currentColor" viewBox="0 0 20 20">
                                <path fill-rule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zM8.707 7.293a1 1 0 00-1.414 1.414L8.586 10l-1.293 1.293a1 1 0 101.414 1.414L10 11.414l1.293 1.293a1 1 0 001.414-1.414L11.414 10l1.293-1.293a1 1 0 00-1.414-1.414L10 8.586 8.707 7.293z" clip-rule="evenodd"/>
                            </svg>
                        }.into_any()
                    )
                };
                let status_display = status.clone();

                view! {
                    <TableRow>
                        <td class="px-6 py-4 whitespace-nowrap text-sm font-medium text-strix-400">
                            <A href=user_route attr:class="hover:text-strix-300">
                                {username}
                            </A>
                        </td>
                        <td class="px-6 py-4 whitespace-nowrap text-sm">
                            <span class=format!("px-2 py-1 inline-flex items-center text-xs font-semibold rounded-full {}", status_class)>
                                {status_icon}
                                {status_display}
                            </span>
                        </td>
                        <td class="px-6 py-4 whitespace-nowrap text-sm text-slate-400">
                            {policies}
                        </td>
                        <td class="px-6 py-4 whitespace-nowrap text-sm text-slate-400">
                            {created}
                        </td>
                        <td class="px-6 py-4 whitespace-nowrap text-sm text-slate-400">
                            <DeleteUserButton username=username_delete />
                        </td>
                    </TableRow>
                }
            }).collect_view()}
        </Table>
    }.into_any()
}

/// Delete user button component.
#[component]
fn DeleteUserButton(
    username: String,
) -> impl IntoView {
    let app_state = expect_context::<AppState>();
    let username_clone = username.clone();

    let on_delete = move |_| {
        let username = username_clone.clone();
        app_state.confirm.show(
            "Delete User",
            format!("Delete user '{}'? This cannot be undone.", username),
            format!("delete-user:{}", username),
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

/// User detail page.
#[component]
pub fn UserDetail() -> impl IntoView {
    let app_state = expect_context::<AppState>();
    let api = app_state.api.clone();
    let user_error = RwSignal::new(Option::<String>::None);
    let access_keys_error = RwSignal::new(Option::<String>::None);
    let params = use_params_map();
    let username = move || params.read().get("username").unwrap_or_default();

    // Version signal for access keys to trigger refetch after mutations
    let keys_version = RwSignal::new(0u32);

    let user = {
        let api = api.clone();
        let app_state = app_state.clone();
        LocalResource::new(move || {
            let api = api.clone();
            let app_state = app_state.clone();
            let username = username();
            async move {
                match api.get_user(&username).await {
                    Ok(u) => {
                        user_error.set(None);
                        Ok(u)
                    }
                    Err(e) => {
                        app_state.handle_error(&e);
                        let msg = e.to_string();
                        user_error.set(Some(msg.clone()));
                        Err(msg)
                    }
                }
            }
        })
    };

    let access_keys = {
        let api = api.clone();
        let app_state = app_state.clone();
        LocalResource::new(move || {
            let api = api.clone();
            let app_state = app_state.clone();
            let username = username();
            let _v = keys_version.get(); // Depend on version to trigger refetch
            async move {
                match api.list_access_keys(&username).await {
                    Ok(r) => {
                        access_keys_error.set(None);
                        Ok(r.access_keys)
                    }
                    Err(e) => {
                        app_state.handle_error(&e);
                        let msg = e.to_string();
                        access_keys_error.set(Some(msg.clone()));
                        Err(msg)
                    }
                }
            }
        })
    };

    // Create access key state
    let new_key = RwSignal::new(Option::<AccessKeyResponse>::None);
    let show_key_modal = RwSignal::new(false);

    let create_key = {
        let api = api.clone();
        let app_state = app_state.clone();
        move |_| {
            let api = api.clone();
            let username = username();
            let app_state_clone = app_state.clone();

            wasm_bindgen_futures::spawn_local(async move {
                match api.create_access_key(&username).await {
                    Ok(key) => {
                        new_key.set(Some(key));
                        show_key_modal.set(true);
                    }
                    Err(e) => {
                        app_state_clone.show_toast(e.to_string(), ToastKind::Error);
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
                        <div class="flex items-center mb-8">
                            <A href="/users" attr:class="text-strix-400 hover:text-strix-300 mr-4">
                                <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 19l-7-7m0 0l7-7m-7 7h18"/>
                                </svg>
                            </A>
                            <h1 class="text-2xl font-semibold text-white">
                                "User: "{username}
                            </h1>
                        </div>

                        <div class="grid grid-cols-1 lg:grid-cols-2 gap-8">
                            <Card title="User Information">
                                <Suspense fallback=|| view! { <LoadingFallback size=LoadingSize::Small /> }>
                                    {move || {
                                        user.get().and_then(|data| {
                                            match &*data {
                                                Ok(u) => Some(view! {
                                                <dl class="divide-y divide-slate-700">
                                                    <div class="py-3 flex justify-between text-sm">
                                                        <dt class="text-slate-400">"Username"</dt>
                                                        <dd class="text-white">{u.username.clone()}</dd>
                                                    </div>
                                                    <div class="py-3 flex justify-between text-sm">
                                                        <dt class="text-slate-400">"ARN"</dt>
                                                        <dd class="text-white font-mono text-xs">{u.arn.clone()}</dd>
                                                    </div>
                                                    <div class="py-3 flex justify-between text-sm items-center">
                                                        <dt class="text-slate-400">"Status"</dt>
                                                        <dd>
                                                            {if u.status == "active" {
                                                                view! {
                                                                    <span class="px-2 py-1 inline-flex items-center text-xs font-semibold rounded-full bg-green-900/50 text-green-400 border border-green-700/50">
                                                                        <svg class="w-3 h-3 mr-1" fill="currentColor" viewBox="0 0 20 20">
                                                                            <path fill-rule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zm3.707-9.293a1 1 0 00-1.414-1.414L9 10.586 7.707 9.293a1 1 0 00-1.414 1.414l2 2a1 1 0 001.414 0l4-4z" clip-rule="evenodd"/>
                                                                        </svg>
                                                                        {u.status.clone()}
                                                                    </span>
                                                                }.into_any()
                                                            } else {
                                                                view! {
                                                                    <span class="px-2 py-1 inline-flex items-center text-xs font-semibold rounded-full bg-red-900/50 text-red-400 border border-red-700/50">
                                                                        <svg class="w-3 h-3 mr-1" fill="currentColor" viewBox="0 0 20 20">
                                                                            <path fill-rule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zM8.707 7.293a1 1 0 00-1.414 1.414L8.586 10l-1.293 1.293a1 1 0 101.414 1.414L10 11.414l1.293 1.293a1 1 0 001.414-1.414L11.414 10l1.293-1.293a1 1 0 00-1.414-1.414L10 8.586 8.707 7.293z" clip-rule="evenodd"/>
                                                                        </svg>
                                                                        {u.status.clone()}
                                                                    </span>
                                                                }.into_any()
                                                            }}
                                                        </dd>
                                                    </div>
                                                    <div class="py-3 flex justify-between text-sm">
                                                        <dt class="text-slate-400">"Policies"</dt>
                                                        <dd class="text-white">
                                                            {if u.policies.is_empty() { "None".to_string() } else { u.policies.join(", ") }}
                                                        </dd>
                                                    </div>
                                                    <div class="py-3 flex justify-between text-sm">
                                                        <dt class="text-slate-400">"Created"</dt>
                                                        <dd class="text-white">{format_date(&u.created_at)}</dd>
                                                    </div>
                                                </dl>
                                                }),
                                                Err(_) => None,
                                            }
                                        })
                                    }}
                                </Suspense>
                                {move || user_error.get().map(|e| view! {
                                    <div class="mt-3 rounded-md bg-red-900/40 border border-red-700 p-3 text-sm text-red-200">
                                        {format!("Failed to load user details: {}", e)}
                                    </div>
                                })}
                            </Card>

                            <Card title="Access Keys">
                                <div class="mb-4">
                                    <button
                                        on:click=create_key
                                        class="text-sm text-strix-400 hover:text-strix-300"
                                    >
                                        "+ Create Access Key"
                                    </button>
                                </div>
                                <Suspense fallback=|| view! { <LoadingFallback size=LoadingSize::Small /> }>
                                    {move || {
                                        access_keys.get().and_then(|data| {
                                            match &*data {
                                                Ok(keys) => Some(view! {
                                                    <AccessKeyList keys=keys.clone() />
                                                }),
                                                Err(_) => None,
                                            }
                                        })
                                    }}
                                </Suspense>
                                {move || access_keys_error.get().map(|e| view! {
                                    <div class="mt-3 rounded-md bg-red-900/40 border border-red-700 p-3 text-sm text-red-200">
                                        {format!("Failed to load access keys: {}", e)}
                                    </div>
                                })}
                            </Card>
                        </div>
                    </div>
                </main>
            </div>
            <ToastContainer />

            // New access key modal
            <Modal
                open=show_key_modal
                title="New Access Key"
            >
                {move || {
                    new_key.get().map(|key| view! {
                        <div class="space-y-4">
                            <div class="bg-yellow-900/30 border border-yellow-700 p-4 rounded-md">
                                <p class="text-sm text-yellow-300">
                                    "Save these credentials now. The secret key will not be shown again."
                                </p>
                            </div>
                            <div>
                                <label class="block text-sm font-medium text-slate-300">"Access Key"</label>
                                <p class="mt-1 text-sm font-mono bg-slate-700 text-white p-2 rounded">{key.access_key_id.clone()}</p>
                            </div>
                            <div>
                                <label class="block text-sm font-medium text-slate-300">"Secret Key"</label>
                                <p class="mt-1 text-sm font-mono bg-slate-700 text-white p-2 rounded break-all">{key.secret_access_key.clone()}</p>
                            </div>
                            <div class="flex justify-end">
                                <button
                                    on:click=move |_| {
                                        keys_version.update(|v| *v += 1);
                                        show_key_modal.set(false);
                                    }
                                    class="px-4 py-2 text-sm font-medium text-white bg-strix-600 rounded-md hover:bg-strix-700"
                                >
                                    "Done"
                                </button>
                            </div>
                        </div>
                    })
                }}
            </Modal>

            // Confirm delete modal for access keys
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
                                        keys_version.update(|v| *v += 1);
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

/// Access key list component.
#[component]
fn AccessKeyList(keys: Vec<AccessKeyInfo>) -> impl IntoView {
    // Early return for empty state
    if keys.is_empty() {
        return view! {
            <p class="text-slate-400 text-sm">"No access keys."</p>
        }.into_any();
    }

    view! {
        <ul class="divide-y divide-slate-700">
            {keys.iter().map(|key| {
                let key_id_display = key.access_key_id.clone();
                let key_id_delete = key.access_key_id.clone();
                let status = key.status.clone();
                let created = format_date(&key.created_at);
                let is_active = status == "active";
                view! {
                    <li class="py-3">
                        <div class="flex justify-between items-center">
                            <div>
                                <p class="text-sm font-mono text-white">{key_id_display}</p>
                                <p class="text-xs text-slate-400">"Created: "{created}</p>
                            </div>
                            <div class="flex items-center space-x-4">
                                {if is_active {
                                    view! {
                                        <span class="px-2 py-0.5 inline-flex items-center text-xs font-semibold rounded-full bg-green-900/50 text-green-400 border border-green-700/50">
                                            <svg class="w-3 h-3 mr-1" fill="currentColor" viewBox="0 0 20 20">
                                                <path fill-rule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zm3.707-9.293a1 1 0 00-1.414-1.414L9 10.586 7.707 9.293a1 1 0 00-1.414 1.414l2 2a1 1 0 001.414 0l4-4z" clip-rule="evenodd"/>
                                            </svg>
                                            {status}
                                        </span>
                                    }.into_any()
                                } else {
                                    view! {
                                        <span class="px-2 py-0.5 inline-flex items-center text-xs font-semibold rounded-full bg-red-900/50 text-red-400 border border-red-700/50">
                                            <svg class="w-3 h-3 mr-1" fill="currentColor" viewBox="0 0 20 20">
                                                <path fill-rule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zM8.707 7.293a1 1 0 00-1.414 1.414L8.586 10l-1.293 1.293a1 1 0 101.414 1.414L10 11.414l1.293 1.293a1 1 0 001.414-1.414L11.414 10l1.293-1.293a1 1 0 00-1.414-1.414L10 8.586 8.707 7.293z" clip-rule="evenodd"/>
                                            </svg>
                                            {status}
                                        </span>
                                    }.into_any()
                                }}
                                <DeleteAccessKeyButton key_id=key_id_delete />
                            </div>
                        </div>
                    </li>
                }
            }).collect_view()}
        </ul>
    }.into_any()
}

/// Delete access key button component.
#[component]
fn DeleteAccessKeyButton(key_id: String) -> impl IntoView {
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
            class="text-red-600 hover:text-red-900 text-xs"
            on:click=on_delete
        >
            "Delete"
        </button>
    }
}

fn format_date(date_str: &str) -> String {
    date_str.split('T').next().unwrap_or(date_str).to_string()
}
