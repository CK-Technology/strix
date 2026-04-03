//! Header component.

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_navigate;

use crate::components::SidebarState;
use crate::state::AppState;

/// Application header with navigation and user menu.
#[component]
pub fn Header() -> impl IntoView {
    let app_state = expect_context::<AppState>();
    let sidebar_state = use_context::<SidebarState>();
    let command_open = RwSignal::new(false);
    let command_query = RwSignal::new(String::new());

    view! {
        <header class="bg-slate-900 shadow-lg border-b border-slate-700">
            <div class="px-4 sm:px-6 lg:px-8">
                <div class="flex justify-between items-center h-16">
                    <div class="flex items-center">
                        // Mobile menu button
                        {move || sidebar_state.map(|state| view! {
                            <button
                                on:click=move |_| state.toggle_mobile()
                                class="md:hidden p-2 mr-2 rounded-md text-slate-400 hover:text-white hover:bg-slate-700"
                                aria-label="Open menu"
                            >
                                <svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 6h16M4 12h16M4 18h16"/>
                                </svg>
                            </button>
                        })}

                        <A href="/" attr:class="flex items-center">
                            <svg class="h-8 w-8 text-strix-400" viewBox="0 0 24 24" fill="currentColor">
                                <path d="M12 2L2 7l10 5 10-5-10-5zM2 17l10 5 10-5M2 12l10 5 10-5"/>
                            </svg>
                            <span class="ml-2 text-xl font-bold text-strix-400">"STRIX"</span>
                        </A>
                    </div>

                    <div class="flex items-center space-x-4">
                        <button
                            class="hidden sm:inline-flex text-xs text-slate-300 bg-slate-800 border border-slate-700 rounded px-2 py-1 hover:border-strix-500"
                            on:click=move |_| command_open.set(true)
                        >
                            "Ctrl+K"
                        </button>
                        <Show when=move || app_state.is_authenticated.get()>
                            <span class="text-sm text-slate-300 hidden sm:inline">
                                {move || app_state.username.get().unwrap_or_default()}
                            </span>
                            <LogoutButton />
                        </Show>
                    </div>
                </div>
            </div>

            <Show when=move || command_open.get()>
                <div class="fixed inset-0 z-50 flex items-start justify-center pt-24 bg-black/50" on:click=move |_| command_open.set(false)>
                    <div class="w-full max-w-xl bg-slate-800 border border-slate-700 rounded-lg p-3" on:click=move |ev| ev.stop_propagation()>
                        <input
                            type="text"
                            class="w-full px-3 py-2 bg-slate-900 border border-slate-700 rounded text-sm text-white"
                            placeholder="Go to: buckets, users, policies, events, audit..."
                            prop:value=move || command_query.get()
                            on:input=move |ev| command_query.set(event_target_value(&ev))
                        />
                        <div class="mt-2 space-y-1 text-sm">
                            <CommandItem label="Dashboard" href="/" q=command_query close=command_open />
                            <CommandItem label="Object Browser" href="/buckets" q=command_query close=command_open />
                            <CommandItem label="Users" href="/users" q=command_query close=command_open />
                            <CommandItem label="Policies" href="/policies" q=command_query close=command_open />
                            <CommandItem label="Access Keys" href="/access-keys" q=command_query close=command_open />
                            <CommandItem label="Events" href="/events" q=command_query close=command_open />
                            <CommandItem label="Audit Logs" href="/audit" q=command_query close=command_open />
                            <CommandItem label="Tenants" href="/tenants" q=command_query close=command_open />
                            <CommandItem label="Billing Exports" href="/billing" q=command_query close=command_open />
                        </div>
                    </div>
                </div>
            </Show>
        </header>
    }
}

#[component]
fn CommandItem(
    label: &'static str,
    href: &'static str,
    q: RwSignal<String>,
    close: RwSignal<bool>,
) -> impl IntoView {
    let visible = move || {
        let query = q.get().to_lowercase();
        query.is_empty() || label.to_lowercase().contains(&query)
    };

    view! {
        <Show when=visible>
            <A href=href attr:class="w-full text-left px-2 py-2 rounded text-slate-200 hover:bg-slate-700 block" on:click=move |_| close.set(false)>
                {label}
            </A>
        </Show>
    }
}

/// Logout button component.
#[component]
fn LogoutButton() -> impl IntoView {
    let app_state = expect_context::<AppState>();
    let navigate = use_navigate();

    let on_logout = move |_| {
        app_state.logout();
        navigate("/login", Default::default());
    };

    view! {
        <button
            on:click=on_logout
            class="text-sm text-slate-400 hover:text-white"
        >
            "Logout"
        </button>
    }
}
