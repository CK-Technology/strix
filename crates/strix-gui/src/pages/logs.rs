//! Server logs page — not yet implemented.

use leptos::prelude::*;

use crate::components::{Card, Header, Sidebar, ToastContainer};
use crate::state::AppState;

/// Server logs page.
///
/// Runtime log viewing is not yet implemented. Strix logs to stdout/stderr
/// and users should configure log shipping for production use.
#[component]
pub fn Logs() -> impl IntoView {
    let _app_state = expect_context::<AppState>();

    view! {
        <div class="flex flex-col min-h-screen">
            <Header />
            <div class="flex flex-1">
                <Sidebar />
                <main class="flex-1 p-8 bg-slate-900">
                    <div class="max-w-7xl mx-auto">
                        <h1 class="text-2xl font-semibold text-white mb-8">"Server Logs"</h1>

                        <Card>
                            <div class="text-center py-16">
                                <div class="mx-auto text-slate-500 mb-4">
                                    <svg class="w-12 h-12 mx-auto" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"/>
                                    </svg>
                                </div>
                                <h3 class="text-lg font-medium text-white mb-2">"Log Viewer Not Yet Implemented"</h3>
                                <p class="text-slate-400 max-w-md mx-auto">
                                    "Runtime log viewing is planned for a future release. Strix currently logs to stdout/stderr."
                                </p>
                            </div>
                        </Card>

                        <div class="mt-6 bg-blue-900/30 border border-blue-700 rounded-lg p-4">
                            <div class="flex">
                                <div class="flex-shrink-0">
                                    <svg class="w-5 h-5 text-blue-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"/>
                                    </svg>
                                </div>
                                <div class="ml-3">
                                    <p class="text-sm text-blue-300">
                                        "For production deployments, configure log shipping to your preferred logging service (e.g., Loki, Elasticsearch, CloudWatch). Use "
                                        <code class="bg-blue-900/50 px-1 rounded">"--log-json"</code>
                                        " for structured JSON output."
                                    </p>
                                </div>
                            </div>
                        </div>
                    </div>
                </main>
            </div>
            <ToastContainer />
        </div>
    }
}
