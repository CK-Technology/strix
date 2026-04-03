//! Audit logs page.

use leptos::prelude::*;
use gloo_storage::{LocalStorage, Storage};

use crate::api::AuditLogQueryOpts;
use crate::components::{Card, Header, LoadingFallback, LoadingSize, Sidebar, Table, TableRow, ToastContainer};
use crate::state::AppState;

/// Audit logs page.
#[component]
pub fn Audit() -> impl IntoView {
    let app_state = expect_context::<AppState>();
    let api = app_state.api.clone();
    let audit_error = RwSignal::new(Option::<String>::None);

    // Filter state
    let bucket_filter = RwSignal::new(String::new());
    let operation_filter = RwSignal::new(String::new());
    let principal_filter = RwSignal::new(String::new());
    let current_page = RwSignal::new(0u32);
    let page_size = 50u32;
    let saved_key = "strix_audit_filters_v1";

    if let Ok::<String, _>(stored) = LocalStorage::get(saved_key) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&stored) {
            if let Some(s) = v.get("bucket").and_then(|x| x.as_str()) {
                bucket_filter.set(s.to_string());
            }
            if let Some(s) = v.get("operation").and_then(|x| x.as_str()) {
                operation_filter.set(s.to_string());
            }
            if let Some(s) = v.get("principal").and_then(|x| x.as_str()) {
                principal_filter.set(s.to_string());
            }
        }
    }

    Effect::new(move || {
        let value = serde_json::json!({
            "bucket": bucket_filter.get(),
            "operation": operation_filter.get(),
            "principal": principal_filter.get(),
        })
        .to_string();
        let _ = LocalStorage::set(saved_key, value);
    });

    // Fetch audit logs
    let audit_logs = {
        let api = api.clone();
        let app_state = app_state.clone();
        LocalResource::new(move || {
            let api = api.clone();
            let app_state = app_state.clone();
            let bucket = bucket_filter.get();
            let operation = operation_filter.get();
            let principal = principal_filter.get();
            let page = current_page.get();

            async move {
                let opts = AuditLogQueryOpts {
                    bucket: if bucket.is_empty() { None } else { Some(bucket) },
                    operation: if operation.is_empty() { None } else { Some(operation) },
                    principal: if principal.is_empty() { None } else { Some(principal) },
                    start_time: None,
                    end_time: None,
                    limit: Some(page_size),
                    offset: Some(page * page_size),
                };
                match api.query_audit_log(opts).await {
                    Ok(resp) => {
                        audit_error.set(None);
                        Ok(resp)
                    }
                    Err(e) => {
                        app_state.handle_error(&e);
                        let msg = e.to_string();
                        audit_error.set(Some(msg.clone()));
                        Err(msg)
                    }
                }
            }
        })
    };

    view! {
        <div class="flex flex-col min-h-screen">
            <Header />
            <div class="flex flex-1">
                <Sidebar />
                <main class="flex-1 p-8 bg-slate-900">
                    <div class="max-w-7xl mx-auto">
                        <div class="flex justify-between items-center mb-8">
                            <h1 class="text-2xl font-semibold text-white">"Audit Logs"</h1>
                            <div class="flex items-center space-x-4">
                                <button
                                    on:click=move |_| {
                                        bucket_filter.set(String::new());
                                        operation_filter.set(String::new());
                                        principal_filter.set(String::new());
                                        current_page.set(0);
                                    }
                                    class="px-4 py-2 text-sm font-medium text-slate-300 bg-slate-700 border border-slate-600 rounded-md hover:bg-slate-600"
                                >
                                    "Clear Filters"
                                </button>
                                <button
                                    on:click=move |_| {
                                        // Force refresh by toggling page
                                        let p = current_page.get();
                                        current_page.set(p);
                                    }
                                    class="px-4 py-2 text-sm font-medium text-white bg-strix-600 rounded-md hover:bg-strix-700"
                                >
                                    "Refresh"
                                </button>
                            </div>
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
                                        "Audit logs track all S3 API operations. Use filters to narrow down events by bucket, operation type, or user."
                                    </p>
                                </div>
                            </div>
                        </div>

                        // Filters
                        <Card>
                            <div class="flex flex-wrap gap-4">
                                <div class="flex-1 min-w-48">
                                    <label class="block text-sm font-medium text-slate-300 mb-1">"Bucket"</label>
                                    <input
                                        type="text"
                                        class="w-full px-3 py-2 bg-slate-700 border border-slate-600 rounded-md text-white placeholder-slate-400 focus:outline-none focus:ring-strix-500 focus:border-strix-500 text-sm"
                                        placeholder="Filter by bucket..."
                                        prop:value=move || bucket_filter.get()
                                        on:input=move |ev| {
                                            bucket_filter.set(event_target_value(&ev));
                                            current_page.set(0);
                                        }
                                    />
                                </div>
                                <div class="w-48">
                                    <label class="block text-sm font-medium text-slate-300 mb-1">"Operation"</label>
                                    <select
                                        class="w-full px-3 py-2 bg-slate-700 border border-slate-600 rounded-md text-white focus:outline-none focus:ring-strix-500 focus:border-strix-500 text-sm"
                                        on:change=move |ev| {
                                            operation_filter.set(event_target_value(&ev));
                                            current_page.set(0);
                                        }
                                    >
                                        <option value="">"All Operations"</option>
                                        <option value="CreateBucket">"CreateBucket"</option>
                                        <option value="DeleteBucket">"DeleteBucket"</option>
                                        <option value="ListBuckets">"ListBuckets"</option>
                                        <option value="PutObject">"PutObject"</option>
                                        <option value="GetObject">"GetObject"</option>
                                        <option value="DeleteObject">"DeleteObject"</option>
                                        <option value="HeadObject">"HeadObject"</option>
                                        <option value="ListObjects">"ListObjects"</option>
                                        <option value="ListObjectsV2">"ListObjectsV2"</option>
                                        <option value="CopyObject">"CopyObject"</option>
                                        <option value="CreateMultipartUpload">"CreateMultipartUpload"</option>
                                        <option value="UploadPart">"UploadPart"</option>
                                        <option value="CompleteMultipartUpload">"CompleteMultipartUpload"</option>
                                        <option value="AbortMultipartUpload">"AbortMultipartUpload"</option>
                                    </select>
                                </div>
                                <div class="flex-1 min-w-48">
                                    <label class="block text-sm font-medium text-slate-300 mb-1">"Principal"</label>
                                    <input
                                        type="text"
                                        class="w-full px-3 py-2 bg-slate-700 border border-slate-600 rounded-md text-white placeholder-slate-400 focus:outline-none focus:ring-strix-500 focus:border-strix-500 text-sm"
                                        placeholder="Filter by user/access key..."
                                        prop:value=move || principal_filter.get()
                                        on:input=move |ev| {
                                            principal_filter.set(event_target_value(&ev));
                                            current_page.set(0);
                                        }
                                    />
                                </div>
                            </div>
                        </Card>

                        // Audit log entries
                        <div class="mt-6">
                            <Suspense fallback=|| view! { <LoadingFallback message="Loading audit logs..." size=LoadingSize::Small /> }>
                                {move || {
                                    audit_logs.get().and_then(|data| {
                                        match &*data {
                                            Ok(response) => Some(view! {
                                                <AuditLogTable
                                                    entries=response.entries.clone()
                                                    total=response.total
                                                    limit=response.limit
                                                    offset=response.offset
                                                    current_page=current_page
                                                />
                                            }),
                                            Err(_) => None,
                                        }
                                    })
                                }}
                            </Suspense>
                            {move || audit_error.get().map(|e| view! {
                                <div class="mt-3 rounded-md bg-red-900/40 border border-red-700 p-3 text-sm text-red-200">
                                    {format!("Failed to load audit logs: {}", e)}
                                </div>
                            })}
                        </div>
                    </div>
                </main>
            </div>
            <ToastContainer />
        </div>
    }
}

