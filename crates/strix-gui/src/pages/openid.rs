//! OpenID Connect (SSO) provider management page.

use leptos::prelude::*;

use crate::api::{OidcProviderInfo, OidcProviderPayload};
use crate::components::{
    Card, ConfirmModal, Header, LoadingFallback, LoadingSize, Modal, Sidebar, Table, TableRow,
    ToastContainer,
};
use crate::state::{AppState, ToastKind};

/// Editable form state shared between create and edit flows.
#[derive(Clone, Copy)]
struct FormState {
    /// Provider id being edited; `None` means a new provider is being created.
    editing_id: RwSignal<Option<String>>,
    id: RwSignal<String>,
    name: RwSignal<String>,
    provider_type: RwSignal<String>,
    issuer_url: RwSignal<String>,
    client_id: RwSignal<String>,
    client_secret: RwSignal<String>,
    redirect_uri: RwSignal<String>,
    scopes: RwSignal<String>,
    username_claim: RwSignal<String>,
    groups_claim: RwSignal<String>,
    default_policy: RwSignal<String>,
    auto_create_users: RwSignal<bool>,
    enabled: RwSignal<bool>,
    error: RwSignal<Option<String>>,
    discovery: RwSignal<Option<Result<String, String>>>,
}

impl FormState {
    fn new() -> Self {
        Self {
            editing_id: RwSignal::new(None),
            id: RwSignal::new(String::new()),
            name: RwSignal::new(String::new()),
            provider_type: RwSignal::new("generic".to_string()),
            issuer_url: RwSignal::new(String::new()),
            client_id: RwSignal::new(String::new()),
            client_secret: RwSignal::new(String::new()),
            redirect_uri: RwSignal::new(String::new()),
            scopes: RwSignal::new("openid email profile".to_string()),
            username_claim: RwSignal::new("sub".to_string()),
            groups_claim: RwSignal::new(String::new()),
            default_policy: RwSignal::new(String::new()),
            auto_create_users: RwSignal::new(true),
            enabled: RwSignal::new(true),
            error: RwSignal::new(None),
            discovery: RwSignal::new(None),
        }
    }

    /// Reset all fields for creating a brand-new provider.
    fn reset_for_create(&self) {
        self.editing_id.set(None);
        self.id.set(String::new());
        self.name.set(String::new());
        self.provider_type.set("generic".to_string());
        self.issuer_url.set(String::new());
        self.client_id.set(String::new());
        self.client_secret.set(String::new());
        self.redirect_uri.set(String::new());
        self.scopes.set("openid email profile".to_string());
        self.username_claim.set("sub".to_string());
        self.groups_claim.set(String::new());
        self.default_policy.set(String::new());
        self.auto_create_users.set(true);
        self.enabled.set(true);
        self.error.set(None);
        self.discovery.set(None);
    }

    /// Populate fields from an existing provider for editing.
    fn load(&self, p: &OidcProviderInfo) {
        self.editing_id.set(Some(p.id.clone()));
        self.id.set(p.id.clone());
        self.name.set(p.name.clone());
        self.provider_type.set(infer_type(&p.issuer_url));
        self.issuer_url.set(p.issuer_url.clone());
        self.client_id.set(p.client_id.clone());
        // Secret is write-only; leave blank to preserve the stored value.
        self.client_secret.set(String::new());
        self.redirect_uri.set(p.redirect_uri.clone());
        self.scopes.set(p.scopes.join(" "));
        self.username_claim.set(p.username_claim.clone());
        self.groups_claim
            .set(p.groups_claim.clone().unwrap_or_default());
        self.default_policy
            .set(p.default_policy.clone().unwrap_or_default());
        self.auto_create_users.set(p.auto_create_users);
        self.enabled.set(p.enabled);
        self.error.set(None);
        self.discovery.set(None);
    }

