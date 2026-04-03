//! Login page.

use leptos::prelude::*;
use leptos_router::hooks::use_navigate;

use crate::api::ApiError;
use crate::state::AppState;

/// Login page component.
#[component]
pub fn Login() -> impl IntoView {
    let app_state = expect_context::<AppState>();
    let navigate = use_navigate();

    let access_key = RwSignal::new(String::new());
    let secret_key = RwSignal::new(String::new());
    let error = RwSignal::new(Option::<String>::None);
    let loading = RwSignal::new(false);

    let on_submit = {
        let app_state = app_state.clone();
        let navigate = navigate.clone();
        move |ev: web_sys::SubmitEvent| {
            ev.prevent_default();

            let access_key_val = access_key.get();
            let secret_key_val = secret_key.get();

            if access_key_val.is_empty() || secret_key_val.is_empty() {
                error.set(Some("Access key and secret key are required".to_string()));
                return;
            }

            loading.set(true);
            error.set(None);

            // Call the API to verify credentials
            let api = app_state.api.clone();
            let app_state = app_state.clone();
            let navigate = navigate.clone();

            wasm_bindgen_futures::spawn_local(async move {
                match api.login(&access_key_val, &secret_key_val).await {
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
                        <svg class="h-16 w-16 text-strix-400" viewBox="0 0 24 24" fill="currentColor">
                            <path d="M12 2L2 7l10 5 10-5-10-5zM2 17l10 5 10-5M2 12l10 5 10-5"/>
                        </svg>
                    </div>
                    <h2 class="mt-6 text-center text-3xl font-extrabold text-white">
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
                        <div>
                            <label for="access-key" class="block text-sm font-medium text-slate-300">
                                "Access Key ID"
                            </label>
                            <input
                                id="access-key"
                                type="text"
                                required=true
                                class="mt-1 block w-full px-3 py-2 bg-slate-700 border border-slate-600 rounded-md shadow-sm text-white placeholder-slate-400 focus:outline-none focus:ring-strix-500 focus:border-strix-500 sm:text-sm"
                                placeholder="AKIAIOSFODNN7EXAMPLE"
                                prop:value=move || access_key.get()
                                on:input=move |ev| access_key.set(event_target_value(&ev))
                            />
                        </div>

                        <div>
                            <label for="secret-key" class="block text-sm font-medium text-slate-300">
                                "Secret Access Key"
                            </label>
                            <input
                                id="secret-key"
                                type="password"
                                required=true
                                class="mt-1 block w-full px-3 py-2 bg-slate-700 border border-slate-600 rounded-md shadow-sm text-white placeholder-slate-400 focus:outline-none focus:ring-strix-500 focus:border-strix-500 sm:text-sm"
                                placeholder="wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"
                                prop:value=move || secret_key.get()
                                on:input=move |ev| secret_key.set(event_target_value(&ev))
                            />
                        </div>
                    </div>

                    <div>
                        <button
                            type="submit"
                            disabled=move || loading.get()
                            class="group relative w-full flex justify-center py-2 px-4 border border-transparent text-sm font-medium rounded-md text-white bg-strix-600 hover:bg-strix-700 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-offset-navy-900 focus:ring-strix-500 disabled:opacity-50"
                        >
                            {move || if loading.get() { "Signing in..." } else { "Sign in" }}
                        </button>
                    </div>
                </form>
            </div>
        </div>
    }
}
