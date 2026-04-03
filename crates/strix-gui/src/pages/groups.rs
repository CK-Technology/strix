//! Groups management page.

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;

use crate::api::GroupInfo;
use crate::components::{Card, ConfirmModal, Header, LoadingFallback, LoadingSize, Modal, Sidebar, Table, TableRow, ToastContainer};
use crate::state::{AppState, ToastKind};

/// Groups list page.
#[component]
pub fn Groups() -> impl IntoView {
    let app_state = expect_context::<AppState>();
    let api = app_state.api.clone();
    let groups_error = RwSignal::new(Option::<String>::None);

    // Version signal to trigger refetch after mutations
    let version = RwSignal::new(0u32);

    let groups = {
        let api = api.clone();
        let app_state = app_state.clone();
        LocalResource::new(move || {
            let api = api.clone();
            let app_state = app_state.clone();
            let _v = version.get(); // Depend on version to trigger refetch
            async move {
                match api.list_groups().await {
                    Ok(r) => {
                        groups_error.set(None);
                        Ok(r.groups)
                    }
                    Err(e) => {
                        app_state.handle_error(&e);
                        let msg = e.to_string();
                        groups_error.set(Some(msg.clone()));
                        Err(msg)
                    }
                }
            }
        })
    };

    // Create group modal state
    let show_create_modal = RwSignal::new(false);
    let new_group_name = RwSignal::new(String::new());

    let open_create_modal = move |_| {
        new_group_name.set(String::new());
        show_create_modal.set(true);
    };

    let on_create = {
        let api = api.clone();
        let app_state = app_state.clone();
        move |_| {
            let name = new_group_name.get();
            if name.is_empty() {
                return;
            }

            let api = api.clone();
            let app_state = app_state.clone();

            wasm_bindgen_futures::spawn_local(async move {
                match api.create_group(&name).await {
                    Ok(_) => {
                        app_state.show_toast("Group created successfully".to_string(), ToastKind::Success);
                        show_create_modal.set(false);
                        version.update(|v| *v += 1);
                    }
                    Err(e) => {
                        app_state.show_toast(e.to_string(), ToastKind::Error);
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
                            <h1 class="text-2xl font-semibold text-white">"Groups"</h1>
                            <button
                                on:click=open_create_modal
                                class="inline-flex items-center px-4 py-2 border border-transparent text-sm font-medium rounded-md text-white bg-strix-600 hover:bg-strix-700"
                            >
                                "Create Group"
                            </button>
                        </div>

                        <Card>
                            <Suspense fallback=|| view! { <LoadingFallback size=LoadingSize::Small /> }>
                                {move || {
                                    groups.get().and_then(|data| {
                                        match &*data {
                                            Ok(group_list) => Some(view! {
                                                <GroupTable groups=group_list.clone() />
                                            }),
                                            Err(_) => None,
                                        }
                                    })
                                }}
                            </Suspense>
                            {move || groups_error.get().map(|e| view! {
                                <div class="mt-3 rounded-md bg-red-900/40 border border-red-700 p-3 text-sm text-red-200">
                                    {format!("Failed to load groups: {}", e)}
                                </div>
                            })}
                        </Card>
                    </div>
                </main>
            </div>
            <ToastContainer />

            // Create group modal
            <Modal open=show_create_modal title="Create Group">
                <div class="space-y-4">
                    <div>
                        <label class="block text-sm font-medium text-slate-300">"Group Name"</label>
                        <input
                            type="text"
                            class="mt-1 block w-full px-3 py-2 bg-slate-700 border border-slate-600 rounded-md shadow-sm text-white placeholder-slate-400 focus:outline-none focus:ring-strix-500 focus:border-strix-500 sm:text-sm"
                            placeholder="developers"
                            prop:value=move || new_group_name.get()
                            on:input=move |ev| new_group_name.set(event_target_value(&ev))
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
            </Modal>

            // Confirm delete modal
            <ConfirmModal
                state=app_state.confirm.clone()
                on_confirm={
                    let api = api.clone();
                    let app_state = app_state.clone();
                    move |action: String| {
                        if let Some(name) = action.strip_prefix("delete-group:") {
                            let name = name.to_string();
                            let api = api.clone();
                            let app_state = app_state.clone();
                            wasm_bindgen_futures::spawn_local(async move {
                                match api.delete_group(&name).await {
                                    Ok(()) => {
                                        app_state.show_toast("Group deleted".to_string(), ToastKind::Success);
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

/// Group table component.
#[component]
fn GroupTable(groups: Vec<GroupInfo>) -> impl IntoView {
    if groups.is_empty() {
        return view! {
            <div class="text-center py-12">
                <svg class="mx-auto h-12 w-12 text-slate-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M17 20h5v-2a3 3 0 00-5.356-1.857M17 20H7m10 0v-2c0-.656-.126-1.283-.356-1.857M7 20H2v-2a3 3 0 015.356-1.857M7 20v-2c0-.656.126-1.283.356-1.857m0 0a5.002 5.002 0 019.288 0M15 7a3 3 0 11-6 0 3 3 0 016 0zm6 3a2 2 0 11-4 0 2 2 0 014 0zM7 10a2 2 0 11-4 0 2 2 0 014 0z"/>
                </svg>
                <h3 class="mt-2 text-sm font-medium text-white">"No groups"</h3>
                <p class="mt-1 text-sm text-slate-400">"Get started by creating a new group."</p>
            </div>
        }.into_any();
    }

    let headers = vec!["Name", "Members", "Policies", "Created", "Actions"];

    view! {
        <Table headers=headers>
            {groups.iter().map(|group| {
                let name = group.name.clone();
                let group_route = format!("/groups/{}", group.name);
                let name_delete = group.name.clone();
                let member_count = group.members.len();
                let policy_count = group.policies.len();
                let created = format_date(&group.created_at);

                view! {
                    <TableRow>
                        <td class="px-6 py-4 whitespace-nowrap text-sm font-medium text-strix-400">
                            <A href=group_route attr:class="hover:text-strix-300">
                                {name}
                            </A>
                        </td>
                        <td class="px-6 py-4 whitespace-nowrap text-sm text-slate-400">
                            {member_count} " member"{if member_count != 1 { "s" } else { "" }}
                        </td>
                        <td class="px-6 py-4 whitespace-nowrap text-sm text-slate-400">
                            {policy_count} " polic"{if policy_count != 1 { "ies" } else { "y" }}
                        </td>
                        <td class="px-6 py-4 whitespace-nowrap text-sm text-slate-400">
                            {created}
                        </td>
                        <td class="px-6 py-4 whitespace-nowrap text-sm text-slate-400">
                            <DeleteGroupButton name=name_delete />
                        </td>
                    </TableRow>
                }
            }).collect_view()}
        </Table>
    }.into_any()
}

/// Delete group button component.
#[component]
fn DeleteGroupButton(name: String) -> impl IntoView {
    let app_state = expect_context::<AppState>();
    let name_clone = name.clone();

    let on_delete = move |_| {
        let name = name_clone.clone();
        app_state.confirm.show(
            "Delete Group",
            format!("Delete group '{}'? This cannot be undone.", name),
            format!("delete-group:{}", name),
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

/// Group detail page.
#[component]
pub fn GroupDetail() -> impl IntoView {
    let app_state = expect_context::<AppState>();
    let api = app_state.api.clone();
    let group_error = RwSignal::new(Option::<String>::None);
    let params = use_params_map();
    let group_name = move || params.read().get("name").unwrap_or_default();

    // Version signal to trigger refetch after mutations
    let version = RwSignal::new(0u32);

    let group = {
        let api = api.clone();
        let app_state = app_state.clone();
        LocalResource::new(move || {
            let api = api.clone();
            let app_state = app_state.clone();
            let name = group_name();
            let _v = version.get(); // Depend on version to trigger refetch
            async move {
                match api.get_group(&name).await {
                    Ok(g) => {
                        group_error.set(None);
                        Ok(g)
                    }
                    Err(e) => {
                        app_state.handle_error(&e);
                        let msg = e.to_string();
                        group_error.set(Some(msg.clone()));
                        Err(msg)
                    }
                }
            }
        })
    };

    // Add member modal state
    let show_add_member_modal = RwSignal::new(false);
    let new_member_username = RwSignal::new(String::new());

    let add_member = {
        let api = api.clone();
        let app_state = app_state.clone();
        move |_| {
            let username = new_member_username.get();
            if username.is_empty() {
                return;
            }

            let api = api.clone();
            let name = group_name();
            let app_state = app_state.clone();

            wasm_bindgen_futures::spawn_local(async move {
                match api.add_user_to_group(&name, &username).await {
                    Ok(()) => {
                        app_state.show_toast("User added to group".to_string(), ToastKind::Success);
                        show_add_member_modal.set(false);
                        version.update(|v| *v += 1);
                    }
                    Err(e) => {
                        app_state.show_toast(e.to_string(), ToastKind::Error);
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
                            <A href="/groups" attr:class="text-strix-400 hover:text-strix-300 mr-4">
                                <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 19l-7-7m0 0l7-7m-7 7h18"/>
                                </svg>
                            </A>
                            <h1 class="text-2xl font-semibold text-white">
                                "Group: "{group_name}
                            </h1>
                        </div>

                        <Suspense fallback=|| view! { <LoadingFallback size=LoadingSize::Small /> }>
                            {move || {
                                group.get().and_then(|data| {
                                    match &*data {
                                        Ok(g) => Some(g.clone()),
                                        Err(_) => None,
                                    }.map(|g| {
                                        let members = g.members.clone();
                                        let policies = g.policies.clone();
                                        let group_name_for_remove = group_name();

                                        view! {
                                            <div class="grid grid-cols-1 lg:grid-cols-2 gap-8">
                                                <Card title="Group Information">
                                                    <dl class="divide-y divide-slate-700">
                                                        <div class="py-3 flex justify-between text-sm">
                                                            <dt class="text-slate-400">"Name"</dt>
                                                            <dd class="text-white">{g.name.clone()}</dd>
                                                        </div>
                                                        <div class="py-3 flex justify-between text-sm">
                                                            <dt class="text-slate-400">"ARN"</dt>
                                                            <dd class="text-white font-mono text-xs">{g.arn.clone()}</dd>
                                                        </div>
                                                        <div class="py-3 flex justify-between text-sm">
                                                            <dt class="text-slate-400">"Created"</dt>
                                                            <dd class="text-white">{format_date(&g.created_at)}</dd>
                                                        </div>
                                                    </dl>
                                                </Card>

                                                <Card title="Members">
                                                    <div class="mb-4">
                                                        <button
                                                            on:click=move |_| {
                                                                new_member_username.set(String::new());
                                                                show_add_member_modal.set(true);
                                                            }
                                                            class="text-sm text-strix-400 hover:text-strix-300"
                                                        >
                                                            "+ Add Member"
                                                        </button>
                                                    </div>
                                                    <MemberList
                                                        members=members
                                                        group_name=group_name_for_remove
                                                    />
                                                </Card>

                                                <Card title="Policies">
                                                    <PolicyList policies=policies />
                                                </Card>
                                            </div>
                                        }
                                    })
                                })
                            }}
                        </Suspense>
                        {move || group_error.get().map(|e| view! {
                            <div class="mt-3 rounded-md bg-red-900/40 border border-red-700 p-3 text-sm text-red-200">
                                {format!("Failed to load group details: {}", e)}
                            </div>
                        })}
                    </div>
                </main>
            </div>
            <ToastContainer />

            // Add member modal
            <Modal open=show_add_member_modal title="Add Member to Group">
                <div class="space-y-4">
                    <div>
                        <label class="block text-sm font-medium text-slate-300">"Username"</label>
                        <input
                            type="text"
                            class="mt-1 block w-full px-3 py-2 bg-slate-700 border border-slate-600 rounded-md shadow-sm text-white placeholder-slate-400 focus:outline-none focus:ring-strix-500 focus:border-strix-500 sm:text-sm"
                            placeholder="username"
                            prop:value=move || new_member_username.get()
                            on:input=move |ev| new_member_username.set(event_target_value(&ev))
                        />
                    </div>

                    <div class="flex justify-end space-x-3">
                        <button
                            on:click=move |_| show_add_member_modal.set(false)
                            class="px-4 py-2 text-sm font-medium text-slate-300 bg-slate-700 border border-slate-600 rounded-md hover:bg-slate-600"
                        >
                            "Cancel"
                        </button>
                        <button
                            on:click=add_member
                            class="px-4 py-2 text-sm font-medium text-white bg-strix-600 rounded-md hover:bg-strix-700"
                        >
                            "Add"
                        </button>
                    </div>
                </div>
            </Modal>

            // Confirm remove member modal
            <ConfirmModal
                state=app_state.confirm.clone()
                on_confirm={
                    let api = api.clone();
                    let app_state = app_state.clone();
                    move |action: String| {
                        if let Some(rest) = action.strip_prefix("remove-member:") {
                            // Action format: remove-member:group_name:username
                            if let Some((group_name, username)) = rest.split_once(':') {
                                let group_name = group_name.to_string();
                                let username = username.to_string();
                                let api = api.clone();
                                let app_state = app_state.clone();
                                wasm_bindgen_futures::spawn_local(async move {
                                    match api.remove_user_from_group(&group_name, &username).await {
                                        Ok(()) => {
                                            app_state.show_toast("Member removed from group".to_string(), ToastKind::Success);
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
                        } else {
                            app_state.confirm.cancel();
                        }
                    }
                }
            />
        </div>
    }
}

/// Member list component.
#[component]
fn MemberList(members: Vec<String>, group_name: String) -> impl IntoView {
    if members.is_empty() {
        return view! {
            <p class="text-slate-400 text-sm">"No members in this group."</p>
        }.into_any();
    }

    view! {
        <ul class="divide-y divide-slate-700">
            {members.iter().map(|username| {
                let username_display = username.clone();
                let user_route = format!("/users/{}", username);
                let username_remove = username.clone();
                let group_name = group_name.clone();

                view! {
                    <li class="py-3">
                        <div class="flex justify-between items-center">
                            <A href=user_route attr:class="text-sm text-strix-400 hover:text-strix-300">
                                {username_display}
                            </A>
                            <RemoveMemberButton username=username_remove group_name=group_name />
                        </div>
                    </li>
                }
            }).collect_view()}
        </ul>
    }.into_any()
}

/// Remove member button component.
#[component]
fn RemoveMemberButton(username: String, group_name: String) -> impl IntoView {
    let app_state = expect_context::<AppState>();
    let username_clone = username.clone();
    let group_name_clone = group_name.clone();

    let on_remove = move |_| {
        let username = username_clone.clone();
        let group_name = group_name_clone.clone();
        app_state.confirm.show(
            "Remove Member",
            format!("Remove '{}' from group '{}'?", username, group_name),
            format!("remove-member:{}:{}", group_name, username),
        );
    };

    view! {
        <button
            class="text-red-600 hover:text-red-900 text-xs"
            on:click=on_remove
        >
            "Remove"
        </button>
    }
}

/// Policy list component.
#[component]
fn PolicyList(policies: Vec<String>) -> impl IntoView {
    if policies.is_empty() {
        return view! {
            <p class="text-slate-400 text-sm">"No policies attached to this group."</p>
        }.into_any();
    }

    view! {
        <ul class="divide-y divide-slate-700">
            {policies.iter().map(|policy_name| {
                let name = policy_name.clone();
                view! {
                    <li class="py-3">
                        <div class="flex justify-between items-center">
                            <span class="text-sm text-white">{name}</span>
                        </div>
                    </li>
                }
            }).collect_view()}
        </ul>
    }.into_any()
}

fn format_date(date_str: &str) -> String {
    date_str.split('T').next().unwrap_or(date_str).to_string()
}