    /// Apply smart defaults for the selected provider type.
    fn apply_preset(&self, kind: &str) {
        self.provider_type.set(kind.to_string());
        self.scopes.set("openid email profile".to_string());
        match kind {
            "azure" => {
                self.issuer_url
                    .set("https://login.microsoftonline.com/{tenant}/v2.0".to_string());
                self.username_claim.set("preferred_username".to_string());
            }
            "google" => {
                self.issuer_url.set("https://accounts.google.com".to_string());
                self.username_claim.set("email".to_string());
            }
            _ => {
                self.username_claim.set("sub".to_string());
            }
        }
    }

    /// Build the API payload from the current field values.
    fn to_payload(self) -> OidcProviderPayload {
        let secret = self.client_secret.get();
        let groups = self.groups_claim.get();
        let policy = self.default_policy.get();
        OidcProviderPayload {
            name: self.name.get().trim().to_string(),
            enabled: self.enabled.get(),
            issuer_url: self.issuer_url.get().trim().to_string(),
            client_id: self.client_id.get().trim().to_string(),
            client_secret: if secret.is_empty() { None } else { Some(secret) },
            redirect_uri: self.redirect_uri.get().trim().to_string(),
            scopes: self
                .scopes
                .get()
                .split_whitespace()
                .map(str::to_string)
                .collect(),
            username_claim: self.username_claim.get().trim().to_string(),
            groups_claim: if groups.trim().is_empty() {
                None
            } else {
                Some(groups.trim().to_string())
            },
            auto_create_users: self.auto_create_users.get(),
            default_policy: if policy.trim().is_empty() {
                None
            } else {
                Some(policy.trim().to_string())
            },
        }
    }
}

/// Guess a provider type from its issuer URL for display/edit prefill.
fn infer_type(issuer: &str) -> String {
    if issuer.contains("login.microsoftonline.com") {
        "azure".to_string()
    } else if issuer.contains("accounts.google.com") {
        "google".to_string()
    } else {
        "generic".to_string()
    }
}

