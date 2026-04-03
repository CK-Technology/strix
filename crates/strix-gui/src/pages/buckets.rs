//! Buckets pages.

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{File, RequestInit, RequestMode};

use crate::api::{BucketUsage, ListObjectsResponse};
use crate::components::{Card, ConfirmModal, Header, LoadingFallback, LoadingSize, Modal, Sidebar, Table, TableRow, ToastContainer};
use crate::state::{AppState, ToastKind};
use crate::tenant::prefixed_bucket_name;

/// Upload status for a single file.
#[derive(Clone, PartialEq)]
pub enum UploadStatus {
    Pending,
    Uploading,
    Complete,
    Failed(String),
}

/// Single file upload entry for display purposes.
#[derive(Clone)]
pub struct UploadEntry {
    pub name: String,
    pub size: u64,
    pub status: UploadStatus,
}

/// Upload queue state for tracking multiple file uploads.
#[derive(Clone, Default)]
pub struct UploadQueue {
    pub entries: RwSignal<Vec<UploadEntry>>,
    pub is_uploading: RwSignal<bool>,
    pub show_panel: RwSignal<bool>,
    pub completed_count: RwSignal<u32>,
    pub failed_count: RwSignal<u32>,
}

#[derive(Clone)]
struct SelectedObject {
    key: String,
    size: u64,
}

/// Upload a file to a pre-signed URL.
async fn upload_file_to_url(url: &str, file: &File) -> Result<(), String> {
    let window = web_sys::window().ok_or("No window")?;

    // Create request options
    let opts = RequestInit::new();
    opts.set_method("PUT");
    opts.set_mode(RequestMode::Cors);
    opts.set_body(file);

    // Create the request
    let request = web_sys::Request::new_with_str_and_init(url, &opts)
        .map_err(|e| format!("Failed to create request: {:?}", e))?;

    // Send the request
    let promise = window.fetch_with_request(&request);
    let response = JsFuture::from(promise)
        .await
        .map_err(|e| format!("Fetch failed: {:?}", e))?;

    let response: web_sys::Response = response.dyn_into().map_err(|_| "Invalid response")?;

    if response.ok() {
        Ok(())
    } else {
        Err(format!("Upload failed: {}", response.status()))
    }
}

