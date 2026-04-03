//! Metrics dashboard page.

use leptos::prelude::*;
use leptos_router::components::A;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use crate::api::{ServerInfo, StorageUsage};
use crate::components::{Card, CircularGauge, Header, LoadingFallback, LoadingSize, Sidebar, ToastContainer};
use crate::state::AppState;

/// Metrics dashboard page.
#[component]
pub fn Metrics() -> impl IntoView {
    let app_state = expect_context::<AppState>();
    let api = app_state.api.clone();
    let server_info_error = RwSignal::new(Option::<String>::None);
    let storage_usage_error = RwSignal::new(Option::<String>::None);

    // Auto-refresh version signal
    let refresh_version = RwSignal::new(0u32);
    let auto_refresh_enabled = RwSignal::new(true);
    let refresh_interval_secs = RwSignal::new(15u32);
    let last_refresh = RwSignal::new(String::new());

    // Fetch server info
    let server_info = {
        let api = api.clone();
        let app_state = app_state.clone();
        LocalResource::new(move || {
            let api = api.clone();
            let app_state = app_state.clone();
            let _v = refresh_version.get();
            async move {
                match api.get_server_info().await {
                    Ok(info) => {
                        server_info_error.set(None);
                        Ok(info)
                    }
                    Err(e) => {
                        app_state.handle_error(&e);
                        let msg = e.to_string();
                        server_info_error.set(Some(msg.clone()));
                        Err(msg)
                    }
                }
            }
        })
    };

    // Fetch storage usage
    let storage_usage = {
        let api = api.clone();
        let app_state = app_state.clone();
        LocalResource::new(move || {
            let api = api.clone();
            let app_state = app_state.clone();
            let _v = refresh_version.get();
            async move {
                match api.get_storage_usage().await {
                    Ok(usage) => {
                        storage_usage_error.set(None);
                        Ok(usage)
                    }
                    Err(e) => {
                        app_state.handle_error(&e);
                        let msg = e.to_string();
                        storage_usage_error.set(Some(msg.clone()));
                        Err(msg)
                    }
                }
            }
        })
    };

    // Update last refresh time
    Effect::new(move || {
        let _v = refresh_version.get();
        if web_sys::window().is_some() {
            let now = js_sys::Date::new_0();
            let time_str = format!(
                "{:02}:{:02}:{:02}",
                now.get_hours(),
                now.get_minutes(),
                now.get_seconds()
            );
            last_refresh.set(time_str);
        }
    });

    // Auto-refresh interval
    Effect::new(move || {
        let enabled = auto_refresh_enabled.get();
        let interval_secs = refresh_interval_secs.get();

        if !enabled {
            return;
        }

        if let Some(window) = web_sys::window() {
            let refresh_signal = refresh_version;
            let auto_refresh_signal = auto_refresh_enabled;
            let closure = Closure::wrap(Box::new(move || {
                if auto_refresh_signal.get() {
                    refresh_signal.update(|v| *v += 1);
                }
            }) as Box<dyn Fn()>);

            if let Ok(interval_id) = window.set_interval_with_callback_and_timeout_and_arguments_0(
                closure.as_ref().unchecked_ref(),
                (interval_secs * 1000) as i32,
            ) {
                on_cleanup(move || {
                    if let Some(window) = web_sys::window() {
                        window.clear_interval_with_handle(interval_id);
                    }
                });
            }
            closure.forget();
        }
    });

    let manual_refresh = move |_| {
        refresh_version.update(|v| *v += 1);
    };

    view! {
        <div class="flex flex-col min-h-screen">
            <Header />
            <div class="flex flex-1">
                <Sidebar />
                <main class="flex-1 p-8 bg-slate-900">
                    <div class="max-w-7xl mx-auto">
                        // Header with refresh controls
                        <div class="flex justify-between items-center mb-8">
                            <h1 class="text-2xl font-semibold text-white">"Metrics Dashboard"</h1>
                            <div class="flex items-center gap-4">
                                // Last refresh time
                                <span class="text-sm text-slate-400">
                                    "Last updated: "
                                    {move || last_refresh.get()}
                                </span>

                                // Auto-refresh toggle
                                <label class="flex items-center gap-2 text-sm text-slate-300">
                                    <input
                                        type="checkbox"
                                        class="w-4 h-4 text-strix-600 bg-slate-700 border-slate-600 rounded focus:ring-strix-500"
                                        prop:checked=move || auto_refresh_enabled.get()
                                        on:change=move |ev| {
                                            auto_refresh_enabled.set(event_target_checked(&ev));
                                        }
                                    />
                                    "Auto-refresh"
                                </label>

                                // Refresh interval selector
                                <select
                                    class="px-2 py-1 text-sm bg-slate-700 border border-slate-600 rounded text-white"
                                    on:change=move |ev| {
                                        if let Ok(val) = event_target_value(&ev).parse() {
                                            refresh_interval_secs.set(val);
                                        }
                                    }
                                >
                                    <option value="10" selected=move || refresh_interval_secs.get() == 10>"10s"</option>
                                    <option value="15" selected=move || refresh_interval_secs.get() == 15>"15s"</option>
                                    <option value="30" selected=move || refresh_interval_secs.get() == 30>"30s"</option>
                                    <option value="60" selected=move || refresh_interval_secs.get() == 60>"60s"</option>
                                </select>

                                // Manual refresh button
                                <button
                                    class="p-2 text-slate-400 hover:text-white bg-slate-700 rounded hover:bg-slate-600"
                                    on:click=manual_refresh
                                    title="Refresh now"
                                >
                                    <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"/>
                                    </svg>
                                </button>
                            </div>
                        </div>

                        // Server info section
                        {move || {
                            let mut errors = Vec::new();
                            if let Some(e) = server_info_error.get() {
                                errors.push(format!("Server info unavailable: {}", e));
                            }
                            if let Some(e) = storage_usage_error.get() {
                                errors.push(format!("Storage usage unavailable: {}", e));
                            }
                            if errors.is_empty() {
                                None
                            } else {
                                Some(view! {
                                    <div class="mb-4 rounded-md bg-red-900/40 border border-red-700 p-3 text-sm text-red-200">
                                        {errors.join(" | ")}
                                    </div>
                                })
                            }
                        }}

                        <div class="mb-8">
                            <Suspense fallback=|| view! { <LoadingFallback message="Loading server info..." size=LoadingSize::Small /> }>
                                {move || {
                                    server_info.get().and_then(|data| {
                                        match &*data {
                                            Ok(info) => Some(view! { <ServerInfoCards info=info.clone() /> }),
                                            Err(_) => None,
                                        }
                                    })
                                }}
                            </Suspense>
                        </div>

                        // Storage overview
                        <div class="mb-8">
                            <h2 class="text-lg font-medium text-white mb-4">"Storage Overview"</h2>
                            <Suspense fallback=|| view! { <LoadingFallback message="Loading storage stats..." size=LoadingSize::Small /> }>
                                {move || {
                                    storage_usage.get().and_then(|data| {
                                        match &*data {
                                            Ok(usage) => Some(view! { <StorageOverview usage=usage.clone() /> }),
                                            Err(_) => None,
                                        }
                                    })
                                }}
                            </Suspense>
                        </div>

                        // Bucket breakdown
                        <div class="mb-8">
                            <h2 class="text-lg font-medium text-white mb-4">"Bucket Statistics"</h2>
                            <Suspense fallback=|| view! { <LoadingFallback message="Loading bucket stats..." size=LoadingSize::Small /> }>
                                {move || {
                                    storage_usage.get().and_then(|data| {
                                        match &*data {
                                            Ok(usage) => Some(view! { <BucketStats usage=usage.clone() /> }),
                                            Err(_) => None,
                                        }
                                    })
                                }}
                            </Suspense>
                        </div>

                        // Prometheus metrics info
                        <Card title="Prometheus Metrics">
                            <div class="space-y-4">
                                <div class="flex items-center space-x-4">
                                    <div class="flex-shrink-0">
                                        <svg class="w-8 h-8 text-orange-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z"/>
                                        </svg>
                                    </div>
                                    <div>
                                        <h3 class="text-white font-medium">"Prometheus Endpoint"</h3>
                                        <p class="text-slate-400 text-sm">"Metrics are available for Prometheus scraping"</p>
                                    </div>
                                </div>
                                <div class="bg-slate-700 p-4 rounded-md">
                                    <code class="text-strix-400 text-sm">"http://localhost:9090/metrics"</code>
                                </div>
                                <p class="text-slate-400 text-sm">
                                    "Configure your Prometheus instance to scrape this endpoint for real-time metrics."
                                </p>
                            </div>
                        </Card>
                    </div>
                </main>
            </div>
            <ToastContainer />
        </div>
    }
}

