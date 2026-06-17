//! Login page.

use leptos::prelude::*;
use leptos_router::hooks::use_navigate;

use crate::api::ApiError;
use crate::components::{Button, Input};
use crate::state::AppState;

/// Login page component.
#[component]
pub fn Login() -> impl IntoView {
    let app_state = expect_context::<AppState>();
    let navigate = use_navigate();

    let username = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let error = RwSignal::new(Option::<String>::None);
    let loading = RwSignal::new(false);

    // SSO providers for "Sign in with..." buttons.
    let providers = RwSignal::new(Vec::<crate::api::AuthProvider>::new());
    {
        let api = app_state.api.clone();
        wasm_bindgen_futures::spawn_local(async move {
            if let Ok(resp) = api.get_auth_providers().await {
                providers.set(resp.providers);
            }
        });
    }

    // Surface an SSO error passed back via ?sso_error=... on the login URL.
    if let Some(window) = web_sys::window() {
        if let Ok(search) = window.location().search() {
            if let Some(idx) = search.find("sso_error=") {
                let raw = &search[idx + "sso_error=".len()..];
                let raw = raw.split('&').next().unwrap_or(raw);
                let decoded = js_sys::decode_uri_component(raw)
                    .ok()
                    .and_then(|v| v.as_string())
                    .unwrap_or_else(|| raw.replace('+', " "));
                if !decoded.is_empty() {
                    error.set(Some(decoded));
                }
            }
        }
    }

    let on_submit = {
        let app_state = app_state.clone();
        let navigate = navigate.clone();
        move |ev: web_sys::SubmitEvent| {
            ev.prevent_default();

            let username_val = username.get();
            let password_val = password.get();

            if username_val.is_empty() || password_val.is_empty() {
                error.set(Some("Username and password are required".to_string()));
                return;
            }

            loading.set(true);
            error.set(None);

            // Call the API to verify credentials
            let api = app_state.api.clone();
            let app_state = app_state.clone();
            let navigate = navigate.clone();

            wasm_bindgen_futures::spawn_local(async move {
                match api.login_with_password(&username_val, &password_val).await {
                    Ok(response) => {
                        // Store the session and navigate to dashboard
                        app_state.login(response.username, response.token);
                        navigate("/", Default::default());
                    }
                    Err(e) => {
                        loading.set(false);
                        let message = match e {
                            ApiError::RateLimited(secs) => {
                                format!(
                                    "Too many failed login attempts. Please try again in {} seconds.",
                                    secs
                                )
                            }
                            ApiError::Api(msg) => msg,
                            _ => e.to_string(),
                        };
                        error.set(Some(message));
                    }
                }
            });
        }
    };

    view! {
        <div class="min-h-screen flex items-center justify-center bg-slate-900">
            <div class="max-w-md w-full space-y-8">
                <div>
                    <div class="flex justify-center">
                        <img
                            src="/strix-web.png"
                            alt="Strix"
                            class="h-32 w-32"
                        />
                    </div>
                    <h2 class="mt-4 text-center text-3xl font-extrabold text-white">
                        "Sign in to "
                        <span class="text-strix-400">"STRIX"</span>
                    </h2>
                </div>

                <form class="mt-8 space-y-6 bg-slate-800 p-8 rounded-lg shadow-xl" on:submit=on_submit>
                    <Show when=move || error.get().is_some()>
                        <div class="rounded-md bg-red-900/50 border border-red-700 p-4">
                            <div class="text-sm text-red-300">
                                {move || error.get().unwrap_or_default()}
                            </div>
                        </div>
                    </Show>

                    <div class="space-y-4">
                        <Input
                            id="username"
                            label="Username"
                            placeholder="root"
                            required=true
                            autocomplete="username"
                            value=username
                        />
                        <Input
                            id="password"
                            label="Password"
                            input_type="password"
                            placeholder="Enter your password"
                            required=true
                            autocomplete="current-password"
                            value=password
                        />
                    </div>

                    <div>
                        <Button button_type="submit" full_width=true disabled=loading>
                            {move || if loading.get() { "Signing in..." } else { "Sign in" }}
                        </Button>
                    </div>

                    <Show when=move || !providers.get().is_empty()>
                        <div class="relative">
                            <div class="absolute inset-0 flex items-center">
                                <div class="w-full border-t border-slate-600"></div>
                            </div>
                            <div class="relative flex justify-center text-sm">
                                <span class="px-2 bg-slate-800 text-slate-400">"or continue with"</span>
                            </div>
                        </div>

                        <div class="space-y-3">
                            <For
                                each=move || providers.get()
                                key=|p| p.id.clone()
                                let:provider
                            >
                                {
                                    let href = format!("/api/v1/login/oidc/{}", provider.id);
                                    let label = format!("Sign in with {}", provider.name);
                                    view! {
                                        <a
                                            href=href
                                            class="flex w-full justify-center items-center rounded-md border border-slate-600 bg-slate-700 px-4 py-2 text-sm font-medium text-white hover:bg-slate-600 transition-colors"
                                        >
                                            {label}
                                        </a>
                                    }
                                }
                            </For>
                        </div>
                    </Show>
                </form>
            </div>
        </div>
    }
}
