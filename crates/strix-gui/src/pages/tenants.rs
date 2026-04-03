//! Tenant/workspace management page.

use leptos::prelude::*;

use crate::components::{Card, Header, Sidebar, ToastContainer};
use crate::state::{AppState, ToastKind};
use crate::tenant::make_slug;

#[component]
pub fn Tenants() -> impl IntoView {
    let app_state = expect_context::<AppState>();
    let api = app_state.api.clone();

    let refresh = RwSignal::new(0u32);
    let tenants_error = RwSignal::new(Option::<String>::None);
    let tenants = {
        let api = api.clone();
        let app_state = app_state.clone();
        LocalResource::new(move || {
            let api = api.clone();
            let app_state = app_state.clone();
            let _v = refresh.get();
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

    let name = RwSignal::new(String::new());
    let slug = RwSignal::new(String::new());
    let owner = RwSignal::new(String::new());
    let notes = RwSignal::new(String::new());

    let create_tenant = {
        let api = api.clone();
        let app_state = app_state.clone();
        move |_| {
            let tenant_name = name.get();
            let tenant_owner = owner.get();
            if tenant_name.trim().is_empty() || tenant_owner.trim().is_empty() {
                app_state.show_toast(
                    "Tenant name and owner are required".to_string(),
                    ToastKind::Error,
                );
                return;
            }

            let resolved_slug = if slug.get().trim().is_empty() {
                make_slug(&tenant_name)
            } else {
                make_slug(&slug.get())
            };

            let notes_value = if notes.get().trim().is_empty() {
                None
            } else {
                Some(notes.get())
            };

            let api = api.clone();
            let app_state = app_state.clone();
            wasm_bindgen_futures::spawn_local(async move {
                match api
                    .create_tenant(&tenant_name, &resolved_slug, &tenant_owner, notes_value)
                    .await
                {
                    Ok(_) => {
                        name.set(String::new());
                        slug.set(String::new());
                        owner.set(String::new());
                        notes.set(String::new());
                        refresh.update(|v| *v += 1);
                        app_state.show_toast("Tenant created".to_string(), ToastKind::Success);
                    }
                    Err(e) => {
                        app_state.handle_error(&e);
                    }
                }
            });
        }
    };

    let delete_tenant = {
        let api = api.clone();
        let app_state = app_state.clone();
        move |slug_value: String| {
            let api = api.clone();
            let app_state = app_state.clone();
            wasm_bindgen_futures::spawn_local(async move {
                match api.delete_tenant(&slug_value).await {
                    Ok(()) => {
                        refresh.update(|v| *v += 1);
                        app_state.show_toast("Tenant deleted".to_string(), ToastKind::Success);
                    }
                    Err(e) => {
                        app_state.handle_error(&e);
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
                    <div class="max-w-7xl mx-auto space-y-6">
                        <h1 class="text-2xl font-semibold text-white">"Tenants & Workspaces"</h1>
                        <Card title="Create Tenant">
                            <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
                                <input class="px-3 py-2 bg-slate-800 border border-slate-700 rounded text-sm text-white" placeholder="Tenant name" prop:value=move || name.get() on:input=move |ev| name.set(event_target_value(&ev)) />
                                <input class="px-3 py-2 bg-slate-800 border border-slate-700 rounded text-sm text-white" placeholder="Owner" prop:value=move || owner.get() on:input=move |ev| owner.set(event_target_value(&ev)) />
                                <input class="px-3 py-2 bg-slate-800 border border-slate-700 rounded text-sm text-white" placeholder="Slug (optional)" prop:value=move || slug.get() on:input=move |ev| slug.set(event_target_value(&ev)) />
                                <input class="px-3 py-2 bg-slate-800 border border-slate-700 rounded text-sm text-white" placeholder="Notes (optional)" prop:value=move || notes.get() on:input=move |ev| notes.set(event_target_value(&ev)) />
                            </div>
                            <div class="mt-3">
                                <button class="px-4 py-2 rounded-md bg-strix-600 hover:bg-strix-700 text-white text-sm" on:click=create_tenant>
                                    "Create Tenant"
                                </button>
                            </div>
                        </Card>

                        <Card title="Tenant Directory">
                            <Suspense fallback=|| view! { <p class="text-sm text-slate-400">"Loading tenants..."</p> }>
                                {move || tenants.get().map(|data| {
                                    let Ok(list) = (*data).clone() else {
                                        return view! {
                                            <p class="text-sm text-red-300">{move || tenants_error.get().unwrap_or_else(|| "Failed to load tenants".to_string())}</p>
                                        }.into_any();
                                    };
                                    if list.is_empty() {
                                        return view! { <p class="text-sm text-slate-400">"No tenants yet."</p> }.into_any();
                                    }

                                    view! {
                                        <div class="space-y-2">
                                            {list.into_iter().map(|t| {
                                                let slug_value = t.slug.clone();
                                                let on_delete = delete_tenant.clone();
                                                view! {
                                                    <div class="flex items-center justify-between border border-slate-700 rounded-md p-3 bg-slate-800">
                                                        <div>
                                                            <div class="text-sm text-white font-medium">{t.name}</div>
                                                            <div class="text-xs text-slate-400">"slug: "{t.slug}" | owner: "{t.owner}</div>
                                                        </div>
                                                        <button
                                                            class="text-xs px-2 py-1 rounded bg-red-700/80 hover:bg-red-700 text-white"
                                                            on:click=move |_| on_delete(slug_value.clone())
                                                        >
                                                            "Delete"
                                                        </button>
                                                    </div>
                                                }
                                            }).collect_view()}
                                        </div>
                                    }.into_any()
                                })}
                            </Suspense>
                        </Card>

                        <Card title="Conventions">
                            <div class="text-sm text-slate-300 space-y-2">
                                <p>"Use bucket naming convention: <tenant-slug>-<bucket-name>."</p>
                                <p class="text-slate-400">"Examples: acme-backups, acme-artifacts, beta-inbox"</p>
                            </div>
                        </Card>
                    </div>
                </main>
            </div>
            <ToastContainer />
        </div>
    }
}