/// Server info cards component.
#[component]
fn ServerInfoCards(info: ServerInfo) -> impl IntoView {
    let uptime_str = format_uptime(info.uptime);

    view! {
        <div class="grid grid-cols-1 md:grid-cols-4 gap-4">
            <Card>
                <div class="text-center">
                    <p class="text-slate-400 text-sm">"Version"</p>
                    <p class="text-2xl font-bold text-white mt-1">{info.version}</p>
                </div>
            </Card>
            <Card>
                <div class="text-center">
                    <p class="text-slate-400 text-sm">"Mode"</p>
                    <p class="text-2xl font-bold text-white mt-1">{info.mode}</p>
                </div>
            </Card>
            <Card>
                <div class="text-center">
                    <p class="text-slate-400 text-sm">"Region"</p>
                    <p class="text-2xl font-bold text-white mt-1">{info.region}</p>
                </div>
            </Card>
            <Card>
                <div class="text-center">
                    <p class="text-slate-400 text-sm">"Uptime"</p>
                    <p class="text-2xl font-bold text-strix-400 mt-1">{uptime_str}</p>
                </div>
            </Card>
        </div>
    }
}

/// Storage overview component.
#[component]
fn StorageOverview(usage: StorageUsage) -> impl IntoView {
    // Derive percentages from real backend data
    let bucket_capacity_pct = std::cmp::min(usage.total_buckets * 10, 100) as u32;
    let object_count_pct = std::cmp::min((usage.total_objects / 100) as u32, 100);
    let bucket_capacity = 0u64;
    let capacity_pct = if bucket_capacity > 0 {
        std::cmp::min(((usage.total_size as f64 / bucket_capacity as f64) * 100.0) as u32, 100)
    } else {
        0
    };

    view! {
        <div class="grid grid-cols-1 md:grid-cols-2 gap-6">
            // Stats cards
            <Card>
                <div class="grid grid-cols-3 gap-4">
                    <div class="flex items-center">
                        <div class="flex-shrink-0 p-3 bg-blue-900/50 rounded-lg">
                            <svg class="w-6 h-6 text-blue-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 8h14M5 8a2 2 0 110-4h14a2 2 0 110 4M5 8v10a2 2 0 002 2h10a2 2 0 002-2V8m-9 4h4"/>
                            </svg>
                        </div>
                        <div class="ml-3">
                            <p class="text-slate-400 text-xs">"Buckets"</p>
                            <p class="text-xl font-bold text-white">{usage.total_buckets}</p>
                        </div>
                    </div>
                    <div class="flex items-center">
                        <div class="flex-shrink-0 p-3 bg-purple-900/50 rounded-lg">
                            <svg class="w-6 h-6 text-purple-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"/>
                            </svg>
                        </div>
                        <div class="ml-3">
                            <p class="text-slate-400 text-xs">"Objects"</p>
                            <p class="text-xl font-bold text-white">{usage.total_objects}</p>
                        </div>
                    </div>
                    <div class="flex items-center">
                        <div class="flex-shrink-0 p-3 bg-green-900/50 rounded-lg">
                            <svg class="w-6 h-6 text-green-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 7v10c0 2.21 3.582 4 8 4s8-1.79 8-4V7M4 7c0 2.21 3.582 4 8 4s8-1.79 8-4M4 7c0-2.21 3.582-4 8-4s8 1.79 8 4"/>
                            </svg>
                        </div>
                        <div class="ml-3">
                            <p class="text-slate-400 text-xs">"Total Size"</p>
                            <p class="text-xl font-bold text-white">{format_bytes(usage.total_size)}</p>
                        </div>
                    </div>
                </div>
            </Card>

            // Circular gauges
            <Card>
                <div class="flex justify-around items-center py-2">
                    <CircularGauge
                        percentage=bucket_capacity_pct
                        color="text-blue-400"
                        label="Buckets".to_string()
                        value=format!("{}", usage.total_buckets)
                    />
                    <CircularGauge
                        percentage=object_count_pct
                        color="text-purple-400"
                        label="Objects".to_string()
                        value=format_compact(usage.total_objects)
                    />
                    <CircularGauge
                        percentage=capacity_pct
                        color="text-strix-400"
                        label="Capacity".to_string()
                        value=format_bytes_short(usage.total_size)
                    />
                </div>
            </Card>
            <p class="text-xs text-slate-500 mt-2">
                {if bucket_capacity > 0 {
                    format!("Capacity based on configured bucket quotas ({})", format_bytes(bucket_capacity))
                } else {
                    "Capacity unavailable (no bucket quotas configured)".to_string()
                }}
            </p>
        </div>
    }
}

