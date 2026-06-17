//! License information page.

use leptos::prelude::*;

use crate::components::{Card, Header, Sidebar, ToastContainer};
use crate::state::AppState;

/// License information page.
#[component]
pub fn License() -> impl IntoView {
    let app_state = expect_context::<AppState>();
    let version = RwSignal::new(String::from("..."));

    let api = app_state.api.clone();
    let app_state_clone = app_state.clone();
    wasm_bindgen_futures::spawn_local(async move {
        match api.get_server_info().await {
            Ok(info) => version.set(info.version),
            Err(e) => {
                app_state_clone.handle_error(&e);
                version.set("unknown".into());
            }
        }
    });

    view! {
        <div class="flex flex-col min-h-screen">
            <Header />
            <div class="flex flex-1">
                <Sidebar />
                <main class="flex-1 p-8 bg-slate-900">
                    <div class="max-w-7xl mx-auto">
                        <h1 class="text-2xl font-semibold text-white mb-8">"License"</h1>

                        <Card>
                            <div class="space-y-6">
                                <div class="flex items-center space-x-4">
                                    <div class="flex-shrink-0">
                                        <div class="w-16 h-16 bg-strix-900/50 rounded-lg flex items-center justify-center">
                                            <span class="text-2xl font-bold text-strix-400">"S"</span>
                                        </div>
                                    </div>
                                    <div>
                                        <h2 class="text-xl font-semibold text-white">"Strix Community Edition"</h2>
                                        <p class="text-slate-400">"Free and Open Source"</p>
                                    </div>
                                </div>

                                <div class="border-t border-slate-700 pt-6">
                                    <dl class="grid grid-cols-1 gap-4 sm:grid-cols-2">
                                        <div>
                                            <dt class="text-sm font-medium text-slate-400">"License Type"</dt>
                                            <dd class="mt-1 text-sm text-white">"GNU AGPL v3"</dd>
                                        </div>
                                        <div>
                                            <dt class="text-sm font-medium text-slate-400">"Version"</dt>
                                            <dd class="mt-1 text-sm text-white">{move || version.get()}</dd>
                                        </div>
                                        <div>
                                            <dt class="text-sm font-medium text-slate-400">"Features"</dt>
                                            <dd class="mt-1 text-sm text-white">"All features included"</dd>
                                        </div>
                                        <div>
                                            <dt class="text-sm font-medium text-slate-400">"Support"</dt>
                                            <dd class="mt-1 text-sm text-white">"Community support"</dd>
                                        </div>
                                    </dl>
                                </div>

                                <div class="bg-strix-900/30 border border-strix-700 rounded-lg p-4">
                                    <div class="flex">
                                        <div class="flex-shrink-0">
                                            <svg class="w-5 h-5 text-strix-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z"/>
                                            </svg>
                                        </div>
                                        <div class="ml-3">
                                            <h3 class="text-sm font-medium text-strix-300">"No artificial limitations"</h3>
                                            <p class="mt-1 text-sm text-strix-300/70">
                                                "Unlike other S3-compatible storage solutions, Strix provides all features in the community edition. No enterprise vs CE split - everything is free and open source."
                                            </p>
                                        </div>
                                    </div>
                                </div>
                            </div>
                        </Card>
                    </div>
                </main>
            </div>
            <ToastContainer />
        </div>
    }
}