/// OpenID Connect provider management page.
#[component]
pub fn OpenId() -> impl IntoView {
    let app_state = expect_context::<AppState>();
    let api = app_state.api.clone();
    let load_error = RwSignal::new(Option::<String>::None);
    let version = RwSignal::new(0u32);

    let providers = {
        let api = api.clone();
        let app_state = app_state.clone();
        LocalResource::new(move || {
            let api = api.clone();
            let app_state = app_state.clone();
            let _v = version.get();
            async move {
                match api.list_oidc_providers().await {
                    Ok(list) => {
                        load_error.set(None);
                        Ok(list)
                    }
                    Err(e) => {
                        app_state.handle_error(&e);
                        let msg = e.to_string();
                        load_error.set(Some(msg.clone()));
                        Err(msg)
                    }
                }
            }
        })
    };

    let form = FormState::new();
    let show_modal = RwSignal::new(false);

    let open_create = move |_| {
        form.reset_for_create();
        show_modal.set(true);
    };

    // Edit requests are signalled by stashing the provider id here.
    let edit_request = RwSignal::new(Option::<OidcProviderInfo>::None);
    Effect::new({
        move || {
            if let Some(p) = edit_request.get() {
                form.load(&p);
                show_modal.set(true);
                edit_request.set(None);
            }
        }
    });

    let on_save = {
        let api = api.clone();
        let app_state = app_state.clone();
        move || {
            form.error.set(None);
            let editing = form.editing_id.get();
            let payload = form.to_payload();

            if payload.name.is_empty() {
                form.error.set(Some("Display name is required".to_string()));
                return;
            }
            if payload.issuer_url.is_empty() {
                form.error.set(Some("Issuer URL is required".to_string()));
                return;
            }
            if payload.issuer_url.contains("{tenant}") {
                form.error
                    .set(Some("Replace {tenant} in the issuer URL with your tenant id".to_string()));
                return;
            }
            if payload.client_id.is_empty() {
                form.error.set(Some("Client ID is required".to_string()));
                return;
            }
            if payload.redirect_uri.is_empty() {
                form.error.set(Some("Redirect URI is required".to_string()));
                return;
            }

            let api = api.clone();
            let app_state = app_state.clone();

            match editing {
                None => {
                    let id = form.id.get().trim().to_string();
                    if id.is_empty() {
                        form.error.set(Some("Provider id is required".to_string()));
                        return;
                    }
                    if payload.client_secret.is_none() {
                        form.error
                            .set(Some("Client secret is required for a new provider".to_string()));
                        return;
                    }
                    wasm_bindgen_futures::spawn_local(async move {
                        match api.create_oidc_provider(&id, payload).await {
                            Ok(_) => {
                                app_state.show_toast(
                                    "OIDC provider created".to_string(),
                                    ToastKind::Success,
                                );
                                show_modal.set(false);
                                version.update(|v| *v += 1);
                            }
                            Err(e) => form.error.set(Some(e.to_string())),
                        }
                    });
                }
                Some(id) => {
                    wasm_bindgen_futures::spawn_local(async move {
                        match api.update_oidc_provider(&id, payload).await {
                            Ok(()) => {
                                app_state.show_toast(
                                    "OIDC provider updated".to_string(),
                                    ToastKind::Success,
                                );
                                show_modal.set(false);
                                version.update(|v| *v += 1);
                            }
                            Err(e) => form.error.set(Some(e.to_string())),
                        }
                    });
                }
            }
        }
    };

    let on_test_discovery = {
        move || {
            let issuer = form.issuer_url.get().trim().trim_end_matches('/').to_string();
            if issuer.is_empty() || issuer.contains("{tenant}") {
                form.discovery
                    .set(Some(Err("Enter a complete issuer URL first".to_string())));
                return;
            }
            let url = format!("{}/.well-known/openid-configuration", issuer);
            form.discovery.set(Some(Ok("Checking…".to_string())));
            wasm_bindgen_futures::spawn_local(async move {
                let result = match gloo_net::http::Request::get(&url).send().await {
                    Ok(resp) if resp.ok() => match resp.json::<serde_json::Value>().await {
                        Ok(doc) => {
                            let auth = doc
                                .get("authorization_endpoint")
                                .and_then(|v| v.as_str())
                                .unwrap_or("?");
                            let token = doc
                                .get("token_endpoint")
                                .and_then(|v| v.as_str())
                                .unwrap_or("?");
                            Ok(format!(
                                "Discovery OK — authorization: {auth} | token: {token}"
                            ))
                        }
                        Err(e) => Err(format!("Invalid discovery document: {e}")),
                    },
                    Ok(resp) => Err(format!("Discovery failed: HTTP {}", resp.status())),
                    Err(e) => Err(format!("Discovery request failed: {e}")),
                };
                form.discovery.set(Some(result));
            });
        }
    };

    let form_view = form;

    view! {
        <div class="flex flex-col min-h-screen">
            <Header />
            <div class="flex flex-1">
                <Sidebar />
                <main class="flex-1 p-8 bg-slate-900">
                    <div class="max-w-7xl mx-auto">
                        <div class="flex justify-between items-center mb-8">
                            <h1 class="text-2xl font-semibold text-white">"OpenID Connect (SSO)"</h1>
                            <button
                                on:click=open_create
                                class="inline-flex items-center px-4 py-2 border border-transparent text-sm font-medium rounded-md text-white bg-strix-600 hover:bg-strix-700"
                            >
                                "Add Provider"
                            </button>
                        </div>

                        <Card>
                            <Suspense fallback=|| view! { <LoadingFallback size=LoadingSize::Small /> }>
                                {move || {
                                    providers.get().and_then(|data| match &data {
                                        Ok(list) => Some(view! {
                                            <ProviderTable providers=list.clone() edit_request=edit_request />
                                        }),
                                        Err(_) => None,
                                    })
                                }}
                            </Suspense>
                            {move || load_error.get().map(|e| view! {
                                <div class="mt-3 rounded-md bg-red-900/40 border border-red-700 p-3 text-sm text-red-200">
                                    {format!("Failed to load providers: {}", e)}
                                </div>
                            })}
                        </Card>

                        <div class="mt-8">
                            <Card title="Configuration reference">
                                <div class="space-y-4 text-sm text-slate-400">
                                    <p>
                                        "Providers are stored in the database and hot-reload without a restart. "
                                        "Client secrets are encrypted at rest and never returned by the API."
                                    </p>
                                    <p>
                                        "Set the IdP redirect/callback URI to: "
                                        <code class="bg-slate-700 px-1 rounded text-slate-200">"https://your-strix-domain/api/v1/auth/callback"</code>
                                    </p>
                                    <p>
                                        "Environment variables ("
                                        <code class="bg-slate-700 px-1 rounded text-slate-200">"STRIX_OIDC_*"</code>
                                        ") seed a provider on first boot; afterwards the console is the source of truth."
                                    </p>
                                </div>
                            </Card>
                        </div>
                    </div>
                </main>
            </div>
            <ToastContainer />

            <Modal open=show_modal title="OIDC Provider">
                <ProviderForm form=form_view on_save=on_save on_test_discovery=on_test_discovery show_modal=show_modal />
            </Modal>

            <ConfirmModal
                state=app_state.confirm.clone()
                on_confirm={
                    let api = api.clone();
                    let app_state = app_state.clone();
                    move |action: String| {
                        if let Some(id) = action.strip_prefix("delete-oidc:") {
                            let id = id.to_string();
                            let api = api.clone();
                            let app_state = app_state.clone();
                            wasm_bindgen_futures::spawn_local(async move {
                                match api.delete_oidc_provider(&id).await {
                                    Ok(()) => {
                                        app_state.show_toast("Provider deleted".to_string(), ToastKind::Success);
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

/// Provider list table.
#[component]
fn ProviderTable(
    providers: Vec<OidcProviderInfo>,
    edit_request: RwSignal<Option<OidcProviderInfo>>,
) -> impl IntoView {
    if providers.is_empty() {
        return view! {
            <div class="text-center py-12">
                <svg class="mx-auto h-12 w-12 text-slate-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 7a2 2 0 012 2m4 0a6 6 0 01-7.743 5.743L11 17H9v2H7v2H4a1 1 0 01-1-1v-2.586a1 1 0 01.293-.707l5.964-5.964A6 6 0 1121 9z"/>
                </svg>
                <h3 class="mt-2 text-sm font-medium text-white">"No SSO providers"</h3>
                <p class="mt-1 text-sm text-slate-400">"Add an identity provider to enable single sign-on."</p>
            </div>
        }.into_any();
    }

    let headers = vec!["ID", "Name", "Issuer", "Status", "Actions"];

    view! {
        <Table headers=headers>
            {providers.into_iter().map(|p| {
                let app_state = expect_context::<AppState>();
                let provider = p.clone();
                let delete_id = p.id.clone();
                let enabled = p.enabled;
                view! {
                    <TableRow>
                        <td class="px-6 py-4 whitespace-nowrap text-sm font-medium text-strix-400">{p.id.clone()}</td>
                        <td class="px-6 py-4 whitespace-nowrap text-sm text-white">{p.name.clone()}</td>
                        <td class="px-6 py-4 text-sm text-slate-400 max-w-xs truncate">{p.issuer_url.clone()}</td>
                        <td class="px-6 py-4 whitespace-nowrap text-sm">
                            {if enabled {
                                view! { <span class="px-2 py-1 text-xs rounded bg-green-900/50 text-green-300">"Enabled"</span> }.into_any()
                            } else {
                                view! { <span class="px-2 py-1 text-xs rounded bg-slate-700 text-slate-400">"Disabled"</span> }.into_any()
                            }}
                        </td>
                        <td class="px-6 py-4 whitespace-nowrap text-sm space-x-4">
                            <button
                                class="text-strix-400 hover:text-strix-300"
                                on:click=move |_| edit_request.set(Some(provider.clone()))
                            >
                                "Edit"
                            </button>
                            <button
                                class="text-red-600 hover:text-red-900"
                                on:click={
                                    let app_state = app_state.clone();
                                    let id = delete_id.clone();
                                    move |_| app_state.confirm.show(
                                        "Delete Provider",
                                        format!("Delete OIDC provider '{}'? This cannot be undone.", id),
                                        format!("delete-oidc:{}", id),
                                    )
                                }
                            >
                                "Delete"
                            </button>
                        </td>
                    </TableRow>
                }
            }).collect_view()}
        </Table>
    }.into_any()
}

/// Add/edit provider form rendered inside the modal.
#[component]
fn ProviderForm<S, T>(
    form: FormState,
    on_save: S,
    on_test_discovery: T,
    show_modal: RwSignal<bool>,
) -> impl IntoView
where
    S: Fn() + 'static,
    T: Fn() + 'static,
{
    let is_edit = form.editing_id;
    let input_class = "mt-1 block w-full px-3 py-2 bg-slate-700 border border-slate-600 rounded-md shadow-sm text-white placeholder-slate-400 focus:outline-none focus:ring-strix-500 focus:border-strix-500 sm:text-sm";

    let preset_form = form;

    view! {
        <div class="space-y-4 max-h-[70vh] overflow-y-auto pr-1">
            <div>
                <label class="block text-sm font-medium text-slate-300">"Provider Type"</label>
                <select
                    class=input_class
                    prop:value=move || preset_form.provider_type.get()
                    on:change=move |ev| preset_form.apply_preset(&event_target_value(&ev))
                >
                    <option value="generic">"Generic OIDC"</option>
                    <option value="azure">"Azure AD / Entra ID"</option>
                    <option value="google">"Google"</option>
                </select>
                <p class="mt-1 text-xs text-slate-400">"Selecting a type fills in issuer and claim defaults."</p>
            </div>

            <div>
                <label class="block text-sm font-medium text-slate-300">"Provider ID"</label>
                <input
                    type="text"
                    class=input_class
                    placeholder="e.g. corp-azure"
                    prop:value=move || form.id.get()
                    prop:disabled=move || is_edit.get().is_some()
                    on:input=move |ev| form.id.set(event_target_value(&ev))
                />
                <p class="mt-1 text-xs text-slate-400">"Stable identifier used in the login URL. Cannot be changed later."</p>
            </div>

            <div>
                <label class="block text-sm font-medium text-slate-300">"Display Name"</label>
                <input
                    type="text"
                    class=input_class
                    placeholder="Sign in with Microsoft"
                    prop:value=move || form.name.get()
                    on:input=move |ev| form.name.set(event_target_value(&ev))
                />
            </div>

            <div>
                <label class="block text-sm font-medium text-slate-300">"Issuer URL"</label>
                <input
                    type="text"
                    class=input_class
                    placeholder="https://accounts.google.com"
                    prop:value=move || form.issuer_url.get()
                    on:input=move |ev| form.issuer_url.set(event_target_value(&ev))
                />
                <button
                    type="button"
                    class="mt-2 px-2 py-1 text-xs rounded bg-slate-700 text-slate-200 hover:bg-slate-600"
                    on:click=move |_| on_test_discovery()
                >
                    "Test discovery"
                </button>
                {move || form.discovery.get().map(|res| match res {
                    Ok(msg) => view! {
                        <p class="mt-2 text-xs text-green-300 break-all">{msg}</p>
                    }.into_any(),
                    Err(msg) => view! {
                        <p class="mt-2 text-xs text-red-300 break-all">{msg}</p>
                    }.into_any(),
                })}
            </div>

            <div>
                <label class="block text-sm font-medium text-slate-300">"Client ID"</label>
                <input
                    type="text"
                    class=input_class
                    prop:value=move || form.client_id.get()
                    on:input=move |ev| form.client_id.set(event_target_value(&ev))
                />
            </div>

            <div>
                <label class="block text-sm font-medium text-slate-300">"Client Secret"</label>
                <input
                    type="password"
                    class=input_class
                    placeholder=move || if is_edit.get().is_some() { "Leave blank to keep current secret" } else { "Required" }
                    prop:value=move || form.client_secret.get()
                    on:input=move |ev| form.client_secret.set(event_target_value(&ev))
                />
                <p class="mt-1 text-xs text-slate-400">"Write-only. The stored secret is never displayed."</p>
            </div>

            <div>
                <label class="block text-sm font-medium text-slate-300">"Redirect URI"</label>
                <input
                    type="text"
                    class=input_class
                    placeholder="https://your-strix-domain/api/v1/auth/callback"
                    prop:value=move || form.redirect_uri.get()
                    on:input=move |ev| form.redirect_uri.set(event_target_value(&ev))
                />
            </div>

            <div>
                <label class="block text-sm font-medium text-slate-300">"Scopes"</label>
                <input
                    type="text"
                    class=input_class
                    placeholder="openid email profile"
                    prop:value=move || form.scopes.get()
                    on:input=move |ev| form.scopes.set(event_target_value(&ev))
                />
                <p class="mt-1 text-xs text-slate-400">"Space-separated list."</p>
            </div>

            <div class="grid grid-cols-2 gap-4">
                <div>
                    <label class="block text-sm font-medium text-slate-300">"Username Claim"</label>
                    <input
                        type="text"
                        class=input_class
                        placeholder="preferred_username"
                        prop:value=move || form.username_claim.get()
                        on:input=move |ev| form.username_claim.set(event_target_value(&ev))
                    />
                </div>
                <div>
                    <label class="block text-sm font-medium text-slate-300">"Groups Claim (optional)"</label>
                    <input
                        type="text"
                        class=input_class
                        placeholder="groups"
                        prop:value=move || form.groups_claim.get()
                        on:input=move |ev| form.groups_claim.set(event_target_value(&ev))
                    />
                </div>
            </div>

            <div>
                <label class="block text-sm font-medium text-slate-300">"Default Policy (optional)"</label>
                <input
                    type="text"
                    class=input_class
                    placeholder="ReadOnlyAccess"
                    prop:value=move || form.default_policy.get()
                    on:input=move |ev| form.default_policy.set(event_target_value(&ev))
                />
                <p class="mt-1 text-xs text-slate-400">"Attached to auto-provisioned users on first login."</p>
            </div>

            <div class="flex items-center gap-8 pt-2">
                <label class="flex items-center gap-2 text-sm text-slate-300">
                    <input
                        type="checkbox"
                        class="h-4 w-4 rounded border-slate-600 bg-slate-700 text-strix-600 focus:ring-strix-500"
                        prop:checked=move || form.auto_create_users.get()
                        on:change=move |ev| form.auto_create_users.set(event_target_checked(&ev))
                    />
                    "Auto-create users"
                </label>
                <label class="flex items-center gap-2 text-sm text-slate-300">
                    <input
                        type="checkbox"
                        class="h-4 w-4 rounded border-slate-600 bg-slate-700 text-strix-600 focus:ring-strix-500"
                        prop:checked=move || form.enabled.get()
                        on:change=move |ev| form.enabled.set(event_target_checked(&ev))
                    />
                    "Enabled"
                </label>
            </div>

            {move || form.error.get().map(|err| view! {
                <div class="p-3 bg-red-900/50 border border-red-700 rounded-md">
                    <p class="text-sm text-red-300">{err}</p>
                </div>
            })}

            <div class="flex justify-end space-x-3 pt-2">
                <button
                    on:click=move |_| show_modal.set(false)
                    class="px-4 py-2 text-sm font-medium text-slate-300 bg-slate-700 border border-slate-600 rounded-md hover:bg-slate-600"
                >
                    "Cancel"
                </button>
                <button
                    on:click=move |_| on_save()
                    class="px-4 py-2 text-sm font-medium text-white bg-strix-600 rounded-md hover:bg-strix-700"
                >
                    {move || if is_edit.get().is_some() { "Save Changes" } else { "Create Provider" }}
                </button>
            </div>
        </div>
    }
}