/// Bucket stats component.
#[component]
fn BucketStats(usage: StorageUsage) -> impl IntoView {
    if usage.buckets.is_empty() {
        return view! {
            <Card>
                <div class="text-center py-8">
                    <p class="text-slate-400">"No buckets yet"</p>
                </div>
            </Card>
        }.into_any();
    }

    view! {
        <Card>
            <div class="overflow-x-auto">
                <table class="min-w-full divide-y divide-slate-700">
                    <thead>
                        <tr>
                            <th class="px-6 py-3 text-left text-xs font-medium text-slate-400 uppercase tracking-wider">"Bucket"</th>
                            <th class="px-6 py-3 text-left text-xs font-medium text-slate-400 uppercase tracking-wider">"Objects"</th>
                            <th class="px-6 py-3 text-left text-xs font-medium text-slate-400 uppercase tracking-wider">"Size"</th>
                            <th class="px-6 py-3 text-left text-xs font-medium text-slate-400 uppercase tracking-wider">"% of Total"</th>
                        </tr>
                    </thead>
                    <tbody class="divide-y divide-slate-700">
                        {usage.buckets.iter().map(|bucket| {
                            let name_display = bucket.name.clone();
                            let bucket_route = format!("/buckets/{}", bucket.name);
                            let objects = bucket.object_count;
                            let size = bucket.total_size;
                            let total = usage.total_size;
                            let percentage = if total > 0 {
                                (size as f64 / total as f64 * 100.0) as u32
                            } else {
                                0
                            };

                            view! {
                                <tr class="hover:bg-slate-800">
                                    <td class="px-6 py-4 whitespace-nowrap">
                                        <A href=bucket_route attr:class="text-strix-400 hover:text-strix-300">
                                            {name_display}
                                        </A>
                                    </td>
                                    <td class="px-6 py-4 whitespace-nowrap text-slate-400">
                                        {objects}
                                    </td>
                                    <td class="px-6 py-4 whitespace-nowrap text-slate-400">
                                        {format_bytes(size)}
                                    </td>
                                    <td class="px-6 py-4 whitespace-nowrap">
                                        <div class="flex items-center">
                                            <div class="flex-1 h-2 bg-slate-700 rounded-full overflow-hidden mr-2 max-w-24">
                                                <div
                                                    class="h-full bg-strix-500"
                                                    style=format!("width: {}%", percentage)
                                                ></div>
                                            </div>
                                            <span class="text-slate-400 text-sm">{percentage}"%"</span>
                                        </div>
                                    </td>
                                </tr>
                            }
                        }).collect_view()}
                    </tbody>
                </table>
            </div>
        </Card>
    }.into_any()
}

