//! 404 Not Found page.

use leptos::prelude::*;

/// Not found page component.
#[component]
pub fn NotFound() -> impl IntoView {
    view! {
        <div class="min-h-screen flex items-center justify-center bg-slate-900">
            <div class="text-center">
                <h1 class="text-6xl font-bold text-slate-600">"404"</h1>
                <p class="mt-4 text-xl text-slate-400">"Page not found"</p>
                <a
                    href="/"
                    class="mt-6 inline-block px-4 py-2 text-sm font-medium text-white bg-strix-600 rounded-md hover:bg-strix-700"
                >
                    "Go to Dashboard"
                </a>
            </div>
        </div>
    }
}
