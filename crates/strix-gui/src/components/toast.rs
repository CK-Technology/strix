//! Toast notification component with queue support.

use leptos::prelude::*;
use wasm_bindgen::prelude::*;

use crate::state::{AppState, Toast, ToastKind};

/// Container for toast notifications.
/// Displays toasts in a stack from bottom-right, with auto-dismiss support.
#[component]
pub fn ToastContainer() -> impl IntoView {
    let app_state = expect_context::<AppState>();

    view! {
        <div class="fixed bottom-4 right-4 z-50 flex flex-col-reverse gap-2 max-w-sm">
            <For
                each=move || app_state.toasts.get()
                key=|toast| toast.id
                children=move |toast| {
                    view! { <ToastItem toast=toast /> }
                }
            />
        </div>
    }
}

/// Individual toast item with auto-dismiss timer.
#[component]
fn ToastItem(toast: Toast) -> impl IntoView {
    let app_state = expect_context::<AppState>();
    let toast_id = toast.id;
    let duration = toast.duration_ms;

    // Set up auto-dismiss timer (runs once on mount)
    if duration > 0 {
        let app_state_timer = app_state.clone();
        request_animation_frame(move || {
            if let Some(window) = web_sys::window() {
                let closure = Closure::once(Box::new(move || {
                    app_state_timer.dismiss_toast(toast_id);
                }) as Box<dyn FnOnce()>);

                let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                    closure.as_ref().unchecked_ref(),
                    duration as i32,
                );
                closure.forget();
            }
        });
    }

    let (bg_class, icon) = match toast.kind {
        ToastKind::Success => (
            "bg-green-600 border-green-500",
            view! {
                <svg class="w-5 h-5 text-green-200" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7"/>
                </svg>
            }.into_any(),
        ),
        ToastKind::Error => (
            "bg-red-600 border-red-500",
            view! {
                <svg class="w-5 h-5 text-red-200" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"/>
                </svg>
            }.into_any(),
        ),
        ToastKind::Info => (
            "bg-blue-600 border-blue-500",
            view! {
                <svg class="w-5 h-5 text-blue-200" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"/>
                </svg>
            }.into_any(),
        ),
        ToastKind::Warning => (
            "bg-yellow-600 border-yellow-500",
            view! {
                <svg class="w-5 h-5 text-yellow-200" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"/>
                </svg>
            }.into_any(),
        ),
    };

    let dismiss = {
        let app_state = app_state.clone();
        move |_| app_state.dismiss_toast(toast_id)
    };

    view! {
        <div
            class=format!(
                "rounded-lg shadow-lg border p-4 text-white {} animate-slide-in",
                bg_class
            )
            role="alert"
        >
            <div class="flex items-start gap-3">
                <div class="flex-shrink-0">
                    {icon}
                </div>
                <div class="flex-1 min-w-0">
                    <p class="text-sm font-medium break-words">{toast.message}</p>
                </div>
                <button
                    on:click=dismiss
                    class="flex-shrink-0 text-white/80 hover:text-white transition-colors"
                    aria-label="Dismiss"
                >
                    <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"/>
                    </svg>
                </button>
            </div>
        </div>
    }
}
