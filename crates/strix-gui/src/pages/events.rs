//! Event notifications management page.

use leptos::prelude::*;
use gloo_storage::{LocalStorage, Storage};

use crate::api::CreateNotificationRuleRequest;
use crate::components::{Card, ConfirmModal, Header, LoadingFallback, LoadingSize, Modal, Sidebar, Table, TableRow, ToastContainer};
use crate::state::{AppState, ToastKind};

/// Event notifications page.
#[component]
pub fn Events() -> impl IntoView {
    let app_state = expect_context::<AppState>();
    let api = app_state.api.clone();

    // Selected bucket for viewing notifications
    let selected_bucket = RwSignal::new(String::new());
    let refresh_trigger = RwSignal::new(0u32);
    let buckets_error = RwSignal::new(Option::<String>::None);
    let notifications_error = RwSignal::new(Option::<String>::None);
    let saved_key = "strix_events_bucket_v1";

    if let Ok::<String, _>(stored_bucket) = LocalStorage::get(saved_key) {
        selected_bucket.set(stored_bucket);
    }

    Effect::new(move || {
        let _ = LocalStorage::set(saved_key, selected_bucket.get());
    });

    // Fetch buckets
    let buckets = {
        let api = api.clone();
        let app_state = app_state.clone();
        LocalResource::new(move || {
            let api = api.clone();
            let app_state = app_state.clone();
            async move {
                match api.list_buckets().await {
                    Ok(r) => {
                        buckets_error.set(None);
                        Ok(r.buckets)
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

    // Fetch notifications for selected bucket
    let notifications = {
        let api = api.clone();
        let app_state = app_state.clone();
        LocalResource::new(move || {
            let api = api.clone();
            let app_state = app_state.clone();
            let bucket = selected_bucket.get();
            let _refresh = refresh_trigger.get();

            async move {
                if bucket.is_empty() {
                    notifications_error.set(None);
                    return Ok(None);
                }
                match api.get_bucket_notifications(&bucket).await {
                    Ok(r) => {
                        notifications_error.set(None);
                        Ok(Some(r))
                    }
                    Err(e) => {
                        app_state.handle_error(&e);
                        let msg = e.to_string();
                        notifications_error.set(Some(msg.clone()));
                        Err(msg)
                    }
                }
            }
        })
    };

    // Create notification modal state
    let show_create_modal = RwSignal::new(false);

    view! {
        <div class="flex flex-col min-h-screen">
            <Header />
            <div class="flex flex-1">
                <Sidebar />
                <main class="flex-1 p-8 bg-slate-900">
                    <div class="max-w-7xl mx-auto">
                        <div class="flex justify-between items-center mb-8">
                            <h1 class="text-2xl font-semibold text-white">"Event Notifications"</h1>
                            <button
                                on:click=move |_| show_create_modal.set(true)
                                disabled=move || selected_bucket.get().is_empty()
                                class="inline-flex items-center px-4 py-2 border border-transparent text-sm font-medium rounded-md text-white bg-strix-600 hover:bg-strix-700 disabled:opacity-50 disabled:cursor-not-allowed"
                            >
                                "Create Notification"
                            </button>
                        </div>

                        // Info banner
                        <div class="mb-6 bg-blue-900/30 border border-blue-700 rounded-lg p-4">
                            <div class="flex">
                                <div class="flex-shrink-0">
                                    <svg class="w-5 h-5 text-blue-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"/>
                                    </svg>
                                </div>
                                <div class="ml-3">
                                    <p class="text-sm text-blue-300">
                                        "Configure event notifications to trigger webhooks or message queues when objects are created or deleted in a bucket."
                                    </p>
                                </div>
                            </div>
                        </div>

                        // Bucket selector
                        <Card>
                            <div class="flex items-center space-x-4">
                                <label class="text-sm font-medium text-slate-300">"Select Bucket:"</label>
                                <Suspense fallback=|| view! { <LoadingFallback size=LoadingSize::Small centered=false /> }>
                                    {move || {
                                        buckets.get().and_then(|data| {
                                            match &*data {
                                                Ok(bucket_list) => Some(view! {
                                                <select
                                                    class="px-3 py-2 bg-slate-700 border border-slate-600 rounded-md text-white focus:outline-none focus:ring-strix-500 focus:border-strix-500 text-sm min-w-[200px]"
                                                    on:change=move |ev| {
                                                        selected_bucket.set(event_target_value(&ev));
                                                    }
                                                >
                                                    <option value="">"-- Select a bucket --"</option>
                                                    {bucket_list.iter().map(|bucket| {
                                                        let name = bucket.name.clone();
                                                        let name_display = name.clone();
                                                        view! {
                                                            <option value=name>{name_display}</option>
                                                        }
                                                    }).collect_view()}
                                                </select>
                                                }),
                                                Err(_) => None,
                                            }
                                        })
                                    }}
                                </Suspense>
                            </div>
                            {move || buckets_error.get().map(|e| view! {
                                <p class="mt-2 text-sm text-red-300">{format!("Bucket list unavailable: {}", e)}</p>
                            })}
                        </Card>

                        // Notification rules
                        <div class="mt-6">
                            <Suspense fallback=|| view! { <LoadingFallback message="Loading notifications..." size=LoadingSize::Small /> }>
                                {move || {
                                    if selected_bucket.get().is_empty() {
                                        return view! {
                                            <Card>
                                                <div class="text-center py-12">
                                                    <svg class="mx-auto h-12 w-12 text-slate-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 17h5l-1.405-1.405A2.032 2.032 0 0118 14.158V11a6.002 6.002 0 00-4-5.659V5a2 2 0 10-4 0v.341C7.67 6.165 6 8.388 6 11v3.159c0 .538-.214 1.055-.595 1.436L4 17h5m6 0v1a3 3 0 11-6 0v-1m6 0H9"/>
                                                    </svg>
                                                    <h3 class="mt-2 text-sm font-medium text-white">"Select a bucket"</h3>
                                                    <p class="mt-1 text-sm text-slate-400">"Choose a bucket to view and manage its event notifications."</p>
                                                </div>
                                            </Card>
                                        }.into_any();
                                    }

                                    notifications.get().and_then(|data| {
                                        match &*data {
                                            Ok(Some(response)) => Some(view! {
                                                <NotificationRulesList
                                                    rules=response.rules.clone()
                                                    bucket=selected_bucket.get()
                                                />
                                            }.into_any()),
                                            _ => None,
                                        }
                                    }).unwrap_or_else(|| view! {
                                        <Card>
                                            <LoadingFallback size=LoadingSize::Small />
                                        </Card>
                                    }.into_any())
                                }}
                            </Suspense>
                            {move || notifications_error.get().map(|e| view! {
                                <div class="mt-3 rounded-md bg-red-900/40 border border-red-700 p-3 text-sm text-red-200">
                                    {format!("Failed to load notification rules: {}", e)}
                                </div>
                            })}
                        </div>

                        // Event types reference
                        <div class="mt-8">
                            <Card title="Supported Event Types">
                                <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
                                    <div>
                                        <h4 class="text-sm font-medium text-strix-400 mb-2">"Object Created Events"</h4>
                                        <ul class="text-sm text-slate-400 space-y-1">
                                            <li>"s3:ObjectCreated:* - All object creation events"</li>
                                            <li>"s3:ObjectCreated:Put - PutObject"</li>
                                            <li>"s3:ObjectCreated:Post - POST upload"</li>
                                            <li>"s3:ObjectCreated:Copy - CopyObject"</li>
                                            <li>"s3:ObjectCreated:CompleteMultipartUpload"</li>
                                        </ul>
                                    </div>
                                    <div>
                                        <h4 class="text-sm font-medium text-strix-400 mb-2">"Object Removed Events"</h4>
                                        <ul class="text-sm text-slate-400 space-y-1">
                                            <li>"s3:ObjectRemoved:* - All object removal events"</li>
                                            <li>"s3:ObjectRemoved:Delete - DeleteObject"</li>
                                            <li>"s3:ObjectRemoved:DeleteMarkerCreated"</li>
                                        </ul>
                                    </div>
                                </div>
                            </Card>
                        </div>
                    </div>
                </main>
            </div>
            <ToastContainer />

            // Create notification modal
            <Modal open=show_create_modal title="Create Notification Rule">
                <CreateNotificationForm
                    bucket=selected_bucket.get()
                    show_modal=show_create_modal
                    refresh_trigger=refresh_trigger
                />
            </Modal>

            // Confirm delete modal
            <ConfirmModal
                state=app_state.confirm.clone()
                on_confirm={
                    let api = api.clone();
                    let app_state = app_state.clone();
                    move |action: String| {
                        if let Some(rest) = action.strip_prefix("delete-notification:") {
                            // Action format: delete-notification:bucket:rule_id
                            if let Some((bucket, rule_id)) = rest.split_once(':') {
                                let bucket = bucket.to_string();
                                let rule_id = rule_id.to_string();
                                let api = api.clone();
                                let app_state = app_state.clone();
                                wasm_bindgen_futures::spawn_local(async move {
                                    match api.delete_bucket_notification(&bucket, &rule_id).await {
                                        Ok(()) => {
                                            app_state.show_toast("Notification rule deleted".to_string(), ToastKind::Success);
                                            refresh_trigger.update(|v| *v += 1);
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

/// Notification rules list component.
#[component]
fn NotificationRulesList(
    rules: Vec<crate::api::NotificationRuleInfo>,
    bucket: String,
) -> impl IntoView {
    if rules.is_empty() {
        return view! {
            <Card>
                <div class="text-center py-12">
                    <svg class="mx-auto h-12 w-12 text-slate-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 17h5l-1.405-1.405A2.032 2.032 0 0118 14.158V11a6.002 6.002 0 00-4-5.659V5a2 2 0 10-4 0v.341C7.67 6.165 6 8.388 6 11v3.159c0 .538-.214 1.055-.595 1.436L4 17h5m6 0v1a3 3 0 11-6 0v-1m6 0H9"/>
                    </svg>
                    <h3 class="mt-2 text-sm font-medium text-white">"No notification rules"</h3>
                    <p class="mt-1 text-sm text-slate-400">"Create a notification rule to get alerts when objects change."</p>
                </div>
            </Card>
        }.into_any();
    }

    let headers = vec!["ID", "Events", "Filter", "Destination", "Actions"];

    view! {
        <Card>
            <Table headers=headers>
                {rules.into_iter().map(|rule| {
                    let rule_id = rule.id.clone();
                    let rule_id_title = rule.id.clone();
                    let rule_id_display = truncate_id(&rule.id);
                    let filter_display = format_filter(&rule.prefix, &rule.suffix);
                    let dest_type = rule.destination_type.clone();
                    let dest_type_badge = rule.destination_type.clone();
                    let dest_url = rule.destination_url.clone();
                    let dest_url_display = truncate_url(&rule.destination_url);
                    let bucket_clone = bucket.clone();
                    let events = rule.events.clone();

                    view! {
                        <TableRow>
                            <td class="px-6 py-4 whitespace-nowrap text-sm text-white font-mono" title=rule_id_title>
                                {rule_id_display}
                            </td>
                            <td class="px-6 py-4 text-sm text-slate-400">
                                <div class="flex flex-wrap gap-1">
                                    {events.into_iter().map(|event| {
                                        view! {
                                            <span class="px-2 py-0.5 bg-slate-700 text-slate-300 rounded text-xs">
                                                {event}
                                            </span>
                                        }
                                    }).collect_view()}
                                </div>
                            </td>
                            <td class="px-6 py-4 whitespace-nowrap text-sm text-slate-400">
                                {filter_display}
                            </td>
                            <td class="px-6 py-4 whitespace-nowrap text-sm">
                                <div class="flex items-center">
                                    <span class=format!("px-2 py-1 rounded text-xs font-medium mr-2 {}", destination_badge_class(&dest_type_badge))>
                                        {dest_type}
                                    </span>
                                    <span class="text-slate-400" title=dest_url>{dest_url_display}</span>
                                </div>
                            </td>
                            <td class="px-6 py-4 whitespace-nowrap text-sm">
                                <DeleteRuleButton
                                    bucket=bucket_clone
                                    rule_id=rule_id
                                />
                            </td>
                        </TableRow>
                    }
                }).collect_view()}
            </Table>
        </Card>
    }.into_any()
}

/// Create notification form component.
#[component]
fn CreateNotificationForm(
    bucket: String,
    show_modal: RwSignal<bool>,
    refresh_trigger: RwSignal<u32>,
) -> impl IntoView {
    let app_state = expect_context::<AppState>();
    let api = app_state.api.clone();

    let destination_type = RwSignal::new("webhook".to_string());
    let destination_url = RwSignal::new(String::new());
    let prefix_filter = RwSignal::new(String::new());
    let suffix_filter = RwSignal::new(String::new());
    let selected_events = RwSignal::new(vec!["s3:ObjectCreated:*".to_string()]);
    let creating = RwSignal::new(false);
    let testing = RwSignal::new(false);

    let toggle_event = move |event: &str| {
        let mut events = selected_events.get();
        if events.contains(&event.to_string()) {
            events.retain(|e| e != event);
        } else {
            events.push(event.to_string());
        }
        selected_events.set(events);
    };

    let on_create = {
        let api = api.clone();
        let bucket = bucket.clone();
        let app_state = app_state.clone();
        move |_| {
            let dest_url = destination_url.get();
            if dest_url.is_empty() {
                app_state.show_toast("Destination URL is required".to_string(), ToastKind::Error);
                return;
            }

            let events = selected_events.get();
            if events.is_empty() {
                app_state.show_toast("Select at least one event".to_string(), ToastKind::Error);
                return;
            }

            creating.set(true);
            let api = api.clone();
            let bucket = bucket.clone();
            let app_state = app_state.clone();
            let dest_type = destination_type.get();
            let prefix = prefix_filter.get();
            let suffix = suffix_filter.get();

            wasm_bindgen_futures::spawn_local(async move {
                let req = CreateNotificationRuleRequest {
                    id: None,
                    events,
                    prefix: if prefix.is_empty() { None } else { Some(prefix) },
                    suffix: if suffix.is_empty() { None } else { Some(suffix) },
                    destination_type: dest_type,
                    destination_url: dest_url,
                };

                match api.create_bucket_notification(&bucket, req).await {
                    Ok(_) => {
                        app_state.show_toast("Notification rule created".to_string(), ToastKind::Success);
                        show_modal.set(false);
                        refresh_trigger.set(refresh_trigger.get() + 1);
                    }
                    Err(e) => {
                        app_state.show_toast(format!("Failed to create rule: {}", e), ToastKind::Error);
                    }
                }
                creating.set(false);
            });
        }
    };

    let events = vec![
        ("s3:ObjectCreated:*", "All object creation"),
        ("s3:ObjectCreated:Put", "PutObject"),
        ("s3:ObjectCreated:Copy", "CopyObject"),
        ("s3:ObjectRemoved:*", "All object removal"),
        ("s3:ObjectRemoved:Delete", "DeleteObject"),
    ];

    let on_test_send = {
        let bucket = bucket.clone();
        let app_state = app_state.clone();
        move |_| {
            let dest = destination_url.get();
            if dest.is_empty() {
                app_state.show_toast("Destination URL is required".to_string(), ToastKind::Error);
                return;
            }

            testing.set(true);
            let app_state = app_state.clone();
            let bucket_name = bucket.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let body = serde_json::json!({
                    "event": "strix.notification.test",
                    "bucket": bucket_name,
                    "timestamp": js_sys::Date::new_0().to_iso_string().as_string().unwrap_or_default(),
                });

                let resp = gloo_net::http::Request::post(&dest)
                    .header("Content-Type", "application/json")
                    .body(body.to_string());

                match resp {
                    Ok(req) => match req.send().await {
                        Ok(r) if r.ok() => {
                            app_state.show_toast("Test notification delivered".to_string(), ToastKind::Success);
                        }
                        Ok(r) => {
                            app_state.show_toast(format!("Test failed with status {}", r.status()), ToastKind::Error);
                        }
                        Err(e) => {
                            app_state.show_toast(format!("Test failed: {}", e), ToastKind::Error);
                        }
                    },
                    Err(e) => {
                        app_state.show_toast(format!("Invalid test request: {}", e), ToastKind::Error);
                    }
                }

                testing.set(false);
            });
        }
    };

    view! {
        <div class="space-y-4">
            // Event selection
            <div>
                <label class="block text-sm font-medium text-slate-300 mb-2">"Events"</label>
                <div class="space-y-2">
                    {events.iter().map(|(event, label)| {
                        let event_str = event.to_string();
                        let event_check = event_str.clone();
                        let label_str = label.to_string();
                        view! {
                            <label class="flex items-center text-sm text-slate-400">
                                <input
                                    type="checkbox"
                                    class="mr-2 rounded bg-slate-700 border-slate-600 text-strix-500 focus:ring-strix-500"
                                    prop:checked=move || selected_events.get().contains(&event_check)
                                    on:change=move |_| toggle_event(&event_str)
                                />
                                {label_str}
                            </label>
                        }
                    }).collect_view()}
                </div>
            </div>

            // Filter
            <div class="grid grid-cols-2 gap-4">
                <div>
                    <label class="block text-sm font-medium text-slate-300 mb-1">"Prefix Filter (optional)"</label>
                    <input
                        type="text"
                        class="w-full px-3 py-2 bg-slate-700 border border-slate-600 rounded-md text-white placeholder-slate-400 focus:outline-none focus:ring-strix-500 focus:border-strix-500 text-sm"
                        placeholder="logs/"
                        prop:value=move || prefix_filter.get()
                        on:input=move |ev| prefix_filter.set(event_target_value(&ev))
                    />
                </div>
                <div>
                    <label class="block text-sm font-medium text-slate-300 mb-1">"Suffix Filter (optional)"</label>
                    <input
                        type="text"
                        class="w-full px-3 py-2 bg-slate-700 border border-slate-600 rounded-md text-white placeholder-slate-400 focus:outline-none focus:ring-strix-500 focus:border-strix-500 text-sm"
                        placeholder=".json"
                        prop:value=move || suffix_filter.get()
                        on:input=move |ev| suffix_filter.set(event_target_value(&ev))
                    />
                </div>
            </div>

            // Destination type
            <div>
                <label class="block text-sm font-medium text-slate-300 mb-1">"Destination Type"</label>
                <select
                    class="w-full px-3 py-2 bg-slate-700 border border-slate-600 rounded-md text-white focus:outline-none focus:ring-strix-500 focus:border-strix-500 text-sm"
                    on:change=move |ev| destination_type.set(event_target_value(&ev))
                >
                    <option value="webhook">"Webhook (HTTP POST)"</option>
                    <option value="amqp">"AMQP / RabbitMQ"</option>
                    <option value="kafka">"Kafka"</option>
                    <option value="redis">"Redis Pub/Sub"</option>
                </select>
            </div>

            // Destination URL
            <div>
                <label class="block text-sm font-medium text-slate-300 mb-1">"Destination URL"</label>
                <input
                    type="text"
                    class="w-full px-3 py-2 bg-slate-700 border border-slate-600 rounded-md text-white placeholder-slate-400 focus:outline-none focus:ring-strix-500 focus:border-strix-500 text-sm"
                    placeholder=move || match destination_type.get().as_str() {
                        "webhook" => "https://example.com/webhook",
                        "amqp" => "amqp://localhost:5672",
                        "kafka" => "localhost:9092:topic-name",
                        "redis" => "redis://localhost:6379",
                        _ => ""
                    }
                    prop:value=move || destination_url.get()
                    on:input=move |ev| destination_url.set(event_target_value(&ev))
                />
            </div>

            // Actions
            <div class="flex justify-end space-x-3 pt-4">
                <button
                    on:click=on_test_send
                    disabled=move || testing.get() || creating.get()
                    class="px-4 py-2 text-sm font-medium text-white bg-slate-700 rounded-md hover:bg-slate-600 disabled:opacity-50"
                >
                    {move || if testing.get() { "Testing..." } else { "Test Send" }}
                </button>
                <button
                    on:click=move |_| show_modal.set(false)
                    class="px-4 py-2 text-sm font-medium text-slate-300 bg-slate-700 border border-slate-600 rounded-md hover:bg-slate-600"
                >
                    "Cancel"
                </button>
                <button
                    on:click=on_create
                    disabled=move || creating.get()
                    class="px-4 py-2 text-sm font-medium text-white bg-strix-600 rounded-md hover:bg-strix-700 disabled:opacity-50"
                >
                    {move || if creating.get() { "Creating..." } else { "Create" }}
                </button>
            </div>
        </div>
    }
}

/// Delete rule button component.
#[component]
fn DeleteRuleButton(
    bucket: String,
    rule_id: String,
) -> impl IntoView {
    let app_state = expect_context::<AppState>();
    let rule_id_clone = rule_id.clone();
    let bucket_clone = bucket.clone();

    let on_delete = move |_| {
        let rule_id = rule_id_clone.clone();
        let bucket = bucket_clone.clone();
        app_state.confirm.show(
            "Delete Notification Rule",
            format!("Delete notification rule '{}'?", rule_id),
            format!("delete-notification:{}:{}", bucket, rule_id),
        );
    };

    view! {
        <button
            class="text-red-600 hover:text-red-400"
            on:click=on_delete
        >
            "Delete"
        </button>
    }
}

fn truncate_id(id: &str) -> String {
    if id.len() > 12 {
        format!("{}...", &id[..12])
    } else {
        id.to_string()
    }
}

fn truncate_url(url: &str) -> String {
    if url.len() > 40 {
        format!("{}...", &url[..40])
    } else {
        url.to_string()
    }
}

fn format_filter(prefix: &Option<String>, suffix: &Option<String>) -> String {
    match (prefix, suffix) {
        (Some(p), Some(s)) => format!("{}* / *{}", p, s),
        (Some(p), None) => format!("{}*", p),
        (None, Some(s)) => format!("*{}", s),
        (None, None) => "*".to_string(),
    }
}

fn destination_badge_class(dest_type: &str) -> &'static str {
    match dest_type {
        "webhook" => "bg-blue-900/50 text-blue-300",
        "amqp" => "bg-purple-900/50 text-purple-300",
        "kafka" => "bg-orange-900/50 text-orange-300",
        "redis" => "bg-red-900/50 text-red-300",
        _ => "bg-slate-700 text-slate-300",
    }
}