fn format_uptime(seconds: u64) -> String {
    if seconds < 60 {
        format!("{}s", seconds)
    } else if seconds < 3600 {
        format!("{}m", seconds / 60)
    } else if seconds < 86400 {
        let hours = seconds / 3600;
        let mins = (seconds % 3600) / 60;
        format!("{}h {}m", hours, mins)
    } else {
        let days = seconds / 86400;
        let hours = (seconds % 86400) / 3600;
        format!("{}d {}h", days, hours)
    }
}

fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes < 1024 * 1024 * 1024 * 1024 {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else {
        format!("{:.2} TB", bytes as f64 / (1024.0 * 1024.0 * 1024.0 * 1024.0))
    }
}

fn format_bytes_short(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{}B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.0}K", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.0}M", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes < 1024 * 1024 * 1024 * 1024 {
        format!("{:.1}G", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else {
        format!("{:.1}T", bytes as f64 / (1024.0 * 1024.0 * 1024.0 * 1024.0))
    }
}

fn format_compact(num: u64) -> String {
    if num < 1000 {
        format!("{}", num)
    } else if num < 1000000 {
        format!("{:.1}K", num as f64 / 1000.0)
    } else if num < 1000000000 {
        format!("{:.1}M", num as f64 / 1000000.0)
    } else {
        format!("{:.1}B", num as f64 / 1000000000.0)
    }
}
