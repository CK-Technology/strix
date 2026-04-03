//! Settings page.

use leptos::prelude::*;

use crate::components::{Card, Header, LoadingFallback, LoadingSize, Sidebar, ToastContainer};
use crate::state::AppState;

/// Settings page component.
#[component]
pub fn Settings() -> impl IntoView {
    let app_state = expect_context::<AppState>();
    let server_info_error = RwSignal::new(Option::<String>::None);

    let server_info = LocalResource::new(move || {
        let api = app_state.api.clone();
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
    });

    view! {
        <div class="flex flex-col min-h-screen">
            <Header />
            <div class="flex flex-1">
                <Sidebar />
                <main class="flex-1 p-8 bg-slate-900">
                    <div class="max-w-7xl mx-auto">
                        <h1 class="text-2xl font-semibold text-white mb-8">"Settings"</h1>

                        <div class="space-y-8">
                            <Card title="Server Configuration">
                                <Suspense fallback=|| view! { <LoadingFallback size=LoadingSize::Small /> }>
                                    {move || {
                                        server_info.get().and_then(|info| {
                                            match &*info {
                                                Ok(i) => Some(view! {
                                                <dl class="divide-y divide-slate-700">
                                                    <div class="py-3 flex justify-between text-sm">
                                                        <dt class="text-slate-400">"Server Version"</dt>
                                                        <dd class="text-white">{i.version.clone()}</dd>
                                                    </div>
                                                    <div class="py-3 flex justify-between text-sm">
                                                        <dt class="text-slate-400">"Deployment Mode"</dt>
                                                        <dd class="text-white">{i.mode.clone()}</dd>
                                                    </div>
                                                    <div class="py-3 flex justify-between text-sm">
                                                        <dt class="text-slate-400">"Region"</dt>
                                                        <dd class="text-white">{i.region.clone()}</dd>
                                                    </div>
                                                </dl>
                                                }),
                                                Err(_) => None,
                                            }
                                        })
                                    }}
                                </Suspense>
                                {move || server_info_error.get().map(|e| view! {
                                    <div class="mt-3 rounded-md bg-red-900/40 border border-red-700 p-3 text-sm text-red-200">
                                        {format!("Server info unavailable: {}", e)}
                                    </div>
                                })}
                            </Card>

                            <Card title="S3 API Endpoints">
                                <div class="space-y-4">
                                    <div>
                                        <label class="block text-sm font-medium text-slate-300">"S3 API Endpoint"</label>
                                        <p class="mt-1 text-sm font-mono bg-slate-700 text-strix-300 p-2 rounded">"http://localhost:9000"</p>
                                    </div>
                                    <div>
                                        <label class="block text-sm font-medium text-slate-300">"Admin API Endpoint"</label>
                                        <p class="mt-1 text-sm font-mono bg-slate-700 text-strix-300 p-2 rounded">"http://localhost:9001/api/v1"</p>
                                    </div>
                                    <div>
                                        <label class="block text-sm font-medium text-slate-300">"Metrics Endpoint"</label>
                                        <p class="mt-1 text-sm font-mono bg-slate-700 text-strix-300 p-2 rounded">"http://localhost:9090/metrics"</p>
                                    </div>
                                </div>
                            </Card>

                            <Card title="Quick Start">
                                <div class="space-y-4">
                                    <p class="text-sm text-slate-300">
                                        "Configure the MinIO client (mc) or Strix CLI (sx) to connect to Strix:"
                                    </p>
                                    <pre class="text-sm bg-slate-950 text-strix-300 p-4 rounded-md overflow-x-auto border border-slate-700">
                                        <code>
                                            "sx alias set strix http://localhost:9000 ACCESS_KEY SECRET_KEY\n"
                                            "sx ls strix/\n"
                                            "sx mb strix/my-bucket\n"
                                            "sx cp file.txt strix/my-bucket/"
                                        </code>
                                    </pre>
                                </div>
                            </Card>
                        </div>
                    </div>
                </main>
            </div>
            <ToastContainer />
        </div>
    }
}
