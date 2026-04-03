//! Billing and usage exports page.

use leptos::prelude::*;

use crate::api::TenantInfo;
use crate::components::{Card, Header, Sidebar, ToastContainer};
use crate::state::{AppState, ToastKind};

#[component]
pub fn BillingExports() -> impl IntoView {
    let app_state = expect_context::<AppState>();
    let api = app_state.api.clone();

    let exporting = RwSignal::new(false);
    let csv_data = RwSignal::new(String::new());
    let active_tab = RwSignal::new("global".to_string());
    let selected_tenant = RwSignal::new(String::new());
    let tenants_error = RwSignal::new(Option::<String>::None);
    let tenants_resource = {
        let api = api.clone();
        let app_state = app_state.clone();
        LocalResource::new(move || {
            let api = api.clone();
            let app_state = app_state.clone();
            async move {
                match api.list_tenants().await {
                    Ok(r) => {
                        tenants_error.set(None);
                        Ok(r.items)
                    }
                    Err(e) => {
                        app_state.handle_error(&e);
                        let msg = e.to_string();
                        tenants_error.set(Some(msg.clone()));
                        Err(msg)
                    }
                }
            }
        })
    };
    let app_state_export = app_state.clone();
    let app_state_copy = app_state.clone();

    let export_now = move |_| {
        exporting.set(true);
        let api = api.clone();
        let app_state = app_state_export.clone();
        let tenants_resource = tenants_resource.clone();
        let active_tab = active_tab.clone();
        let selected_tenant = selected_tenant.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let tenant_filter = if selected_tenant.get().is_empty() {
                None
            } else {
                Some(selected_tenant.get())
            };

            match api
                .get_storage_usage_for_tenant(tenant_filter.as_deref())
                .await
            {
                Ok(usage) => {
                    let tenant_list = tenants_resource
                        .get()
                        .and_then(|d| match &*d {
                            Ok(items) => Some(items.clone()),
                            Err(_) => None,
                        })
                        .unwrap_or_default();
                    let mut out = if active_tab.get() == "tenant" {
                        String::from("tenant_slug,tenant_name,bucket_count,object_count,total_size_bytes\n")
                    } else {
                        String::from("bucket,object_count,total_size_bytes\n")
                    };

                    if active_tab.get() == "tenant" {
                        let mut rows: Vec<(TenantInfo, u64, u64, u64)> = tenant_list
                            .iter()
                            .cloned()
                            .map(|t| (t, 0, 0, 0))
                            .collect();

                        for b in usage.buckets {
                            if let Some(t) = tenant_list
                                .iter()
                                .find(|t| b.name.starts_with(&format!("{}-", t.slug)))
                            {
                                if let Some((_, buckets, objects, size)) =
                                    rows.iter_mut().find(|(tenant, _, _, _)| tenant.id == t.id)
                                {
                                    *buckets += 1;
                                    *objects += b.object_count;
                                    *size += b.total_size;
                                }
                            }
                        }

                        for (tenant, bucket_count, object_count, size) in rows {
                            out.push_str(&format!(
                                "{},{},{},{},{}\n",
                                tenant.slug, tenant.name, bucket_count, object_count, size
                            ));
                        }
                    } else {
                        for b in usage.buckets {
                            out.push_str(&format!("{},{},{}\n", b.name, b.object_count, b.total_size));
                        }
                    }
                    csv_data.set(out);
                    app_state.show_toast("Export generated".to_string(), ToastKind::Success);
                }
                Err(e) => {
                    app_state.handle_error(&e);
                }
            }
            exporting.set(false);
        });
    };

    let copy_export = move |_| {
        if let Some(window) = web_sys::window() {
            let clipboard = window.navigator().clipboard();
            let _ = clipboard.write_text(&csv_data.get());
        }
        app_state_copy.show_toast("Copied export to clipboard".to_string(), ToastKind::Info);
    };

    view! {
        <div class="flex flex-col min-h-screen">
            <Header />
            <div class="flex flex-1">
                <Sidebar />
                <main class="flex-1 p-8 bg-slate-900">
                    <div class="max-w-7xl mx-auto space-y-6">
                        <h1 class="text-2xl font-semibold text-white">"Billing Exports"</h1>
                        <Card title="Usage Export (CSV)">
                            <div class="space-y-4">
                                <div class="flex items-center gap-2">
                                    <button
                                        class=move || if active_tab.get() == "global" { "px-3 py-1 rounded bg-strix-600 text-white text-xs" } else { "px-3 py-1 rounded bg-slate-700 text-slate-200 text-xs" }
                                        on:click=move |_| active_tab.set("global".to_string())
                                    >
                                        "Global"
                                    </button>
                                    <button
                                        class=move || if active_tab.get() == "tenant" { "px-3 py-1 rounded bg-strix-600 text-white text-xs" } else { "px-3 py-1 rounded bg-slate-700 text-slate-200 text-xs" }
                                        on:click=move |_| active_tab.set("tenant".to_string())
                                    >
                                        "Tenant Rollups"
                                    </button>
                                </div>
                                <div class="flex items-center gap-2">
                                    <label class="text-xs text-slate-400">"Tenant Filter"</label>
                                    <select
                                        class="px-2 py-1 text-xs rounded bg-slate-800 border border-slate-700 text-slate-200"
                                        on:change=move |ev| selected_tenant.set(event_target_value(&ev))
                                    >
                                        <option value="">"All tenants"</option>
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
                                </div>
                                {move || tenants_error.get().map(|e| view! {
                                    <p class="text-xs text-red-300">{format!("Tenant list unavailable: {}", e)}</p>
                                })}
                                <p class="text-sm text-slate-400">"Generate quick bucket-level usage export for MSP/client reporting."</p>
                                <div class="flex gap-3">
                                    <button
                                        class="px-4 py-2 rounded-md text-white bg-strix-600 hover:bg-strix-700 disabled:opacity-50"
                                        disabled=move || exporting.get()
                                        on:click=export_now
                                    >
                                        {move || if exporting.get() { "Generating..." } else { "Generate Export" }}
                                    </button>
                                    <button
                                        class="px-4 py-2 rounded-md text-slate-200 bg-slate-700 hover:bg-slate-600"
                                        on:click=copy_export
                                    >
                                        "Copy CSV"
                                    </button>
                                </div>
                                <textarea
                                    class="w-full min-h-64 px-3 py-2 bg-slate-800 border border-slate-700 rounded text-xs font-mono text-slate-200"
                                    prop:value=move || csv_data.get()
                                    readonly=true
                                />
                            </div>
                        </Card>
                    </div>
                </main>
            </div>
            <ToastContainer />
        </div>
    }
}
