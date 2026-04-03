//! Server logs viewer page.

use gloo_storage::{LocalStorage, Storage};
use leptos::prelude::*;

use crate::components::{Card, Header, Sidebar, ToastContainer};
use crate::state::AppState;

/// Server logs viewer page.
#[component]
pub fn Logs() -> impl IntoView {
    let _app_state = expect_context::<AppState>();

    // Filter state
    let log_level = RwSignal::new("all".to_string());
    let search_query = RwSignal::new(String::new());
    let auto_refresh = RwSignal::new(false);
    let saved_key = "strix_logs_filters_v1";

    Effect::new(move || {
        let level = log_level.get();
        let query = search_query.get();
        let value = serde_json::json!({ "level": level, "query": query }).to_string();
        let _ = LocalStorage::set(saved_key, value);
    });

    if let Ok::<String, _>(stored) = LocalStorage::get(saved_key) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&stored) {
            if let Some(level) = v.get("level").and_then(|x| x.as_str()) {
                log_level.set(level.to_string());
            }
            if let Some(query) = v.get("query").and_then(|x| x.as_str()) {
                search_query.set(query.to_string());
            }
        }
    }

    // Sample log entries for demonstration
    let sample_logs = vec![
        LogEntry {
            timestamp: "2026-02-17 10:30:15".to_string(),
            level: "INFO".to_string(),
            message: "Server started on 0.0.0.0:9000".to_string(),
            source: "main".to_string(),
        },
        LogEntry {
            timestamp: "2026-02-17 10:30:16".to_string(),
            level: "INFO".to_string(),
            message: "IAM store initialized".to_string(),
            source: "iam".to_string(),
        },
        LogEntry {
            timestamp: "2026-02-17 10:30:17".to_string(),
            level: "INFO".to_string(),
            message: "Storage backend ready".to_string(),
            source: "storage".to_string(),
        },
        LogEntry {
            timestamp: "2026-02-17 10:31:05".to_string(),
            level: "DEBUG".to_string(),
            message: "S3 request: ListBuckets".to_string(),
            source: "s3".to_string(),
        },
        LogEntry {
            timestamp: "2026-02-17 10:32:10".to_string(),
            level: "INFO".to_string(),
            message: "Bucket 'test-bucket' created".to_string(),
            source: "storage".to_string(),
        },
        LogEntry {
            timestamp: "2026-02-17 10:33:22".to_string(),
            level: "DEBUG".to_string(),
            message: "S3 request: PutObject".to_string(),
            source: "s3".to_string(),
        },
        LogEntry {
            timestamp: "2026-02-17 10:34:00".to_string(),
            level: "WARN".to_string(),
            message: "Rate limit approaching for user 'alice'".to_string(),
            source: "auth".to_string(),
        },
        LogEntry {
            timestamp: "2026-02-17 10:35:15".to_string(),
            level: "ERROR".to_string(),
            message: "Failed to write object: disk full".to_string(),
            source: "storage".to_string(),
        },
    ];

    view! {
        <div class="flex flex-col min-h-screen">
            <Header />
            <div class="flex flex-1">
                <Sidebar />
                <main class="flex-1 p-8 bg-slate-900">
                    <div class="max-w-7xl mx-auto">
                        <div class="flex justify-between items-center mb-8">
                            <h1 class="text-2xl font-semibold text-white">"Server Logs"</h1>
                            <div class="flex items-center space-x-4">
                                <label class="flex items-center text-slate-400 text-sm">
                                    <input
                                        type="checkbox"
                                        class="mr-2 rounded bg-slate-700 border-slate-600 text-strix-500 focus:ring-strix-500"
                                        prop:checked=move || auto_refresh.get()
                                        on:change=move |ev| auto_refresh.set(event_target_checked(&ev))
                                    />
                                    "Auto-refresh"
                                </label>
                                <button class="px-4 py-2 text-sm font-medium text-white bg-strix-600 rounded-md hover:bg-strix-700">
                                    "Export Logs"
                                </button>
                            </div>
                        </div>

                        // Filters
                        <Card>
                            <div class="flex flex-wrap gap-4">
                                <div class="flex-1 min-w-48">
                                    <label class="block text-sm font-medium text-slate-300 mb-1">"Search"</label>
                                    <input
                                        type="text"
                                        class="w-full px-3 py-2 bg-slate-700 border border-slate-600 rounded-md text-white placeholder-slate-400 focus:outline-none focus:ring-strix-500 focus:border-strix-500 text-sm"
                                        placeholder="Search logs..."
                                        prop:value=move || search_query.get()
                                        on:input=move |ev| search_query.set(event_target_value(&ev))
                                    />
                                </div>
                                <div class="w-40">
                                    <label class="block text-sm font-medium text-slate-300 mb-1">"Log Level"</label>
                                    <select
                                        class="w-full px-3 py-2 bg-slate-700 border border-slate-600 rounded-md text-white focus:outline-none focus:ring-strix-500 focus:border-strix-500 text-sm"
                                        on:change=move |ev| log_level.set(event_target_value(&ev))
                                    >
                                        <option value="all">"All Levels"</option>
                                        <option value="ERROR">"Error"</option>
                                        <option value="WARN">"Warning"</option>
                                        <option value="INFO">"Info"</option>
                                        <option value="DEBUG">"Debug"</option>
                                    </select>
                                </div>
                            </div>
                        </Card>

                        // Log entries
                        <div class="mt-6">
                            <Card>
                                <div class="font-mono text-sm">
                                    {sample_logs.iter().filter(|log| {
                                        let level_filter = log_level.get();
                                        let query = search_query.get().to_lowercase();

                                        let level_matches = level_filter == "all" || log.level == level_filter;
                                        let query_matches = query.is_empty() ||
                                            log.message.to_lowercase().contains(&query) ||
                                            log.source.to_lowercase().contains(&query);

                                        level_matches && query_matches
                                    }).map(|log| {
                                        let level_class = match log.level.as_str() {
                                            "ERROR" => "text-red-400",
                                            "WARN" => "text-yellow-400",
                                            "INFO" => "text-blue-400",
                                            "DEBUG" => "text-slate-400",
                                            _ => "text-slate-400",
                                        };

                                        view! {
                                            <div class="py-2 border-b border-slate-700 last:border-0 hover:bg-slate-800">
                                                <span class="text-slate-500">{log.timestamp.clone()}</span>
                                                " "
                                                <span class=level_class>"["{log.level.clone()}"]"</span>
                                                " "
                                                <span class="text-strix-400">"["{log.source.clone()}"]"</span>
                                                " "
                                                <span class="text-white">{log.message.clone()}</span>
                                            </div>
                                        }
                                    }).collect_view()}
                                </div>
                            </Card>
                        </div>

                        // Info notice
                        <div class="mt-6 bg-blue-900/30 border border-blue-700 rounded-lg p-4">
                            <div class="flex">
                                <div class="flex-shrink-0">
                                    <svg class="w-5 h-5 text-blue-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"/>
                                    </svg>
                                </div>
                                <div class="ml-3">
                                    <p class="text-sm text-blue-300">
                                        "For production deployments, configure log shipping to your preferred logging service (e.g., Loki, Elasticsearch, CloudWatch)."
                                    </p>
                                </div>
                            </div>
                        </div>
                    </div>
                </main>
            </div>
            <ToastContainer />
        </div>
    }
}

#[derive(Clone)]
struct LogEntry {
    timestamp: String,
    level: String,
    message: String,
    source: String,
}
