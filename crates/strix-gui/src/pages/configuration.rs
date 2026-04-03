//! Server configuration page.

use leptos::prelude::*;

use crate::components::{Card, Header, LoadingFallback, LoadingSize, Sidebar, ToastContainer};
use crate::state::AppState;

/// Server configuration page.
#[component]
pub fn Configuration() -> impl IntoView {
    let app_state = expect_context::<AppState>();
    let api = app_state.api.clone();
    let config_error = RwSignal::new(Option::<String>::None);
    let server_info_error = RwSignal::new(Option::<String>::None);

    // Fetch server config
    let server_config = {
        let api = api.clone();
        let app_state = app_state.clone();
        LocalResource::new(move || {
            let api = api.clone();
            let app_state = app_state.clone();
            async move {
                match api.get_server_config().await {
                    Ok(config) => {
                        config_error.set(None);
                        Ok(config)
                    }
                    Err(e) => {
                        app_state.handle_error(&e);
                        let msg = e.to_string();
                        config_error.set(Some(msg.clone()));
                        Err(msg)
                    }
                }
            }
        })
    };

    // Fetch server info for uptime
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

    view! {
        <div class="flex flex-col min-h-screen">
            <Header />
            <div class="flex flex-1">
                <Sidebar />
                <main class="flex-1 p-8 bg-slate-900">
                    <div class="max-w-7xl mx-auto">
                        <h1 class="text-2xl font-semibold text-white mb-8">"Server Configuration"</h1>

                        {move || {
                            let mut errors = Vec::new();
                            if let Some(e) = server_info_error.get() {
                                errors.push(format!("Server info unavailable: {}", e));
                            }
                            if let Some(e) = config_error.get() {
                                errors.push(format!("Server config unavailable: {}", e));
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

                        // Server Status
                        <div class="mb-8">
                            <h2 class="text-lg font-medium text-white mb-4">"Server Status"</h2>
                            <Suspense fallback=|| view! { <LoadingFallback size=LoadingSize::Small /> }>
                                {move || {
                                    server_info.get().and_then(|data| {
                                        match &*data {
                                            Ok(info) => Some(view! {
                                            <div class="grid grid-cols-1 md:grid-cols-4 gap-4">
                                                <StatusCard
                                                    label="Version"
                                                    value=info.version.clone()
                                                    icon="tag"
                                                />
                                                <StatusCard
                                                    label="Mode"
                                                    value=info.mode.clone()
                                                    icon="server"
                                                />
                                                <StatusCard
                                                    label="Region"
                                                    value=info.region.clone()
                                                    icon="globe"
                                                />
                                                <StatusCard
                                                    label="Uptime"
                                                    value=format_uptime(info.uptime)
                                                    icon="clock"
                                                />
                                            </div>
                                            }),
                                            Err(_) => None,
                                        }
                                    })
                                }}
                            </Suspense>
                        </div>

                        // Network Configuration
                        <div class="mb-8">
                            <Card title="Network Configuration">
                                <Suspense fallback=|| view! { <LoadingFallback size=LoadingSize::Small /> }>
                                    {move || {
                                        server_config.get().and_then(|data| {
                                            match &*data {
                                                Ok(config) => Some(view! {
                                                <div class="grid grid-cols-1 md:grid-cols-2 gap-6">
                                                    <ConfigItem
                                                        label="S3 API Endpoint"
                                                        value=config.s3_address.clone()
                                                        description="Address for S3-compatible API operations"
                                                    />
                                                    <ConfigItem
                                                        label="Admin Console"
                                                        value=config.console_address.clone()
                                                        description="Web console and admin API address"
                                                    />
                                                    <ConfigItem
                                                        label="Metrics Endpoint"
                                                        value=config.metrics_address.clone()
                                                        description="Prometheus metrics scrape endpoint"
                                                    />
                                                    <ConfigItem
                                                        label="Region"
                                                        value=config.region.clone()
                                                        description="S3 region identifier"
                                                    />
                                                </div>
                                                }),
                                                Err(_) => None,
                                            }
                                        })
                                    }}
                                </Suspense>
                            </Card>
                        </div>

                        // Storage Configuration
                        <div class="mb-8">
                            <Card title="Storage Configuration">
                                <Suspense fallback=|| view! { <LoadingFallback size=LoadingSize::Small /> }>
                                    {move || {
                                        server_config.get().and_then(|data| {
                                            match &*data {
                                                Ok(config) => Some(view! {
                                                <div class="grid grid-cols-1 md:grid-cols-2 gap-6">
                                                    <ConfigItem
                                                        label="Storage Backend"
                                                        value=config.storage_backend.clone()
                                                        description="Type of storage backend in use"
                                                    />
                                                    <ConfigItem
                                                        label="Data Directory"
                                                        value=config.data_dir.clone()
                                                        description="Path where data is stored"
                                                    />
                                                    {config.disk_total.map(|total| {
                                                        let available = config.disk_available.unwrap_or(0);
                                                        let used = total.saturating_sub(available);
                                                        let usage_pct = if total > 0 {
                                                            (used as f64 / total as f64 * 100.0) as u32
                                                        } else {
                                                            0
                                                        };
                                                        view! {
                                                            <div class="col-span-2">
                                                                <label class="block text-sm font-medium text-slate-300 mb-2">"Disk Usage"</label>
                                                                <div class="flex items-center">
                                                                    <div class="flex-1 h-4 bg-slate-700 rounded-full overflow-hidden mr-4">
                                                                        <div
                                                                            class=format!("h-full {}", if usage_pct > 90 { "bg-red-500" } else if usage_pct > 75 { "bg-yellow-500" } else { "bg-strix-500" })
                                                                            style=format!("width: {}%", usage_pct)
                                                                        ></div>
                                                                    </div>
                                                                    <span class="text-sm text-slate-400 min-w-[200px]">
                                                                        {format_bytes(used)} " / " {format_bytes(total)} " (" {usage_pct} "%)"
                                                                    </span>
                                                                </div>
                                                            </div>
                                                        }.into_any()
                                                    }).unwrap_or_else(|| view! {
                                                        <ConfigItem
                                                            label="Disk Usage"
                                                            value="Not available".to_string()
                                                            description="Disk usage information unavailable"
                                                        />
                                                    }.into_any())}
                                                </div>
                                                }),
                                                Err(_) => None,
                                            }
                                        })
                                    }}
                                </Suspense>
                            </Card>
                        </div>

                        // Logging Configuration
                        <div class="mb-8">
                            <Card title="Logging">
                                <Suspense fallback=|| view! { <LoadingFallback size=LoadingSize::Small /> }>
                                    {move || {
                                        server_config.get().and_then(|data| {
                                            match &*data {
                                                Ok(config) => Some(view! {
                                                <div class="grid grid-cols-1 md:grid-cols-2 gap-6">
                                                    <ConfigItem
                                                        label="Log Level"
                                                        value=config.log_level.clone()
                                                        description="Current logging verbosity level"
                                                    />
                                                    <div>
                                                        <label class="block text-sm font-medium text-slate-300 mb-1">"Log Levels"</label>
                                                        <p class="text-sm text-slate-400 mb-2">"Available levels (from least to most verbose):"</p>
                                                        <div class="flex flex-wrap gap-2">
                                                            {["error", "warn", "info", "debug", "trace"].iter().map(|level| {
                                                                let is_current = config.log_level.to_lowercase() == *level;
                                                                view! {
                                                                    <span class=format!(
                                                                        "px-2 py-1 rounded text-xs font-medium {}",
                                                                        if is_current { "bg-strix-900/50 text-strix-300 border border-strix-700" } else { "bg-slate-700 text-slate-400" }
                                                                    )>
                                                                        {*level}
                                                                    </span>
                                                                }
                                                            }).collect_view()}
                                                        </div>
                                                    </div>
                                                </div>
                                                }),
                                                Err(_) => None,
                                            }
                                        })
                                    }}
                                </Suspense>
                            </Card>
                        </div>

                        // Environment Variables Reference
                        <Card title="Environment Variables">
                            <p class="text-sm text-slate-400 mb-4">
                                "Server configuration can be set via environment variables or command-line arguments."
                            </p>
                            <div class="overflow-x-auto">
                                <table class="min-w-full divide-y divide-slate-700">
                                    <thead>
                                        <tr>
                                            <th class="px-4 py-3 text-left text-xs font-medium text-slate-400 uppercase tracking-wider">"Variable"</th>
                                            <th class="px-4 py-3 text-left text-xs font-medium text-slate-400 uppercase tracking-wider">"Description"</th>
                                            <th class="px-4 py-3 text-left text-xs font-medium text-slate-400 uppercase tracking-wider">"Default"</th>
                                        </tr>
                                    </thead>
                                    <tbody class="divide-y divide-slate-700">
                                        <EnvVarRow var="STRIX_ADDRESS" desc="S3 API listen address" default="0.0.0.0:9000" />
                                        <EnvVarRow var="STRIX_CONSOLE_ADDRESS" desc="Admin console address" default="0.0.0.0:9001" />
                                        <EnvVarRow var="STRIX_METRICS_ADDRESS" desc="Metrics endpoint address" default="0.0.0.0:9090" />
                                        <EnvVarRow var="STRIX_DATA_DIR" desc="Data directory path" default="/var/lib/strix" />
                                        <EnvVarRow var="STRIX_ROOT_USER" desc="Root access key" default="(required)" />
                                        <EnvVarRow var="STRIX_ROOT_PASSWORD" desc="Root secret key" default="(required)" />
                                        <EnvVarRow var="STRIX_LOG_LEVEL" desc="Logging level" default="info" />
                                    </tbody>
                                </table>
                            </div>
                        </Card>
                    </div>
                </main>
            </div>
            <ToastContainer />
        </div>
    }
}

/// Status card component.
#[component]
fn StatusCard(
    label: &'static str,
    value: String,
    icon: &'static str,
) -> impl IntoView {
    let icon_svg = match icon {
        "tag" => view! {
            <svg class="w-5 h-5 text-strix-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M7 7h.01M7 3h5c.512 0 1.024.195 1.414.586l7 7a2 2 0 010 2.828l-7 7a2 2 0 01-2.828 0l-7-7A2 2 0 013 12V7a4 4 0 014-4z"/>
            </svg>
        }.into_any(),
        "server" => view! {
            <svg class="w-5 h-5 text-strix-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 12h14M5 12a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v4a2 2 0 01-2 2M5 12a2 2 0 00-2 2v4a2 2 0 002 2h14a2 2 0 002-2v-4a2 2 0 00-2-2m-2-4h.01M17 16h.01"/>
            </svg>
        }.into_any(),
        "globe" => view! {
            <svg class="w-5 h-5 text-strix-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 12a9 9 0 01-9 9m9-9a9 9 0 00-9-9m9 9H3m9 9a9 9 0 01-9-9m9 9c1.657 0 3-4.03 3-9s-1.343-9-3-9m0 18c-1.657 0-3-4.03-3-9s1.343-9 3-9m-9 9a9 9 0 019-9"/>
            </svg>
        }.into_any(),
        "clock" => view! {
            <svg class="w-5 h-5 text-strix-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z"/>
            </svg>
        }.into_any(),
        _ => view! { <div class="w-5 h-5"></div> }.into_any(),
    };

    view! {
        <div class="bg-slate-800 rounded-lg p-4 border border-slate-700">
            <div class="flex items-center">
                <div class="flex-shrink-0">{icon_svg}</div>
                <div class="ml-3">
                    <p class="text-sm text-slate-400">{label}</p>
                    <p class="text-lg font-semibold text-white">{value}</p>
                </div>
            </div>
        </div>
    }
}

/// Configuration item component.
#[component]
fn ConfigItem(
    label: &'static str,
    value: String,
    description: &'static str,
) -> impl IntoView {
    view! {
        <div>
            <label class="block text-sm font-medium text-slate-300 mb-1">{label}</label>
            <p class="text-white font-mono bg-slate-700 px-3 py-2 rounded text-sm">{value}</p>
            <p class="text-xs text-slate-500 mt-1">{description}</p>
        </div>
    }
}

/// Environment variable table row.
#[component]
fn EnvVarRow(
    var: &'static str,
    desc: &'static str,
    default: &'static str,
) -> impl IntoView {
    view! {
        <tr class="hover:bg-slate-800">
            <td class="px-4 py-3 whitespace-nowrap text-sm font-mono text-strix-400">{var}</td>
            <td class="px-4 py-3 text-sm text-slate-400">{desc}</td>
            <td class="px-4 py-3 whitespace-nowrap text-sm font-mono text-slate-500">{default}</td>
        </tr>
    }
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