/// Audit log table component.
#[component]
fn AuditLogTable(
    entries: Vec<crate::api::AuditLogEntry>,
    total: u64,
    limit: u32,
    offset: u32,
    current_page: RwSignal<u32>,
) -> impl IntoView {
    if entries.is_empty() {
        return view! {
            <Card>
                <div class="text-center py-12">
                    <svg class="mx-auto h-12 w-12 text-slate-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2m-6 9l2 2 4-4"/>
                    </svg>
                    <h3 class="mt-2 text-sm font-medium text-white">"No audit log entries"</h3>
                    <p class="mt-1 text-sm text-slate-400">"No entries match the current filters."</p>
                </div>
            </Card>
        }.into_any();
    }

    let total_pages = (total as u32 + limit - 1) / limit;
    let page = offset / limit;

    let headers = vec!["Timestamp", "Operation", "Bucket", "Key", "Principal", "Status", "Duration"];

    view! {
        <Card>
            <Table headers=headers>
                {entries.iter().map(|entry| {
                    let timestamp = format_timestamp(&entry.timestamp);
                    let operation = entry.operation.clone();
                    let bucket = entry.bucket.clone().unwrap_or_else(|| "-".to_string());
                    let key = entry.key.clone().unwrap_or_else(|| "-".to_string());
                    let key_display = truncate_key(&key, 30);
                    let principal = entry.principal.clone().unwrap_or_else(|| "-".to_string());
                    let principal_display = truncate_key(&principal, 20);
                    let status_code = entry.status_code;
                    let duration = entry.duration_ms.map(|d| format!("{}ms", d)).unwrap_or_else(|| "-".to_string());

                    let status_class = if status_code >= 200 && status_code < 300 {
                        "bg-green-900/50 text-green-300"
                    } else if status_code >= 400 && status_code < 500 {
                        "bg-yellow-900/50 text-yellow-300"
                    } else if status_code >= 500 {
                        "bg-red-900/50 text-red-300"
                    } else {
                        "bg-slate-700 text-slate-300"
                    };

                    view! {
                        <TableRow>
                            <td class="px-6 py-4 whitespace-nowrap text-sm text-slate-400 font-mono">
                                {timestamp}
                            </td>
                            <td class="px-6 py-4 whitespace-nowrap text-sm">
                                <span class="px-2 py-1 bg-slate-700 text-slate-300 rounded text-xs font-medium">
                                    {operation}
                                </span>
                            </td>
                            <td class="px-6 py-4 whitespace-nowrap text-sm text-strix-400">
                                {bucket}
                            </td>
                            <td class="px-6 py-4 whitespace-nowrap text-sm text-slate-400" title=key>
                                {key_display}
                            </td>
                            <td class="px-6 py-4 whitespace-nowrap text-sm text-slate-400" title=principal>
                                {principal_display}
                            </td>
                            <td class="px-6 py-4 whitespace-nowrap text-sm">
                                <span class=format!("px-2 py-1 rounded text-xs font-medium {}", status_class)>
                                    {status_code}
                                </span>
                            </td>
                            <td class="px-6 py-4 whitespace-nowrap text-sm text-slate-400">
                                {duration}
                            </td>
                        </TableRow>
                    }
                }).collect_view()}
            </Table>

            // Pagination
            <div class="mt-4 flex items-center justify-between border-t border-slate-700 pt-4">
                <div class="text-sm text-slate-400">
                    "Showing " {offset + 1} " - " {std::cmp::min(offset + limit, total as u32)} " of " {total} " entries"
                </div>
                <div class="flex space-x-2">
                    <button
                        on:click=move |_| {
                            if page > 0 {
                                current_page.set(page - 1);
                            }
                        }
                        disabled=move || page == 0
                        class="px-3 py-1 text-sm bg-slate-700 text-slate-300 rounded hover:bg-slate-600 disabled:opacity-50 disabled:cursor-not-allowed"
                    >
                        "Previous"
                    </button>
                    <span class="px-3 py-1 text-sm text-slate-400">
                        "Page " {page + 1} " of " {total_pages}
                    </span>
                    <button
                        on:click=move |_| {
                            if page + 1 < total_pages {
                                current_page.set(page + 1);
                            }
                        }
                        disabled=move || page + 1 >= total_pages
                        class="px-3 py-1 text-sm bg-slate-700 text-slate-300 rounded hover:bg-slate-600 disabled:opacity-50 disabled:cursor-not-allowed"
                    >
                        "Next"
                    </button>
                </div>
            </div>
        </Card>
    }.into_any()
}

fn format_timestamp(iso: &str) -> String {
    // Parse ISO 8601 and format as readable
    if let Some(date_part) = iso.split('T').next() {
        if let Some(time_part) = iso.split('T').nth(1) {
            let time = time_part.split('.').next().unwrap_or(time_part);
            return format!("{} {}", date_part, time);
        }
    }
    iso.to_string()
}

fn truncate_key(key: &str, max_len: usize) -> String {
    if key.len() > max_len {
        format!("{}...", &key[..max_len - 3])
    } else {
        key.to_string()
    }
}
