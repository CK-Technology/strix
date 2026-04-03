//! Strix Web GUI - A Leptos-based admin console.

mod api;
mod components;
mod pages;
mod state;
mod tenant;

use leptos::prelude::*;
use leptos_router::{
    components::{Route, Router, Routes},
    path,
};

pub use api::ApiClient;
pub use state::AppState;
pub use tenant::{Tenant, load_tenants, save_tenants};

/// Main application component.
#[component]
pub fn App() -> impl IntoView {
    // Initialize panic hook for better error messages
    console_error_panic_hook::set_once();

    // Create global app state
    let app_state = AppState::new();
    provide_context(app_state);

    // Create sidebar state for responsive behavior
    let sidebar_state = components::SidebarState::new();
    provide_context(sidebar_state);

    view! {
        <Router>
            <main class="min-h-screen bg-slate-900">
                <Routes fallback=|| view! { <pages::NotFound /> }>
                    <Route path=path!("/") view=|| view! { <RequireAuth><pages::Dashboard /></RequireAuth> } />
                    <Route path=path!("/login") view=LoginRoute />
                    <Route path=path!("/buckets") view=|| view! { <RequireAuth><pages::Buckets /></RequireAuth> } />
                    <Route path=path!("/buckets/:name") view=|| view! { <RequireAuth><pages::BucketDetail /></RequireAuth> } />
                    <Route path=path!("/users") view=|| view! { <RequireAuth><pages::Users /></RequireAuth> } />
                    <Route path=path!("/users/:username") view=|| view! { <RequireAuth><pages::UserDetail /></RequireAuth> } />
                    <Route path=path!("/groups") view=|| view! { <RequireAuth><pages::Groups /></RequireAuth> } />
                    <Route path=path!("/groups/:name") view=|| view! { <RequireAuth><pages::GroupDetail /></RequireAuth> } />
                    <Route path=path!("/policies") view=|| view! { <RequireAuth><pages::Policies /></RequireAuth> } />
                    <Route path=path!("/openid") view=|| view! { <RequireAuth><pages::OpenId /></RequireAuth> } />
                    <Route path=path!("/access-keys") view=|| view! { <RequireAuth><pages::AccessKeys /></RequireAuth> } />
                    <Route path=path!("/metrics") view=|| view! { <RequireAuth><pages::Metrics /></RequireAuth> } />
                    <Route path=path!("/logs") view=|| view! { <RequireAuth><pages::Logs /></RequireAuth> } />
                    <Route path=path!("/audit") view=|| view! { <RequireAuth><pages::Audit /></RequireAuth> } />
                    <Route path=path!("/events") view=|| view! { <RequireAuth><pages::Events /></RequireAuth> } />
                    <Route path=path!("/configuration") view=|| view! { <RequireAuth><pages::Configuration /></RequireAuth> } />
                    <Route path=path!("/settings") view=|| view! { <RequireAuth><pages::Settings /></RequireAuth> } />
                    <Route path=path!("/billing") view=|| view! { <RequireAuth><pages::BillingExports /></RequireAuth> } />
                    <Route path=path!("/tenants") view=|| view! { <RequireAuth><pages::Tenants /></RequireAuth> } />
                    <Route path=path!("/license") view=|| view! { <RequireAuth><pages::License /></RequireAuth> } />
                </Routes>
            </main>
        </Router>
    }
}

#[component]
fn RequireAuth(children: ChildrenFn) -> impl IntoView {
    let app_state = expect_context::<AppState>();

    Effect::new(move || {
        if !app_state.is_authenticated.get() {
            if let Some(window) = web_sys::window() {
                let current = window.location().pathname().unwrap_or_default();
                if current != "/login" {
                    let _ = window.location().set_href("/login");
                }
            }
        }
    });

    view! {
        {move || {
            if app_state.is_authenticated.get() {
                children()
            } else {
                view! { <></> }.into_any()
            }
        }}
    }
}

#[component]
fn LoginRoute() -> impl IntoView {
    let app_state = expect_context::<AppState>();

    Effect::new(move || {
        if app_state.is_authenticated.get() {
            if let Some(window) = web_sys::window() {
                if window.location().pathname().unwrap_or_default() == "/login" {
                    let _ = window.location().set_href("/");
                }
            }
        }
    });

    view! {
        {move || {
            if app_state.is_authenticated.get() {
                view! { <></> }.into_any()
            } else {
                view! { <pages::Login /> }.into_any()
            }
        }}
    }
}

/// Mount the app to the DOM.
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn main() {
    leptos::mount::mount_to_body(App);
}
