//! Dashboard page.

use leptos::prelude::*;
use leptos_router::components::A;

use crate::api::{ServerInfo, StorageUsage};
use crate::components::{Card, Header, LoadingFallback, LoadingSize, Sidebar, ToastContainer};
use crate::state::AppState;

/// Dashboard page component.
#[component]
pub fn Dashboard() -> impl IntoView {
    let app_state = expect_context::<AppState>();
    let api = app_state.api.clone();
    let server_info_error = RwSignal::new(Option::<String>::None);
    let storage_usage_error = RwSignal::new(Option::<String>::None);

    // Fetch server info
    let server_info = {
        let api = api.clone();
        let app_state = app_state.clone();
        LocalResource::new(move || {
            let api = api.clone();
            let app_state = app_state.clone();
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

    view! {
        <div class="flex flex-col min-h-screen">
            <Header />
            <div class="flex flex-1">
                <Sidebar />
                <main class="flex-1 p-8 bg-slate-900">
                    <div class="max-w-7xl mx-auto">
                        <h1 class="text-2xl font-semibold text-white mb-8">"Dashboard"</h1>

                        // Stats cards
                        <div class="grid grid-cols-1 gap-5 sm:grid-cols-2 lg:grid-cols-4 mb-8">
                            <Suspense fallback=|| view! { <StatCardSkeleton /> }>
                                {move || {
                                    storage_usage.get().and_then(|usage| {
                                        match &*usage {
                                            Ok(u) => Some(view! {
                                                <StatCard
                                                    title="Total Buckets"
                                                    value=u.total_buckets.to_string()
                                                    icon="folder"
                                                />
                                            }),
                                            Err(_) => None,
                                        }
                                    })
                                }}
                            </Suspense>
                            <Suspense fallback=|| view! { <StatCardSkeleton /> }>
                                {move || {
                                    storage_usage.get().and_then(|usage| {
                                        match &*usage {
                                            Ok(u) => Some(view! {
                                                <StatCard
                                                    title="Total Objects"
                                                    value=u.total_objects.to_string()
                                                    icon="file"
                                                />
                                            }),
                                            Err(_) => None,
                                        }
                                    })
                                }}
                            </Suspense>
                            <Suspense fallback=|| view! { <StatCardSkeleton /> }>
                                {move || {
                                    storage_usage.get().and_then(|usage| {
                                        match &*usage {
                                            Ok(u) => Some(view! {
                                                <StatCard
                                                    title="Total Size"
                                                    value=format_size(u.total_size)
                                                    icon="database"
                                                />
                                            }),
                                            Err(_) => None,
                                        }
                                    })
                                }}
                            </Suspense>
                            <Suspense fallback=|| view! { <StatCardSkeleton /> }>
                                {move || {
                                    server_info.get().and_then(|info| {
                                        match &*info {
                                            Ok(i) => Some(view! {
                                                <StatCard
                                                    title="Uptime"
                                                    value=format_duration(i.uptime)
                                                    icon="clock"
                                                />
                                            }),
                                            Err(_) => None,
                                        }
                                    })
                                }}
                            </Suspense>
                        </div>

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
                                    <div class="mb-6 rounded-md bg-red-900/40 border border-red-700 p-3 text-sm text-red-200">
                                        {errors.join(" | ")}
                                    </div>
                                })
                            }
                        }}

                        // Quick actions
                        <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-3 mb-8">
                            <A href="/buckets" attr:class="px-4 py-3 rounded-md bg-slate-800 border border-slate-700 hover:border-strix-500 text-slate-200 text-sm block">
                                "Open Object Browser"
                            </A>
                            <A href="/access-keys" attr:class="px-4 py-3 rounded-md bg-slate-800 border border-slate-700 hover:border-strix-500 text-slate-200 text-sm block">
                                "Manage Access Keys"
                            </A>
                            <A href="/policies" attr:class="px-4 py-3 rounded-md bg-slate-800 border border-slate-700 hover:border-strix-500 text-slate-200 text-sm block">
                                "Policy Templates"
                            </A>
                            <A href="/events" attr:class="px-4 py-3 rounded-md bg-slate-800 border border-slate-700 hover:border-strix-500 text-slate-200 text-sm block">
                                "Test Event Destinations"
                            </A>
                        </div>

                        // Server info
                        <div class="grid grid-cols-1 lg:grid-cols-2 gap-8">
                            <Card title="Server Information">
                                <Suspense fallback=|| view! { <LoadingFallback size=LoadingSize::Small /> }>
                                    {move || {
                                        server_info.get().and_then(|info| {
                                            match &*info {
                                                Ok(i) => Some(view! { <ServerInfoTable info=i.clone() /> }),
                                                Err(_) => None,
                                            }
                                        })
                                    }}
                                </Suspense>
                            </Card>

                            <Card title="Bucket Usage">
                                <Suspense fallback=|| view! { <LoadingFallback size=LoadingSize::Small /> }>
                                    {move || {
                                        storage_usage.get().and_then(|usage| {
                                            match &*usage {
                                                Ok(u) => Some(view! { <BucketUsageList usage=u.clone() /> }),
                                                Err(_) => None,
                                            }
                                        })
                                    }}
                                </Suspense>
                            </Card>
                        </div>
                    </div>
                </main>
            </div>
            <ToastContainer />
        </div>
    }
}