/// Buckets list page.
#[component]
pub fn Buckets() -> impl IntoView {
    let app_state = expect_context::<AppState>();
    let api = app_state.api.clone();

    // Version signal to trigger refetch after mutations
    let version = RwSignal::new(0u32);

    // Search/filter state
    let search_query = RwSignal::new(String::new());
    let prefix_filter = RwSignal::new(Option::<String>::None);
    let buckets_error = RwSignal::new(Option::<String>::None);

    let buckets = {
        let api = api.clone();
        let app_state = app_state.clone();
        LocalResource::new(move || {
            let api = api.clone();
            let app_state = app_state.clone();
            let _v = version.get(); // Depend on version to trigger refetch
            async move {
                match api.get_storage_usage().await {
                    Ok(u) => {
                        buckets_error.set(None);
                        Ok(u.buckets)
                    }
                    Err(e) => {
                        app_state.handle_error(&e);
                        let msg = e.to_string();
                        buckets_error.set(Some(msg.clone()));
                        Err(msg)
                    }
                }
            }
        })
    };

    let show_create_modal = RwSignal::new(false);

    // Extract unique prefixes from bucket names for prefix chips
    let prefixes = move || {
        buckets.get().and_then(|data| {
            match &*data {
                Ok(bucket_list) => Some(bucket_list.clone()),
                Err(_) => None,
            }.map(|bucket_list| {
                let mut prefixes: Vec<String> = bucket_list
                    .iter()
                    .filter_map(|b| {
                        // Extract prefix (first part before hyphen or underscore)
                        b.name.split(|c| c == '-' || c == '_').next()
                            .filter(|p| p.len() >= 2 && *p != b.name.as_str())
                            .map(|p| p.to_string())
                    })
                    .collect();
                prefixes.sort();
                prefixes.dedup();
                prefixes.truncate(5); // Show max 5 prefix chips
                prefixes
            })
        }).unwrap_or_default()
    };

    // Filter buckets based on search and prefix
    let filtered_buckets = move || {
        buckets.get().and_then(|data| {
            match &*data {
                Ok(bucket_list) => Some(bucket_list.clone()),
                Err(_) => None,
            }.map(|bucket_list| {
                let query = search_query.get().to_lowercase();
                let prefix = prefix_filter.get();

                bucket_list.into_iter().filter(|b| {
                    let name = b.name.to_lowercase();
                    let matches_search = query.is_empty() || name.contains(&query);
                    let matches_prefix = prefix.as_ref().map_or(true, |p| name.starts_with(&p.to_lowercase()));
                    matches_search && matches_prefix
                }).collect::<Vec<_>>()
            })
        })
    };

    view! {
        <div class="flex flex-col min-h-screen">
            <Header />
            <div class="flex flex-1">
                <Sidebar />
                <main class="flex-1 p-8 bg-slate-900">
                    <div class="max-w-7xl mx-auto">
                        <div class="flex justify-between items-center mb-6">
                            <h1 class="text-2xl font-semibold text-white">"Buckets"</h1>
                            <button
                                class="inline-flex items-center px-4 py-2 border border-transparent text-sm font-medium rounded-md shadow-sm text-white bg-strix-600 hover:bg-strix-700 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-offset-slate-900 focus:ring-strix-500"
                                on:click=move |_| show_create_modal.set(true)
                            >
                                <svg class="w-5 h-5 mr-2" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 6v6m0 0v6m0-6h6m-6 0H6"/>
                                </svg>
                                "Create Bucket"
                            </button>
                        </div>

                        // Search and filter bar
                        <div class="mb-6 flex flex-wrap items-center gap-4">
                            // Search input
                            <div class="relative flex-1 min-w-[200px] max-w-md">
                                <div class="absolute inset-y-0 left-0 pl-3 flex items-center pointer-events-none">
                                    <svg class="h-5 w-5 text-slate-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"/>
                                    </svg>
                                </div>
                                <input
                                    type="text"
                                    placeholder="Search buckets..."
                                    class="block w-full pl-10 pr-3 py-2 border border-slate-600 rounded-md bg-slate-800 text-white placeholder-slate-400 focus:outline-none focus:ring-1 focus:ring-strix-500 focus:border-strix-500 sm:text-sm"
                                    prop:value=move || search_query.get()
                                    on:input=move |ev| search_query.set(event_target_value(&ev))
                                />
                            </div>

                            // Prefix filter chips
                            <div class="flex flex-wrap items-center gap-2">
                                {move || {
                                    let current_prefix = prefix_filter.get();
                                    let prefix_list = prefixes();

                                    if prefix_list.is_empty() {
                                        return None;
                                    }

                                    Some(view! {
                                        <>
                                            <span class="text-xs text-slate-400">"Filter:"</span>
                                            // "All" chip
                                            <button
                                                class=move || {
                                                    if current_prefix.is_none() {
                                                        "px-3 py-1 text-xs rounded-full bg-strix-600 text-white"
                                                    } else {
                                                        "px-3 py-1 text-xs rounded-full bg-slate-700 text-slate-300 hover:bg-slate-600"
                                                    }
                                                }
                                                on:click=move |_| prefix_filter.set(None)
                                            >
                                                "All"
                                            </button>
                                            // Prefix chips
                                            {prefix_list.into_iter().map(|p| {
                                                let p_clone = p.clone();
                                                let p_display = p.clone();
                                                let p_check = p.clone();
                                                view! {
                                                    <button
                                                        class=move || {
                                                            if prefix_filter.get().as_ref() == Some(&p_check) {
                                                                "px-3 py-1 text-xs rounded-full bg-strix-600 text-white"
                                                            } else {
                                                                "px-3 py-1 text-xs rounded-full bg-slate-700 text-slate-300 hover:bg-slate-600"
                                                            }
                                                        }
                                                        on:click=move |_| prefix_filter.set(Some(p_clone.clone()))
                                                    >
                                                        {p_display}
                                                    </button>
                                                }
                                            }).collect_view()}
                                        </>
                                    })
                                }}
                            </div>
                        </div>

                        {move || buckets_error.get().map(|e| view! {
                            <div class="mb-4 rounded-md bg-red-900/40 border border-red-700 p-3 text-sm text-red-200">
                                {format!("Bucket list unavailable: {}", e)}
                            </div>
                        })}

                        <Card>
                            <Suspense fallback=|| view! { <LoadingFallback size=LoadingSize::Small /> }>
                                {move || {
                                    filtered_buckets().map(|bucket_list| {
                                        let total_count = buckets.get()
                                            .and_then(|d| match &*d {
                                                Ok(l) => Some(l.len()),
                                                Err(_) => None,
                                            })
                                            .unwrap_or(0);
                                        let filtered_count = bucket_list.len();
                                        let is_filtered = !search_query.get().is_empty() || prefix_filter.get().is_some();

                                        view! {
                                            <>
                                                {is_filtered.then(|| view! {
                                                    <div class="mb-4 text-sm text-slate-400">
                                                        "Showing " {filtered_count} " of " {total_count} " buckets"
                                                    </div>
                                                })}
                                                <BucketTable buckets=bucket_list />
                                            </>
                                        }
                                    })
                                }}
                            </Suspense>
                        </Card>
                    </div>
                </main>
            </div>
            <ToastContainer />

            // Create bucket modal
            <Modal
                open=show_create_modal
                title="Create Bucket"
            >
                <CreateBucketForm show_modal=show_create_modal on_created=move || version.update(|v| *v += 1) />
            </Modal>

            // Confirm delete modal
            <ConfirmModal
                state=app_state.confirm.clone()
                on_confirm={
                    let api = api.clone();
                    let app_state = app_state.clone();
                    move |action: String| {
                        if let Some(name) = action.strip_prefix("delete-bucket:") {
                            let name = name.to_string();
                            let api = api.clone();
                            let app_state = app_state.clone();
                            wasm_bindgen_futures::spawn_local(async move {
                                match api.delete_bucket(&name).await {
                                    Ok(()) => {
                                        app_state.show_toast("Bucket deleted".to_string(), ToastKind::Success);
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

/// Create bucket form with feature toggles.
#[component]
fn CreateBucketForm(
    show_modal: RwSignal<bool>,
    on_created: impl Fn() + Clone + Send + Sync + 'static,
) -> impl IntoView {
    let app_state = expect_context::<AppState>();
    let api = app_state.api.clone();

    // Form state
    let bucket_name = RwSignal::new(String::new());
    let versioning = RwSignal::new(false);
    let object_locking = RwSignal::new(false);
    let quota_enabled = RwSignal::new(false);
    let quota_size = RwSignal::new(String::new());
    let creating = RwSignal::new(false);
    let error = RwSignal::new(Option::<String>::None);
    let tenant_slug = RwSignal::new(String::new());
    let tenant_list_error = RwSignal::new(Option::<String>::None);
    let tenants_resource = {
        let api = api.clone();
        let app_state = app_state.clone();
        LocalResource::new(move || {
            let api = api.clone();
            let app_state = app_state.clone();
            async move {
                match api.list_tenants().await {
                    Ok(r) => {
                        tenant_list_error.set(None);
                        Ok(r.items)
                    }
                    Err(e) => {
                        app_state.handle_error(&e);
                        let msg = e.to_string();
                        tenant_list_error.set(Some(msg.clone()));
                        Err(msg)
                    }
                }
            }
        })
    };

    let on_create = {
        let api = api.clone();
        let on_created = on_created.clone();
        move |_| {
            let name = bucket_name.get();
            if name.is_empty() {
                error.set(Some("Bucket name is required".to_string()));
                return;
            }

            let resolved_name = if tenant_slug.get().trim().is_empty() {
                name.clone()
            } else {
                prefixed_bucket_name(&tenant_slug.get(), &name)
            };

            creating.set(true);
            error.set(None);

            let api = api.clone();
            let on_created = on_created.clone();
            let enable_versioning = versioning.get();
            let enable_object_locking = object_locking.get();
            // Note: Quota is not yet implemented in the backend
            let _quota_enabled = quota_enabled.get();
            let _quota_size = quota_size.get();

            wasm_bindgen_futures::spawn_local(async move {
                match api.create_bucket_with_options(
                    &resolved_name,
                    if tenant_slug.get().is_empty() {
                        None
                    } else {
                        Some(tenant_slug.get())
                    },
                    enable_versioning,
                    enable_object_locking,
                ).await {
                    Ok(_) => {
                        show_modal.set(false);
                        bucket_name.set(String::new());
                        tenant_slug.set(String::new());
                        on_created();
                    }
                    Err(e) => {
                        error.set(Some(e.to_string()));
                    }
                }
                creating.set(false);
            });
        }
    };

    view! {
        <div class="space-y-6">
            // Error message
            {move || error.get().map(|e| view! {
                <div class="rounded-md bg-red-900/50 border border-red-700 p-4">
                    <p class="text-sm text-red-300">{e}</p>
                </div>
            })}

            // Bucket Name
            <div>
                <label for="bucket-name" class="block text-sm font-medium text-slate-300">
                    "Bucket Name"
                </label>
                <input
                    id="bucket-name"
                    type="text"
                    class="mt-1 block w-full px-3 py-2 bg-slate-700 border border-slate-600 rounded-md shadow-sm text-white placeholder-slate-400 focus:outline-none focus:ring-strix-500 focus:border-strix-500 sm:text-sm"
                    placeholder="my-bucket"
                    prop:value=move || bucket_name.get()
                    on:input=move |ev| bucket_name.set(event_target_value(&ev))
                />
                <p class="mt-1 text-xs text-slate-400">
                    "Bucket names must be lowercase and can contain letters, numbers, and hyphens."
                </p>
            </div>

            <div>
                <label class="block text-sm font-medium text-slate-300">"Tenant Prefix (optional)"</label>
                <select
                    class="mt-1 block w-full px-3 py-2 bg-slate-700 border border-slate-600 rounded-md text-white"
                    on:change=move |ev| tenant_slug.set(event_target_value(&ev))
                >
                    <option value="">"No tenant prefix"</option>
                    {move || tenants_resource.get().map(|data| {
                        match &*data {
                            Ok(list) => list.clone().into_iter().map(|t| {
                                let slug = t.slug.clone();
                                let label = format!("{} ({})", t.name, t.slug);
                                view! { <option value=slug>{label}</option> }
                            }).collect_view(),
                            Err(_) => Vec::new(),
                        }
                    })}
                </select>
                <p class="mt-1 text-xs text-slate-400">"Resulting bucket: <tenant>-<bucket-name> when selected."</p>
                {move || tenant_list_error.get().map(|e| view! {
                    <p class="mt-1 text-xs text-red-300">{format!("Could not load tenants: {}", e)}</p>
                })}
            </div>

            // Feature Toggles Section
            <div class="border-t border-slate-700 pt-4">
                <h4 class="text-sm font-medium text-slate-300 mb-4">"Features"</h4>

                // Versioning Toggle
                <FeatureToggle
                    label="Versioning"
                    description="Keep multiple versions of objects in the bucket"
                    checked=versioning
                />

                // Object Locking Toggle
                <FeatureToggle
                    label="Object Locking"
                    description="Enable WORM (Write Once Read Many) protection for compliance"
                    checked=object_locking
                    warning="This cannot be disabled after bucket creation"
                />

                // Quota Toggle
                <div class="flex items-start justify-between py-3 border-b border-slate-700/50">
                    <div class="flex-1">
                        <label class="text-sm font-medium text-white">"Quota"</label>
                        <p class="text-xs text-slate-400 mt-1">"Limit the maximum size of the bucket"</p>
                    </div>
                    <div class="flex items-center">
                        <button
                            type="button"
                            class=move || format!(
                                "relative inline-flex h-6 w-11 flex-shrink-0 cursor-pointer rounded-full border-2 border-transparent transition-colors duration-200 ease-in-out focus:outline-none focus:ring-2 focus:ring-strix-500 focus:ring-offset-2 focus:ring-offset-navy-900 {}",
                                if quota_enabled.get() { "bg-strix-600" } else { "bg-slate-600" }
                            )
                            on:click=move |_| quota_enabled.update(|v| *v = !*v)
                        >
                            <span
                                class=move || format!(
                                    "pointer-events-none inline-block h-5 w-5 transform rounded-full bg-white shadow ring-0 transition duration-200 ease-in-out {}",
                                    if quota_enabled.get() { "translate-x-5" } else { "translate-x-0" }
                                )
                            />
                        </button>
                    </div>
                </div>

                // Quota Size Input (shown when quota is enabled)
                {move || {
                    if quota_enabled.get() {
                        Some(view! {
                            <div class="py-3">
                                <label for="quota-size" class="block text-sm font-medium text-slate-300">
                                    "Quota Size"
                                </label>
                                <div class="mt-1 flex rounded-md shadow-sm">
                                    <input
                                        id="quota-size"
                                        type="text"
                                        class="flex-1 block w-full px-3 py-2 bg-slate-700 border border-slate-600 rounded-l-md shadow-sm text-white placeholder-slate-400 focus:outline-none focus:ring-strix-500 focus:border-strix-500 sm:text-sm"
                                        placeholder="100"
                                        prop:value=move || quota_size.get()
                                        on:input=move |ev| quota_size.set(event_target_value(&ev))
                                    />
                                    <span class="inline-flex items-center px-3 bg-slate-600 border border-l-0 border-slate-600 rounded-r-md text-slate-300 sm:text-sm">
                                        "GB"
                                    </span>
                                </div>
                            </div>
                        })
                    } else {
                        None
                    }
                }}
            </div>

            // Actions
            <div class="flex justify-end space-x-3 pt-4 border-t border-slate-700">
                <button
                    type="button"
                    class="px-4 py-2 text-sm font-medium text-slate-300 bg-slate-700 border border-slate-600 rounded-md shadow-sm hover:bg-slate-600"
                    on:click=move |_| show_modal.set(false)
                >
                    "Cancel"
                </button>
                <button
                    type="button"
                    class="px-4 py-2 text-sm font-medium text-white bg-strix-600 border border-transparent rounded-md shadow-sm hover:bg-strix-700 disabled:opacity-50"
                    disabled=move || creating.get()
                    on:click=on_create
                >
                    {move || if creating.get() { "Creating..." } else { "Create Bucket" }}
                </button>
            </div>
        </div>
    }
}

/// Feature toggle component.
#[component]
fn FeatureToggle(
    label: &'static str,
    description: &'static str,
    checked: RwSignal<bool>,
    #[prop(optional)]
    warning: Option<&'static str>,
) -> impl IntoView {
    view! {
        <div class="flex items-start justify-between py-3 border-b border-slate-700/50">
            <div class="flex-1">
                <label class="text-sm font-medium text-white">{label}</label>
                <p class="text-xs text-slate-400 mt-1">{description}</p>
                {warning.map(|w| view! {
                    <p class="text-xs text-yellow-400 mt-1">
                        <svg class="w-3 h-3 inline mr-1" fill="currentColor" viewBox="0 0 20 20">
                            <path fill-rule="evenodd" d="M8.257 3.099c.765-1.36 2.722-1.36 3.486 0l5.58 9.92c.75 1.334-.213 2.98-1.742 2.98H4.42c-1.53 0-2.493-1.646-1.743-2.98l5.58-9.92zM11 13a1 1 0 11-2 0 1 1 0 012 0zm-1-8a1 1 0 00-1 1v3a1 1 0 002 0V6a1 1 0 00-1-1z" clip-rule="evenodd"/>
                        </svg>
                        {w}
                    </p>
                })}
            </div>
            <div class="flex items-center ml-4">
                <button
                    type="button"
                    class=move || format!(
                        "relative inline-flex h-6 w-11 flex-shrink-0 cursor-pointer rounded-full border-2 border-transparent transition-colors duration-200 ease-in-out focus:outline-none focus:ring-2 focus:ring-strix-500 focus:ring-offset-2 focus:ring-offset-navy-900 {}",
                        if checked.get() { "bg-strix-600" } else { "bg-slate-600" }
                    )
                    on:click=move |_| checked.update(|v| *v = !*v)
                >
                    <span
                        class=move || format!(
                            "pointer-events-none inline-block h-5 w-5 transform rounded-full bg-white shadow ring-0 transition duration-200 ease-in-out {}",
                            if checked.get() { "translate-x-5" } else { "translate-x-0" }
                        )
                    />
                </button>
            </div>
        </div>
    }
}

/// Bucket table component.
#[component]
fn BucketTable(buckets: Vec<BucketUsage>) -> impl IntoView {
    if buckets.is_empty() {
        return view! {
            <div class="text-center py-12">
                <svg class="mx-auto h-12 w-12 text-slate-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z"/>
                </svg>
                <h3 class="mt-2 text-sm font-medium text-white">"No buckets"</h3>
                <p class="mt-1 text-sm text-slate-400">"Get started by creating a new bucket."</p>
            </div>
        }.into_any();
    }

    let headers = vec!["Name", "Objects", "Size", "Created", "Actions"];

    view! {
        <Table headers=headers>
            {buckets.iter().map(|bucket| {
                let name = bucket.name.clone();
                let bucket_route = format!("/buckets/{}", bucket.name);
                let name_delete = bucket.name.clone();
                let objects = bucket.object_count;
                let size = format_size(bucket.total_size);
                let created = format_date(&bucket.created_at);
                view! {
                    <TableRow>
                        <td class="px-6 py-4 whitespace-nowrap text-sm font-medium">
                            <A href=bucket_route attr:class="text-strix-400 hover:text-strix-300">
                                {name}
                            </A>
                        </td>
                        <td class="px-6 py-4 whitespace-nowrap text-sm text-slate-400">
                            {objects}
                        </td>
                        <td class="px-6 py-4 whitespace-nowrap text-sm text-slate-400">
                            {size}
                        </td>
                        <td class="px-6 py-4 whitespace-nowrap text-sm text-slate-400">
                            {created}
                        </td>
                        <td class="px-6 py-4 whitespace-nowrap text-sm text-slate-400">
                            <DeleteBucketButton name=name_delete />
                        </td>
                    </TableRow>
                }
            }).collect_view()}
        </Table>
    }.into_any()
}

/// Delete bucket button component.
#[component]
fn DeleteBucketButton(name: String) -> impl IntoView {
    let app_state = expect_context::<AppState>();
    let name_clone = name.clone();

    let on_delete = move |_| {
        let name = name_clone.clone();
        app_state.confirm.show(
            "Delete Bucket",
            format!("Delete bucket '{}'? This cannot be undone.", name),
            format!("delete-bucket:{}", name),
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

/// Bucket detail page with object browser.
#[component]
pub fn BucketDetail() -> impl IntoView {
    let params = use_params_map();
    let bucket = RwSignal::new(params.read().get("name").unwrap_or_default());

    let app_state = expect_context::<AppState>();
    let api = app_state.api.clone();
    let current_prefix = RwSignal::new(String::new());
    let selected_objects = RwSignal::new(Vec::<String>::new());
    let preview_open = RwSignal::new(false);
    let preview_title = RwSignal::new(String::new());
    let preview_content = RwSignal::new(String::new());
    let dragging = RwSignal::new(false);
    let app_state_for_preview = app_state.clone();
    let api_for_confirm = api.clone();
    let app_state_for_confirm = app_state.clone();

    // Upload queue for per-file progress tracking
    let upload_queue = UploadQueue::default();

    // Version signal to trigger refetch after mutations
    let version = RwSignal::new(0u32);

    // File input ref
    let file_input_ref = NodeRef::<leptos::html::Input>::new();

    // Fetch objects
    let objects = {
        let api = api.clone();
        let app_state = app_state.clone();
        LocalResource::new(move || {
            let api = api.clone();
            let app_state = app_state.clone();
            let bucket = bucket.get();
            let prefix = current_prefix.get();
            let _v = version.get(); // Depend on version to trigger refetch
            async move {
                match api.list_objects(&bucket, Some(&prefix), Some("/")).await {
                    Ok(list) => Ok(list),
                    Err(e) => {
                        app_state.handle_error(&e);
                        Err(e.to_string())
                    }
                }
            }
        })
    };

    // Go up one level
    let go_up = move |_| {
        let prefix = current_prefix.get();
        if prefix.is_empty() {
            return;
        }
        // Remove the last path component
        let parts: Vec<&str> = prefix.trim_end_matches('/').split('/').collect();
        if parts.len() > 1 {
            let new_prefix = parts[..parts.len() - 1].join("/") + "/";
            current_prefix.set(new_prefix);
        } else {
            current_prefix.set(String::new());
        }
        selected_objects.set(Vec::new());
    };

    // Delete selected objects
    let delete_selected = {
        let app_state = app_state.clone();
        move |_| {
            let selected = selected_objects.get();
            if selected.is_empty() {
                return;
            }

            // Store the count in the action ID for the confirm handler
            let count = selected.len();
            app_state.confirm.show(
                "Delete Objects",
                format!("Delete {} selected object(s)? This cannot be undone.", count),
                format!("delete-objects:{}", count),
            );
        }
    };

    // Trigger file input click
    let trigger_upload = move |_| {
        if let Some(input) = file_input_ref.get() {
            input.click();
        }
    };

    // Handle file selection and upload
    let handle_file_change = {
        let api = api.clone();
        let app_state = app_state.clone();
        let upload_queue = upload_queue.clone();
        move |ev: web_sys::Event| {
            use wasm_bindgen::JsCast;
            use web_sys::{FileList, HtmlInputElement};

            let Some(target) = ev.target() else {
                return;
            };
            let Ok(input) = target.dyn_into::<HtmlInputElement>() else {
                return;
            };
            let files: Option<FileList> = input.files();

            if let Some(files) = files {
                let file_count = files.length();
                if file_count == 0 {
                    return;
                }

                let prefix = current_prefix.get();
                let api = api.clone();
                let bucket = bucket.get();
                let app_state = app_state.clone();
                let upload_queue = upload_queue.clone();

                // Collect files and create initial entries
                let mut file_list: Vec<(File, String)> = Vec::new();
                let mut entries: Vec<UploadEntry> = Vec::new();

                for i in 0..file_count {
                    if let Some(file) = files.get(i) {
                        let file_name = file.name();
                        let file_size = file.size() as u64;
                        let key = if prefix.is_empty() {
                            file_name.clone()
                        } else {
                            format!("{}{}", prefix, file_name)
                        };

                        file_list.push((file, key.clone()));
                        entries.push(UploadEntry {
                            name: file_name,
                            size: file_size,
                            status: UploadStatus::Pending,
                        });
                    }
                }

                // Update the display
                upload_queue.entries.set(entries);
                upload_queue.show_panel.set(true);
                upload_queue.is_uploading.set(true);
                upload_queue.completed_count.set(0);
                upload_queue.failed_count.set(0);

                wasm_bindgen_futures::spawn_local(async move {
                    let total = file_list.len();
                    let mut uploaded = 0;
                    let mut failed = 0;

                    for (idx, (file, key)) in file_list.into_iter().enumerate() {
                        // Update status to uploading
                        upload_queue.entries.update(|entries| {
                            if let Some(entry) = entries.get_mut(idx) {
                                entry.status = UploadStatus::Uploading;
                            }
                        });

                        // Get presigned URL and upload
                        let result = match api.get_upload_url(&bucket, &key).await {
                            Ok(url) => upload_file_to_url(&url, &file).await,
                            Err(e) => Err(e.to_string()),
                        };

                        match result {
                            Ok(()) => {
                                uploaded += 1;
                                upload_queue.completed_count.set(uploaded);
                                upload_queue.entries.update(|entries| {
                                    if let Some(entry) = entries.get_mut(idx) {
                                        entry.status = UploadStatus::Complete;
                                    }
                                });
                            }
                            Err(e) => {
                                failed += 1;
                                upload_queue.failed_count.set(failed);
                                upload_queue.entries.update(|entries| {
                                    if let Some(entry) = entries.get_mut(idx) {
                                        entry.status = UploadStatus::Failed(e);
                                    }
                                });
                            }
                        }
                    }

                    upload_queue.is_uploading.set(false);

                    // Refresh the list if any files uploaded
                    if uploaded > 0 {
                        app_state.show_toast(format!("Uploaded {} of {} file(s)", uploaded, total), ToastKind::Success);
                        version.update(|v| *v += 1);
                    } else if failed > 0 {
                        app_state.show_toast(format!("All {} uploads failed", failed), ToastKind::Error);
                    }
                });
            }

            // Clear the input so the same file can be selected again
            input.set_value("");
        }
    };

    // Handle file drop
    let handle_drop = {
        let api = api.clone();
        let app_state = app_state.clone();
        let upload_queue = upload_queue.clone();
        move |ev: web_sys::DragEvent| {
            ev.prevent_default();
            dragging.set(false);

            let data_transfer = match ev.data_transfer() {
                Some(dt) => dt,
                None => return,
            };

            let files = match data_transfer.files() {
                Some(f) => f,
                None => return,
            };

            let file_count = files.length();
            if file_count == 0 {
                return;
            }

            let prefix = current_prefix.get();
            let api = api.clone();
            let bucket = bucket.get();
            let app_state = app_state.clone();
            let upload_queue = upload_queue.clone();

            // Collect files and create initial entries
            let mut file_list: Vec<(File, String)> = Vec::new();
            let mut entries: Vec<UploadEntry> = Vec::new();

            for i in 0..file_count {
                if let Some(file) = files.get(i) {
                    let file_name = file.name();
                    let file_size = file.size() as u64;
                    let key = if prefix.is_empty() {
                        file_name.clone()
                    } else {
                        format!("{}{}", prefix, file_name)
                    };

                    file_list.push((file, key.clone()));
                    entries.push(UploadEntry {
                        name: file_name,
                        size: file_size,
                        status: UploadStatus::Pending,
                    });
                }
            }

            // Update the display
            upload_queue.entries.set(entries);
            upload_queue.show_panel.set(true);
            upload_queue.is_uploading.set(true);
            upload_queue.completed_count.set(0);
            upload_queue.failed_count.set(0);

            wasm_bindgen_futures::spawn_local(async move {
                let total = file_list.len();
                let mut uploaded = 0;
                let mut failed = 0;

                for (idx, (file, key)) in file_list.into_iter().enumerate() {
                    // Update status to uploading
                    upload_queue.entries.update(|entries| {
                        if let Some(entry) = entries.get_mut(idx) {
                            entry.status = UploadStatus::Uploading;
                        }
                    });

                    // Get presigned URL and upload
                    let result = match api.get_upload_url(&bucket, &key).await {
                        Ok(url) => upload_file_to_url(&url, &file).await,
                        Err(e) => Err(e.to_string()),
                    };

                    match result {
                        Ok(()) => {
                            uploaded += 1;
                            upload_queue.completed_count.set(uploaded);
                            upload_queue.entries.update(|entries| {
                                if let Some(entry) = entries.get_mut(idx) {
                                    entry.status = UploadStatus::Complete;
                                }
                            });
                        }
                        Err(e) => {
                            failed += 1;
                            upload_queue.failed_count.set(failed);
                            upload_queue.entries.update(|entries| {
                                if let Some(entry) = entries.get_mut(idx) {
                                    entry.status = UploadStatus::Failed(e);
                                }
                            });
                        }
                    }
                }

                upload_queue.is_uploading.set(false);

                // Refresh the list if any files uploaded
                if uploaded > 0 {
                    app_state.show_toast(format!("Uploaded {} of {} file(s)", uploaded, total), ToastKind::Success);
                    version.update(|v| *v += 1);
                } else if failed > 0 {
                    app_state.show_toast(format!("All {} uploads failed", failed), ToastKind::Error);
                }
            });
        }
    };

    // Handle drag over (prevent default to allow drop)
    let handle_dragover = move |ev: web_sys::DragEvent| {
        ev.prevent_default();
        dragging.set(true);
    };

    // Handle drag leave
    let handle_dragleave = move |ev: web_sys::DragEvent| {
        ev.prevent_default();
        // Only set dragging to false if we're leaving the drop zone itself
        // (not just moving between child elements)
        if let Some(target) = ev.target() {
            if let Some(related) = ev.related_target() {
                use wasm_bindgen::JsCast;
                if let Some(target_el) = target.dyn_ref::<web_sys::Element>() {
                    if let Some(related_el) = related.dyn_ref::<web_sys::Element>() {
                        if target_el.contains(Some(related_el)) {
                            return; // Moving to a child element, don't change state
                        }
                    }
                }
            }
        }
        dragging.set(false);
    };

    view! {
        <div class="flex flex-col min-h-screen">
            <Header />
            <div class="flex flex-1">
                <Sidebar />
                <main class="flex-1 p-8 bg-slate-900">
                    <div class="max-w-7xl mx-auto">
                        // Breadcrumb navigation
                        <div class="flex items-center mb-6">
                            <A href="/buckets" attr:class="text-strix-400 hover:text-strix-300">
                                "Buckets"
                            </A>
                            <svg class="w-5 h-5 mx-2 text-slate-500" fill="currentColor" viewBox="0 0 20 20">
                                <path fill-rule="evenodd" d="M7.293 14.707a1 1 0 010-1.414L10.586 10 7.293 6.707a1 1 0 011.414-1.414l4 4a1 1 0 010 1.414l-4 4a1 1 0 01-1.414 0z" clip-rule="evenodd"/>
                            </svg>
                            <span class="text-white font-medium">{move || bucket.get()}</span>

                            {move || {
                                let prefix = current_prefix.get();
                                if prefix.is_empty() {
                                    None
                                } else {
                                    Some(view! {
                                        <span class="ml-2 text-slate-400 text-sm">{format!("/{}", prefix)}</span>
                                    })
                                }
                            }}
                        </div>

                        // Hidden file input for uploads
                        <input
                            type="file"
                            multiple=true
                            style="display: none"
                            node_ref=file_input_ref
                            on:change=handle_file_change
                        />

                        // Toolbar
                        <div class="flex justify-between items-center mb-4">
            <div class="flex items-center space-x-2">
                                <button
                                    class="inline-flex items-center px-3 py-2 border border-slate-600 shadow-sm text-sm font-medium rounded-md text-slate-300 bg-slate-700 hover:bg-slate-600 disabled:opacity-50"
                                    disabled=move || current_prefix.get().is_empty()
                                    on:click=go_up
                                >
                                    <svg class="w-4 h-4 mr-1" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 19l-7-7m0 0l7-7m-7 7h18"/>
                                    </svg>
                                    "Up"
                                </button>

                                <button
                                    class="inline-flex items-center px-3 py-2 border border-transparent text-sm font-medium rounded-md text-white bg-strix-600 hover:bg-strix-700 disabled:opacity-50"
                                    disabled=move || upload_queue.is_uploading.get()
                                    on:click=trigger_upload
                                >
                                    <svg class="w-4 h-4 mr-1" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12"/>
                                    </svg>
                                    {move || if upload_queue.is_uploading.get() { "Uploading..." } else { "Upload" }}
                                </button>

                                <button
                                    class="inline-flex items-center px-3 py-2 border border-transparent text-sm font-medium rounded-md text-white bg-red-600 hover:bg-red-700 disabled:opacity-50"
                                    disabled=move || selected_objects.get().is_empty()
                                    on:click=delete_selected
                                >
                                    <svg class="w-4 h-4 mr-1" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"/>
                                    </svg>
                                    "Delete Selected"
                                </button>

                                <button
                                    class="inline-flex items-center px-3 py-2 border border-slate-600 text-sm font-medium rounded-md text-slate-200 bg-slate-700 hover:bg-slate-600 disabled:opacity-50"
                                    disabled=move || selected_objects.get().is_empty()
                                    on:click={
                                        let api = api.clone();
                                        let app_state = app_state.clone();
                                        move |_| {
                                            let selected = selected_objects.get();
                                            if selected.is_empty() {
                                                return;
                                            }
                                            let bucket = bucket.get();
                                            let api = api.clone();
                                            let app_state = app_state.clone();
                                            wasm_bindgen_futures::spawn_local(async move {
                                                let mut lines = Vec::new();
                                                for key in selected {
                                                    if let Ok(p) = api.presign_url(&bucket, &key, "GET", 3600).await {
                                                        lines.push(format!(
                                                            "aws s3 cp '{}' ./ --endpoint-url http://localhost:9000 --profile strix",
                                                            p.url
                                                        ));
                                                    }
                                                }
                                                let out = lines.join("\n");
                                                if let Some(window) = web_sys::window() {
                                                    let clipboard = window.navigator().clipboard();
                                                    let _ = clipboard.write_text(&out);
                                                }
                                                app_state.show_toast("Copied CLI snippets for selected objects".to_string(), ToastKind::Info);
                                            });
                                        }
                                    }
                                >
                                    "Copy CLI Snippet"
                                </button>
                            </div>

                            <div class="flex items-center gap-3">
                                // Upload progress summary
                                {move || {
                                    let entries = upload_queue.entries.get();
                                    let total = entries.len();
                                    let complete = upload_queue.completed_count.get() as usize;
                                    let failed = upload_queue.failed_count.get() as usize;

                                    if total > 0 {
                                        Some(view! {
                                            <button
                                                class="flex items-center gap-2 px-3 py-1 bg-slate-800 rounded-md hover:bg-slate-700"
                                                on:click=move |_| upload_queue.show_panel.update(|v| *v = !*v)
                                            >
                                                {if upload_queue.is_uploading.get() {
                                                    view! {
                                                        <div class="w-4 h-4 border-2 border-strix-400 border-t-transparent rounded-full animate-spin" />
                                                    }.into_any()
                                                } else {
                                                    view! {
                                                        <svg class="w-4 h-4 text-strix-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12"/>
                                                        </svg>
                                                    }.into_any()
                                                }}
                                                <span class="text-sm text-slate-300">{complete} "/" {total}</span>
                                                {(failed > 0).then(|| view! {
                                                    <span class="text-xs text-red-400">"(" {failed} " failed)"</span>
                                                })}
                                            </button>
                                        })
                                    } else {
                                        None
                                    }
                                }}
                                <div class="text-sm text-slate-400">
                                    {move || {
                                        let count = selected_objects.get().len();
                                        if count > 0 {
                                            format!("{} selected", count)
                                        } else {
                                            String::new()
                                        }
                                    }}
                                </div>
                            </div>
                        </div>

                        // Drop zone wrapper
                        <div
                            class=move || {
                                if dragging.get() {
                                    "relative rounded-lg border-2 border-dashed border-strix-500 bg-strix-500/10 transition-all"
                                } else {
                                    "relative rounded-lg border-2 border-transparent transition-all"
                                }
                            }
                            on:dragover=handle_dragover
                            on:dragleave=handle_dragleave
                            on:drop=handle_drop
                        >
                            // Drag overlay
                            {move || {
                                if dragging.get() {
                                    Some(view! {
                                        <div class="absolute inset-0 flex items-center justify-center bg-slate-900/80 rounded-lg z-10 pointer-events-none">
                                            <div class="text-center">
                                                <svg class="mx-auto h-12 w-12 text-strix-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12"/>
                                                </svg>
                                                <p class="mt-2 text-lg font-medium text-white">"Drop files to upload"</p>
                                            </div>
                                        </div>
                                    })
                                } else {
                                    None
                                }
                            }}

                            <Card>
                                <Suspense fallback=|| view! { <LoadingFallback size=LoadingSize::Small /> }>
                                    {move || {
                                        let app_state_for_preview_outer = app_state_for_preview.clone();
                                        objects.get().and_then(|data| {
                                            match &*data {
                                                Ok(response) => Some(response.clone()),
                                                Err(_) => None,
                                            }.map(|response| {
                                                let prefix = current_prefix.get();
                                                let bucket_for_preview = bucket.get();
                                                let app_state_for_preview_outer = app_state_for_preview_outer.clone();
                                                view! {
                                                    <ObjectBrowser
                                                        response=response
                                                        prefix=prefix
                                                        bucket=bucket_for_preview.clone()
                                                        selected=selected_objects
                                                        on_navigate={
                                                            let current_prefix = current_prefix;
                                                            let selected_objects = selected_objects;
                                                            move |prefix: String| {
                                                                current_prefix.set(prefix);
                                                                selected_objects.set(Vec::new());
                                                            }
                                                        }
                                                        on_preview={
                                                            let app_state_preview = app_state_for_preview_outer.clone();
                                                            let preview_open = preview_open;
                                                            let preview_title = preview_title;
                                                            let preview_content = preview_content;
                                                            move |obj: SelectedObject| {
                                                                preview_open.set(true);
                                                                preview_title.set(obj.key.clone());
                                                                preview_content.set(format!(
                                                                    "Inline preview is disabled during refactor. Use Download for '{}' ({}).",
                                                                    obj.key,
                                                                    format_size(obj.size)
                                                                ));
                                                                app_state_preview.show_toast("Preview opened".to_string(), ToastKind::Info);
                                                            }
                                                        }
                                                    />
                                                }
                                            })
                                        })
                                    }}
                                </Suspense>
                                {move || {
                                    objects.get().and_then(|data| {
                                        match &*data {
                                            Ok(_) => None,
                                            Err(e) => Some(view! {
                                                <div class="mt-3 rounded-md bg-red-900/40 border border-red-700 p-3 text-sm text-red-200">
                                                    {format!("Failed to load objects: {}", e)}
                                                </div>
                                            }),
                                        }
                                    })
                                }}
                            </Card>
                        </div>
                    </div>
                </main>
            </div>

            // Upload panel (slide-up panel showing per-file progress)
            {move || {
                let entries = upload_queue.entries.get();
                if entries.is_empty() || !upload_queue.show_panel.get() {
                    return None;
                }

                Some(view! {
                    <UploadPanel upload_queue=upload_queue.clone() />
                })
            }}

            <ToastContainer />

            <ConfirmModal
                state=app_state_for_confirm.confirm.clone()
                on_confirm={
                    let api = api_for_confirm.clone();
                    let app_state = app_state_for_confirm.clone();
                    move |action: String| {
                        if action.starts_with("delete-objects:") {
                            let selected = selected_objects.get();
                            let api = api.clone();
                            let bucket = bucket.get();
                            let app_state = app_state.clone();
                            let count = selected.len();
                            wasm_bindgen_futures::spawn_local(async move {
                                match api.delete_objects(&bucket, selected).await {
                                    Ok(()) => {
                                        app_state.show_toast(format!("Deleted {} object(s)", count), ToastKind::Success);
                                        selected_objects.set(Vec::new());
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

            <Modal open=preview_open title="Object Preview">
                <div class="space-y-3">
                    <div class="text-xs text-slate-400">{move || preview_title.get()}</div>
                    <p class="text-sm text-slate-300">{move || preview_content.get()}</p>
                    <div class="flex justify-end">
                        <button
                            class="px-3 py-2 text-sm rounded-md text-white bg-slate-700 hover:bg-slate-600"
                            on:click=move |_| preview_open.set(false)
                        >
                            "Close"
                        </button>
                    </div>
                </div>
            </Modal>
        </div>
    }
}

/// Object browser component.
#[component]
fn ObjectBrowser(
    response: ListObjectsResponse,
    prefix: String,
    bucket: String,
    selected: RwSignal<Vec<String>>,
    on_navigate: impl Fn(String) + Clone + 'static,
    on_preview: impl Fn(SelectedObject) + Clone + 'static,
) -> impl IntoView {
    // Early return for empty state
    if response.prefixes.is_empty() && response.objects.is_empty() {
        return view! {
            <div class="text-center py-12">
                <svg class="mx-auto h-12 w-12 text-slate-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M7 21h10a2 2 0 002-2V9.414a1 1 0 00-.293-.707l-5.414-5.414A1 1 0 0012.586 3H7a2 2 0 00-2 2v14a2 2 0 002 2z"/>
                </svg>
                <h3 class="mt-2 text-sm font-medium text-white">"No objects"</h3>
                <p class="mt-1 text-sm text-slate-400">"This folder is empty."</p>
            </div>
        }.into_any();
    }

    // Clone data to avoid lifetime issues with iterators in view! macro
    let prefixes = response.prefixes.clone();
    let objects = response.objects.clone();

    // Build folder rows
    let folder_rows: Vec<_> = prefixes.iter().map(|folder_prefix| {
        let display_name = folder_prefix
            .strip_prefix(&prefix)
            .unwrap_or(folder_prefix)
            .trim_end_matches('/')
            .to_string();
        let folder_prefix_clone = folder_prefix.clone();
        let on_nav = on_navigate.clone();
        view! {
            <tr class="hover:bg-slate-700 cursor-pointer" on:click=move |_| on_nav(folder_prefix_clone.clone())>
                <td class="px-6 py-4"></td>
                <td class="px-6 py-4 whitespace-nowrap">
                    <div class="flex items-center">
                        <svg class="w-5 h-5 text-yellow-400 mr-3" fill="currentColor" viewBox="0 0 20 20">
                            <path d="M2 6a2 2 0 012-2h5l2 2h5a2 2 0 012 2v6a2 2 0 01-2 2H4a2 2 0 01-2-2V6z"/>
                        </svg>
                        <span class="text-sm font-medium text-white">{display_name}"/"</span>
                    </div>
                </td>
                <td class="px-6 py-4 whitespace-nowrap text-sm text-slate-400">"-"</td>
                <td class="px-6 py-4 whitespace-nowrap text-sm text-slate-400">"-"</td>
                <td class="px-6 py-4 whitespace-nowrap text-sm text-slate-400">"-"</td>
            </tr>
        }
    }).collect();

    // Build file rows
    let file_rows: Vec<_> = objects.iter().map(|obj| {
        let key_for_check = obj.key.clone();
        let key_for_toggle = obj.key.clone();
        let key_for_download = obj.key.clone();
        let bucket_for_download = bucket.clone();
        let display_name = obj.key
            .strip_prefix(&prefix)
            .unwrap_or(&obj.key)
            .to_string();
        let display_name_for_download = display_name.clone();
        let size = format_size(obj.size);
        let modified = format_date(&obj.last_modified);

        view! {
            <ObjectRow
                key=key_for_check
                key_for_toggle=key_for_toggle
                key_for_download=key_for_download
                bucket=bucket_for_download
                display_name=display_name
                display_name_for_download=display_name_for_download
                size=size
                modified=modified
                selected=selected
                size_raw=obj.size
                on_preview=on_preview.clone()
            />
        }
    }).collect();

    view! {
        <div class="overflow-hidden">
            <table class="min-w-full divide-y divide-slate-700">
                <thead class="bg-slate-800">
                    <tr>
                        <th scope="col" class="w-10 px-6 py-3"></th>
                        <th scope="col" class="px-6 py-3 text-left text-xs font-medium text-slate-400 uppercase tracking-wider">
                            "Name"
                        </th>
                        <th scope="col" class="px-6 py-3 text-left text-xs font-medium text-slate-400 uppercase tracking-wider">
                            "Size"
                        </th>
                        <th scope="col" class="px-6 py-3 text-left text-xs font-medium text-slate-400 uppercase tracking-wider">
                            "Last Modified"
                        </th>
                        <th scope="col" class="px-6 py-3 text-left text-xs font-medium text-slate-400 uppercase tracking-wider">
                            "Actions"
                        </th>
                    </tr>
                </thead>
                <tbody class="bg-slate-800 divide-y divide-slate-700">
                    {folder_rows}
                    {file_rows}
                </tbody>
            </table>
        </div>
    }.into_any()
}

/// Single object row component to isolate closure logic.
#[component]
fn ObjectRow(
    key: String,
    key_for_toggle: String,
    key_for_download: String,
    bucket: String,
    display_name: String,
    display_name_for_download: String,
    size: String,
    size_raw: u64,
    modified: String,
    selected: RwSignal<Vec<String>>,
    on_preview: impl Fn(SelectedObject) + Clone + 'static,
) -> impl IntoView {
    let app_state = expect_context::<AppState>();
    let downloading = RwSignal::new(false);

    let key_check = key.clone();
    let key_check2 = key.clone();
    let is_selected_class = move || selected.get().contains(&key_check);
    let is_selected_checked = move || selected.get().contains(&key_check2);

    let key_toggle = key_for_toggle.clone();
    let toggle_select = move |_: web_sys::Event| {
        let key = key_toggle.clone();
        selected.update(|s| {
            if let Some(pos) = s.iter().position(|k| k == &key) {
                s.remove(pos);
            } else {
                s.push(key);
            }
        });
    };

    let api = app_state.api.clone();
    let bucket_dl = bucket.clone();
    let key_dl = key_for_download.clone();
    let filename = display_name_for_download.clone();
    let app_state_download = app_state.clone();
    let on_download = move |ev: web_sys::MouseEvent| {
        ev.stop_propagation();
        if downloading.get() {
            return;
        }
        downloading.set(true);

        let api = api.clone();
        let bucket = bucket_dl.clone();
        let key = key_dl.clone();
        let filename = filename.clone();
        let app_state = app_state_download.clone();

        wasm_bindgen_futures::spawn_local(async move {
            match api.get_download_url(&bucket, &key).await {
                Ok(url) => {
                    if let Some(window) = web_sys::window() {
                        if let Some(document) = window.document() {
                            if let Ok(link) = document.create_element("a") {
                                let _ = link.set_attribute("href", &url);
                                let _ = link.set_attribute("download", &filename);
                                let _ = link.set_attribute("target", "_blank");
                                if let Some(body) = document.body() {
                                    let _ = body.append_child(&link);
                                    if let Some(html_link) = link.dyn_ref::<web_sys::HtmlElement>() {
                                        html_link.click();
                                    }
                                    let _ = body.remove_child(&link);
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    app_state.show_toast(format!("Failed to get download URL: {}", e), ToastKind::Error);
                }
            }
            downloading.set(false);
        });
    };

    let display_name_icon = display_name.clone();
    let preview_key = key.clone();
    let on_preview_click = {
        let on_preview = on_preview.clone();
        move |ev: web_sys::MouseEvent| {
            ev.stop_propagation();
            on_preview(SelectedObject {
                key: preview_key.clone(),
                size: size_raw,
            });
        }
    };

    view! {
        <tr class=move || if is_selected_class() { "bg-strix-900/30" } else { "hover:bg-slate-700" }>
            <td class="px-6 py-4">
                <input
                    type="checkbox"
                    class="h-4 w-4 text-strix-500 bg-slate-700 border-slate-600 rounded cursor-pointer focus:ring-strix-500"
                    prop:checked=is_selected_checked
                    on:change=toggle_select
                />
            </td>
            <td class="px-6 py-4 whitespace-nowrap">
                <div class="flex items-center">
                    <FileIcon name=display_name_icon />
                    <span class="ml-3 text-sm text-white">{display_name}</span>
                </div>
            </td>
            <td class="px-6 py-4 whitespace-nowrap text-sm text-slate-400">
                {size}
            </td>
            <td class="px-6 py-4 whitespace-nowrap text-sm text-slate-400">
                {modified}
            </td>
            <td class="px-6 py-4 whitespace-nowrap text-sm">
                <div class="flex items-center gap-3">
                    <button
                        class="text-slate-300 hover:text-white"
                        on:click=on_preview_click
                    >
                        "Preview"
                    </button>
                    <button
                        class="text-strix-400 hover:text-strix-300 disabled:opacity-50"
                        disabled=move || downloading.get()
                        on:click=on_download
                    >
                        {move || if downloading.get() { "..." } else { "Download" }}
                    </button>
                </div>
            </td>
        </tr>
    }
}

/// Upload panel component showing per-file progress.
#[component]
fn UploadPanel(
    upload_queue: UploadQueue,
) -> impl IntoView {
    let clear_all = {
        let upload_queue = upload_queue.clone();
        move |_| {
            upload_queue.entries.set(Vec::new());
            upload_queue.show_panel.set(false);
            upload_queue.completed_count.set(0);
            upload_queue.failed_count.set(0);
        }
    };

    let close_panel = {
        let upload_queue = upload_queue.clone();
        move |_| {
            upload_queue.show_panel.set(false);
        }
    };

    view! {
        <div class="fixed bottom-0 right-4 w-96 bg-slate-800 border border-slate-700 rounded-t-lg shadow-xl z-40">
            // Header
            <div class="flex items-center justify-between px-4 py-3 border-b border-slate-700">
                <div class="flex items-center gap-2">
                    <svg class="w-5 h-5 text-strix-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12"/>
                    </svg>
                    <span class="text-sm font-medium text-white">"Uploads"</span>
                    {move || {
                        let entries = upload_queue.entries.get();
                        let complete = upload_queue.completed_count.get();
                        view! {
                            <span class="text-xs text-slate-400">"(" {complete} "/" {entries.len()} ")"</span>
                        }
                    }}
                </div>
                <div class="flex items-center gap-2">
                    <button
                        on:click=clear_all
                        disabled=move || upload_queue.is_uploading.get()
                        class="text-xs text-slate-400 hover:text-slate-300 disabled:opacity-50"
                    >
                        "Clear"
                    </button>
                    <button
                        on:click=close_panel
                        class="text-slate-400 hover:text-white"
                    >
                        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"/>
                        </svg>
                    </button>
                </div>
            </div>

            // File list
            <div class="max-h-64 overflow-y-auto">
                {move || {
                    upload_queue.entries.get().iter().map(|entry| {
                        let name = entry.name.clone();
                        let size = format_size(entry.size);
                        let status = entry.status.clone();

                        view! {
                            <div class="flex items-center gap-3 px-4 py-2 border-b border-slate-700/50">
                                // Status icon
                                {match &status {
                                    UploadStatus::Pending => view! {
                                        <div class="w-5 h-5 rounded-full border-2 border-slate-500" />
                                    }.into_any(),
                                    UploadStatus::Uploading => view! {
                                        <div class="w-5 h-5 border-2 border-strix-400 border-t-transparent rounded-full animate-spin" />
                                    }.into_any(),
                                    UploadStatus::Complete => view! {
                                        <svg class="w-5 h-5 text-green-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7"/>
                                        </svg>
                                    }.into_any(),
                                    UploadStatus::Failed(_) => view! {
                                        <svg class="w-5 h-5 text-red-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"/>
                                        </svg>
                                    }.into_any(),
                                }}

                                // File info
                                <div class="flex-1 min-w-0">
                                    <p class="text-sm text-white truncate">{name}</p>
                                    <p class="text-xs text-slate-400">{size}</p>
                                </div>

                                // Error message
                                {if let UploadStatus::Failed(ref err) = status {
                                    Some(view! {
                                        <span class="text-xs text-red-400 truncate max-w-[100px]" title=err.clone()>
                                            {err.clone()}
                                        </span>
                                    })
                                } else {
                                    None
                                }}
                            </div>
                        }
                    }).collect_view()
                }}
            </div>
        </div>
    }
}

/// File icon component based on file extension.
#[component]
fn FileIcon(name: String) -> impl IntoView {
    let ext = name.split('.').last().unwrap_or("").to_lowercase();

    let (color, icon) = match ext.as_str() {
        "jpg" | "jpeg" | "png" | "gif" | "svg" | "webp" => ("text-pink-500", "image"),
        "pdf" => ("text-red-500", "pdf"),
        "doc" | "docx" => ("text-blue-500", "doc"),
        "xls" | "xlsx" => ("text-green-500", "xls"),
        "zip" | "tar" | "gz" | "rar" => ("text-yellow-600", "archive"),
        "mp4" | "mov" | "avi" | "webm" => ("text-purple-500", "video"),
        "mp3" | "wav" | "ogg" => ("text-indigo-500", "audio"),
        "js" | "ts" | "py" | "rs" | "go" | "java" => ("text-gray-600", "code"),
        _ => ("text-gray-400", "file"),
    };

    match icon {
        "image" => view! {
            <svg class=format!("w-5 h-5 {}", color) fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16l4.586-4.586a2 2 0 012.828 0L16 16m-2-2l1.586-1.586a2 2 0 012.828 0L20 14m-6-6h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z"/>
            </svg>
        }.into_any(),
        "archive" => view! {
            <svg class=format!("w-5 h-5 {}", color) fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 8h14M5 8a2 2 0 110-4h14a2 2 0 110 4M5 8v10a2 2 0 002 2h10a2 2 0 002-2V8m-9 4h4"/>
            </svg>
        }.into_any(),
        _ => view! {
            <svg class=format!("w-5 h-5 {}", color) fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M7 21h10a2 2 0 002-2V9.414a1 1 0 00-.293-.707l-5.414-5.414A1 1 0 0012.586 3H7a2 2 0 00-2 2v14a2 2 0 002 2z"/>
            </svg>
        }.into_any(),
    }
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

fn format_date(date_str: &str) -> String {
    // Simple date formatting - just return the date part
    date_str.split('T').next().unwrap_or(date_str).to_string()
}
