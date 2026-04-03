//! Policies management page.

use leptos::prelude::*;

use crate::api::{PolicyDocument, PolicyInfo, PolicyStatement};
use crate::components::{Card, ConfirmModal, Header, LoadingFallback, LoadingSize, Modal, Sidebar, Table, TableRow, ToastContainer};
use crate::state::{AppState, ToastKind};

/// Policies list page.
#[component]
pub fn Policies() -> impl IntoView {
    let app_state = expect_context::<AppState>();
    let api = app_state.api.clone();
    let policies_error = RwSignal::new(Option::<String>::None);

    // Version signal to trigger refetch after mutations
    let version = RwSignal::new(0u32);

    let policies = {
        let api = api.clone();
        let app_state = app_state.clone();
        LocalResource::new(move || {
            let api = api.clone();
            let app_state = app_state.clone();
            let _v = version.get(); // Depend on version to trigger refetch
            async move {
                match api.list_policies().await {
                    Ok(r) => {
                        policies_error.set(None);
                        Ok(r.policies)
                    }
                    Err(e) => {
                        app_state.handle_error(&e);
                        let msg = e.to_string();
                        policies_error.set(Some(msg.clone()));
                        Err(msg)
                    }
                }
            }
        })
    };

    // Create policy modal state
    let show_create_modal = RwSignal::new(false);
    let new_policy_name = RwSignal::new(String::new());
    let new_policy_description = RwSignal::new(String::new());
    let new_policy_json = RwSignal::new(default_policy_json());
    let json_error = RwSignal::new(Option::<String>::None);

    let open_create_modal = move |_| {
        new_policy_name.set(String::new());
        new_policy_description.set(String::new());
        new_policy_json.set(default_policy_json());
        json_error.set(None);
        show_create_modal.set(true);
    };

    let on_create = {
        let api = api.clone();
        let app_state = app_state.clone();
        move |_| {
            let name = new_policy_name.get();
            if name.is_empty() {
                json_error.set(Some("Policy name is required".to_string()));
                return;
            }

            // Parse the JSON
            let json_str = new_policy_json.get();
            let statements: Vec<PolicyStatement> = match serde_json::from_str(&json_str) {
                Ok(s) => s,
                Err(e) => {
                    json_error.set(Some(format!("Invalid JSON: {}", e)));
                    return;
                }
            };

            let policy = PolicyDocument {
                name: name.clone(),
                version: "2012-10-17".to_string(),
                statements,
            };

            let description = {
                let d = new_policy_description.get();
                if d.is_empty() { None } else { Some(d) }
            };

            let api = api.clone();
            let app_state = app_state.clone();

            wasm_bindgen_futures::spawn_local(async move {
                match api.create_policy(policy, description).await {
                    Ok(_) => {
                        app_state.show_toast("Policy created successfully".to_string(), ToastKind::Success);
                        show_create_modal.set(false);
                        version.update(|v| *v += 1);
                    }
                    Err(e) => {
                        json_error.set(Some(e.to_string()));
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
                            <h1 class="text-2xl font-semibold text-white">"IAM Policies"</h1>
                            <button
                                on:click=open_create_modal
                                class="inline-flex items-center px-4 py-2 border border-transparent text-sm font-medium rounded-md text-white bg-strix-600 hover:bg-strix-700"
                            >
                                "Create Policy"
                            </button>
                        </div>

                        <Card>
                            <Suspense fallback=|| view! { <LoadingFallback size=LoadingSize::Small /> }>
                                {move || {
                                    policies.get().and_then(|data| {
                                        match &*data {
                                            Ok(policy_list) => Some(view! {
                                                <PolicyTable policies=policy_list.clone() />
                                            }),
                                            Err(_) => None,
                                        }
                                    })
                                }}
                            </Suspense>
                            {move || policies_error.get().map(|e| view! {
                                <div class="mt-3 rounded-md bg-red-900/40 border border-red-700 p-3 text-sm text-red-200">
                                    {format!("Failed to load policies: {}", e)}
                                </div>
                            })}
                        </Card>

                        // Built-in policies info
                        <div class="mt-8">
                            <h2 class="text-lg font-medium text-white mb-4">"Built-in Policies"</h2>
                            <Card>
                                <div class="space-y-4">
                                    <div class="flex justify-between items-center py-2 border-b border-slate-700">
                                        <div>
                                            <p class="text-white font-medium">"AdministratorAccess"</p>
                                            <p class="text-slate-400 text-sm">"Full access to all resources"</p>
                                        </div>
                                        <span class="px-2 py-1 text-xs rounded bg-blue-900/50 text-blue-300">"Built-in"</span>
                                    </div>
                                    <div class="flex justify-between items-center py-2 border-b border-slate-700">
                                        <div>
                                            <p class="text-white font-medium">"ReadOnlyAccess"</p>
                                            <p class="text-slate-400 text-sm">"Read-only access to buckets and objects"</p>
                                        </div>
                                        <span class="px-2 py-1 text-xs rounded bg-blue-900/50 text-blue-300">"Built-in"</span>
                                    </div>
                                    <div class="flex justify-between items-center py-2">
                                        <div>
                                            <p class="text-white font-medium">"ReadWriteAccess"</p>
                                            <p class="text-slate-400 text-sm">"Read and write access to objects (no admin operations)"</p>
                                        </div>
                                        <span class="px-2 py-1 text-xs rounded bg-blue-900/50 text-blue-300">"Built-in"</span>
                                    </div>
                                </div>
                            </Card>
                        </div>
                    </div>
                </main>
            </div>
            <ToastContainer />

            // Create policy modal
            <Modal open=show_create_modal title="Create Policy">
                <div class="space-y-4">
                    <div>
                        <label class="block text-sm font-medium text-slate-300">"Policy Name"</label>
                        <input
                            type="text"
                            class="mt-1 block w-full px-3 py-2 bg-slate-700 border border-slate-600 rounded-md shadow-sm text-white placeholder-slate-400 focus:outline-none focus:ring-strix-500 focus:border-strix-500 sm:text-sm"
                            placeholder="MyCustomPolicy"
                            prop:value=move || new_policy_name.get()
                            on:input=move |ev| new_policy_name.set(event_target_value(&ev))
                        />
                    </div>

                    <div>
                        <label class="block text-sm font-medium text-slate-300">"Description (optional)"</label>
                        <input
                            type="text"
                            class="mt-1 block w-full px-3 py-2 bg-slate-700 border border-slate-600 rounded-md shadow-sm text-white placeholder-slate-400 focus:outline-none focus:ring-strix-500 focus:border-strix-500 sm:text-sm"
                            placeholder="A custom policy for..."
                            prop:value=move || new_policy_description.get()
                            on:input=move |ev| new_policy_description.set(event_target_value(&ev))
                        />
                    </div>

                    <div>
                        <label class="block text-sm font-medium text-slate-300">"Policy Statements (JSON)"</label>
                        <div class="mt-2 flex flex-wrap gap-2">
                            <button
                                class="px-2 py-1 text-xs rounded bg-slate-700 text-slate-200 hover:bg-slate-600"
                                on:click=move |_| new_policy_json.set(template_backup_readonly().to_string())
                            >
                                "Template: Backup ReadOnly"
                            </button>
                            <button
                                class="px-2 py-1 text-xs rounded bg-slate-700 text-slate-200 hover:bg-slate-600"
                                on:click=move |_| new_policy_json.set(template_ci_artifacts().to_string())
                            >
                                "Template: CI Artifacts"
                            </button>
                            <button
                                class="px-2 py-1 text-xs rounded bg-slate-700 text-slate-200 hover:bg-slate-600"
                                on:click=move |_| new_policy_json.set(template_upload_only().to_string())
                            >
                                "Template: Upload Only"
                            </button>
                        </div>
                        <textarea
                            rows="12"
                            class="mt-1 block w-full px-3 py-2 bg-slate-700 border border-slate-600 rounded-md shadow-sm text-white font-mono text-sm placeholder-slate-400 focus:outline-none focus:ring-strix-500 focus:border-strix-500"
                            prop:value=move || new_policy_json.get()
                            on:input=move |ev| {
                                new_policy_json.set(event_target_value(&ev));
                                json_error.set(None);
                            }
                        />
                        <p class="mt-1 text-xs text-slate-400">
                            "Enter policy statements as a JSON array"
                        </p>
                    </div>

                    {move || json_error.get().map(|err| view! {
                        <div class="p-3 bg-red-900/50 border border-red-700 rounded-md">
                            <p class="text-sm text-red-300">{err}</p>
                        </div>
                    })}

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
                        if let Some(name) = action.strip_prefix("delete-policy:") {
                            let name = name.to_string();
                            let api = api.clone();
                            let app_state = app_state.clone();
                            wasm_bindgen_futures::spawn_local(async move {
                                match api.delete_policy(&name).await {
                                    Ok(()) => {
                                        app_state.show_toast("Policy deleted".to_string(), ToastKind::Success);
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

/// Policy table component.
#[component]
fn PolicyTable(policies: Vec<PolicyInfo>) -> impl IntoView {
    if policies.is_empty() {
        return view! {
            <div class="text-center py-12">
                <svg class="mx-auto h-12 w-12 text-slate-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z"/>
                </svg>
                <h3 class="mt-2 text-sm font-medium text-white">"No custom policies"</h3>
                <p class="mt-1 text-sm text-slate-400">"Create a custom policy to get started."</p>
            </div>
        }.into_any();
    }

    let headers = vec!["Name", "Description", "Statements", "Actions"];

    view! {
        <Table headers=headers>
            {policies.iter().map(|policy| {
                let name = policy.name.clone();
                let name_delete = policy.name.clone();
                let description = policy.description.clone().unwrap_or_else(|| "-".to_string());
                let statement_count = policy.statements.len();

                view! {
                    <TableRow>
                        <td class="px-6 py-4 whitespace-nowrap text-sm font-medium text-strix-400">
                            {name}
                        </td>
                        <td class="px-6 py-4 whitespace-nowrap text-sm text-slate-400">
                            {description}
                        </td>
                        <td class="px-6 py-4 whitespace-nowrap text-sm text-slate-400">
                            {statement_count} " statement"{if statement_count != 1 { "s" } else { "" }}
                        </td>
                        <td class="px-6 py-4 whitespace-nowrap text-sm text-slate-400">
                            <DeletePolicyButton name=name_delete />
                        </td>
                    </TableRow>
                }
            }).collect_view()}
        </Table>
    }.into_any()
}

/// Delete policy button component.
#[component]
fn DeletePolicyButton(name: String) -> impl IntoView {
    let app_state = expect_context::<AppState>();
    let name_clone = name.clone();

    let on_delete = move |_| {
        let name = name_clone.clone();
        app_state.confirm.show(
            "Delete Policy",
            format!("Delete policy '{}'? This cannot be undone.", name),
            format!("delete-policy:{}", name),
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

fn default_policy_json() -> String {
    r#"[
  {
    "Effect": "Allow",
    "Action": ["s3:GetObject", "s3:ListBucket"],
    "Resource": ["arn:aws:s3:::my-bucket/*"]
  }
]"#.to_string()
}

fn template_backup_readonly() -> &'static str {
    r#"[
  {
    "Effect": "Allow",
    "Action": ["s3:ListBucket", "s3:GetObject", "s3:GetObjectVersion"],
    "Resource": ["arn:aws:s3:::client-backups", "arn:aws:s3:::client-backups/*"]
  }
]"#
}

fn template_ci_artifacts() -> &'static str {
    r#"[
  {
    "Effect": "Allow",
    "Action": ["s3:ListBucket", "s3:PutObject", "s3:GetObject", "s3:DeleteObject"],
    "Resource": ["arn:aws:s3:::ci-artifacts", "arn:aws:s3:::ci-artifacts/*"]
  }
]"#
}

fn template_upload_only() -> &'static str {
    r#"[
  {
    "Effect": "Allow",
    "Action": ["s3:PutObject"],
    "Resource": ["arn:aws:s3:::client-ingest/*"]
  }
]"#
}