/// Stat card component.
#[component]
fn StatCard(title: &'static str, value: String, icon: &'static str) -> impl IntoView {
    let icon_svg = match icon {
        "folder" => view! {
            <svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z"/>
            </svg>
        }.into_any(),
        "file" => view! {
            <svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M7 21h10a2 2 0 002-2V9.414a1 1 0 00-.293-.707l-5.414-5.414A1 1 0 0012.586 3H7a2 2 0 00-2 2v14a2 2 0 002 2z"/>
            </svg>
        }.into_any(),
        "database" => view! {
            <svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 7v10c0 2.21 3.582 4 8 4s8-1.79 8-4V7M4 7c0 2.21 3.582 4 8 4s8-1.79 8-4M4 7c0-2.21 3.582-4 8-4s8 1.79 8 4m0 5c0 2.21-3.582 4-8 4s-8-1.79-8-4"/>
            </svg>
        }.into_any(),
        "clock" => view! {
            <svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z"/>
            </svg>
        }.into_any(),
        _ => view! { <span></span> }.into_any(),
    };

    view! {
        <div class="bg-slate-800 overflow-hidden shadow rounded-lg border border-slate-700">
            <div class="p-5">
                <div class="flex items-center">
                    <div class="flex-shrink-0">
                        <div class="text-strix-400">{icon_svg}</div>
                    </div>
                    <div class="ml-5 w-0 flex-1">
                        <dl>
                            <dt class="text-sm font-medium text-slate-400 truncate">{title}</dt>
                            <dd class="text-lg font-semibold text-white">{value}</dd>
                        </dl>
                    </div>
                </div>
            </div>
        </div>
    }
}

/// Skeleton loading card.
#[component]
fn StatCardSkeleton() -> impl IntoView {
    view! {
        <div class="bg-slate-800 overflow-hidden shadow rounded-lg animate-pulse border border-slate-700">
            <div class="p-5">
                <div class="flex items-center">
                    <div class="flex-shrink-0">
                        <div class="w-6 h-6 bg-slate-700 rounded"></div>
                    </div>
                    <div class="ml-5 w-0 flex-1">
                        <div class="h-4 bg-slate-700 rounded w-20 mb-2"></div>
                        <div class="h-6 bg-slate-700 rounded w-16"></div>
                    </div>
                </div>
            </div>
        </div>
    }
}

/// Server info table.
#[component]
fn ServerInfoTable(info: ServerInfo) -> impl IntoView {
    let commit = info.commit.clone();
    view! {
        <dl class="divide-y divide-slate-700">
            <div class="py-3 flex justify-between text-sm">
                <dt class="text-slate-400">"Version"</dt>
                <dd class="text-white">{info.version.clone()}</dd>
            </div>
            <div class="py-3 flex justify-between text-sm">
                <dt class="text-slate-400">"Mode"</dt>
                <dd class="text-white">{info.mode.clone()}</dd>
            </div>
            <div class="py-3 flex justify-between text-sm">
                <dt class="text-slate-400">"Region"</dt>
                <dd class="text-white">{info.region.clone()}</dd>
            </div>
            {commit.map(|c| view! {
                <div class="py-3 flex justify-between text-sm">
                    <dt class="text-slate-400">"Commit"</dt>
                    <dd class="text-white font-mono text-xs">{c}</dd>
                </div>
            })}
        </dl>
    }
}

/// Bucket usage list.
#[component]
fn BucketUsageList(usage: StorageUsage) -> impl IntoView {
    let buckets = usage.buckets.clone();
    let has_buckets = !buckets.is_empty();

    view! {
        <Show
            when=move || has_buckets
            fallback=|| view! {
                <p class="text-slate-400 text-sm">"No buckets yet. Create one to get started."</p>
            }
        >
            <ul class="divide-y divide-slate-700">
                {buckets.iter().map(|b| {
                    let name = b.name.clone();
                    let bucket_route = format!("/buckets/{}", b.name);
                    let object_count = b.object_count;
                    let total_size = format_size(b.total_size);
                    view! {
                        <li class="py-3">
                            <div class="flex justify-between">
                                <A href=bucket_route attr:class="text-strix-400 hover:text-strix-300">
                                    {name}
                                </A>
                                <span class="text-slate-400 text-sm">
                                    {object_count}" objects, "{total_size}
                                </span>
                            </div>
                        </li>
                    }
                }).collect_view()}
            </ul>
        </Show>
    }
}

/// Format bytes as human-readable size.
fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    const TB: u64 = GB * 1024;

    if bytes >= TB {
        format!("{:.2} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Format seconds as human-readable duration.
fn format_duration(seconds: u64) -> String {
    let days = seconds / 86400;
    let hours = (seconds % 86400) / 3600;
    let minutes = (seconds % 3600) / 60;

    if days > 0 {
        format!("{}d {}h", days, hours)
    } else if hours > 0 {
        format!("{}h {}m", hours, minutes)
    } else if minutes > 0 {
        format!("{}m", minutes)
    } else {
        format!("{}s", seconds)
    }
}
