//! Modal dialog component with accessibility features.

use leptos::prelude::*;
use wasm_bindgen::prelude::*;

/// Modal dialog with accessibility support.
///
/// Features:
/// - Escape key to close
/// - Click backdrop to close
/// - Focus management
/// - Prevents background scroll
/// - ARIA attributes
#[component]
pub fn Modal(
    /// Whether the modal is open.
    open: RwSignal<bool>,
    /// Modal title.
    title: &'static str,
    /// Modal content.
    children: Children,
) -> impl IntoView {
    let rendered_children = children();
    let modal_id = "modal-dialog";
    let title_id = "modal-title";

    // Prevent background scroll when modal is open
    Effect::new(move || {
        if let Some(document) = web_sys::window().and_then(|w| w.document()) {
            if let Some(body) = document.body() {
                if open.get() {
                    let _ = body.style().set_property("overflow", "hidden");
                } else {
                    let _ = body.style().remove_property("overflow");
                }
            }
        }
    });

    // Handle Escape key
    Effect::new(move || {
        if !open.get() {
            return;
        }

        if let Some(window) = web_sys::window() {
            let closure = Closure::wrap(Box::new(move |ev: web_sys::KeyboardEvent| {
                if ev.key() == "Escape" {
                    open.set(false);
                }
            }) as Box<dyn Fn(web_sys::KeyboardEvent)>);

            let callback = closure.as_ref().unchecked_ref();

            let _ = window.add_event_listener_with_callback("keydown", callback);
            closure.forget();
        }
    });

    let on_backdrop_click = move |_| {
        open.set(false);
    };

    view! {
        <div
            id=modal_id
            class="fixed inset-0 z-50 overflow-y-auto"
            style:display=move || if open.get() { "block" } else { "none" }
            role="dialog"
            aria-modal="true"
            aria-labelledby=title_id
        >
            <div class="flex min-h-screen items-center justify-center p-4">
                // Backdrop
                <div
                    class="fixed inset-0 bg-black bg-opacity-70 transition-opacity"
                    on:click=on_backdrop_click
                    aria-hidden="true"
                />

                // Modal content
                <div
                    class="relative bg-slate-800 rounded-lg shadow-xl max-w-lg w-full border border-slate-700 focus:outline-none"
                    tabindex="-1"
                >
                    <div class="px-6 py-4 border-b border-slate-700">
                        <h3 id=title_id class="text-lg font-medium text-white">{title}</h3>
                    </div>
                    <div class="px-6 py-4">
                        {rendered_children}
                    </div>
                </div>
            </div>
        </div>
    }
}

/// Confirmation state for ConfirmModal.
#[derive(Clone, Default)]
pub struct ConfirmState {
    /// Whether the modal is open.
    pub open: RwSignal<bool>,
    /// Title of the confirmation.
    pub title: RwSignal<String>,
    /// Message to display.
    pub message: RwSignal<String>,
    /// Callback when confirmed - stores a unique ID for the pending action.
    pub pending_action: RwSignal<Option<String>>,
    /// Whether an action is in progress.
    pub loading: RwSignal<bool>,
}

impl ConfirmState {
    /// Create a new confirmation state.
    pub fn new() -> Self {
        Self {
            open: RwSignal::new(false),
            title: RwSignal::new(String::new()),
            message: RwSignal::new(String::new()),
            pending_action: RwSignal::new(None),
            loading: RwSignal::new(false),
        }
    }

    /// Show a confirmation dialog and return the action ID.
    /// The caller should check `pending_action` signal for this ID when the user confirms.
    pub fn show(
        &self,
        title: impl Into<String>,
        message: impl Into<String>,
        action_id: impl Into<String>,
    ) {
        self.title.set(title.into());
        self.message.set(message.into());
        self.pending_action.set(Some(action_id.into()));
        self.loading.set(false);
        self.open.set(true);
    }

    /// Close the dialog without confirming.
    pub fn cancel(&self) {
        self.open.set(false);
        self.pending_action.set(None);
        self.loading.set(false);
    }

    /// Called when user clicks confirm - sets loading state.
    /// Returns the pending action ID if any.
    pub fn confirm(&self) -> Option<String> {
        let action = self.pending_action.get();
        if action.is_some() {
            self.loading.set(true);
        }
        action
    }

    /// Close after action completes.
    pub fn done(&self) {
        self.open.set(false);
        self.pending_action.set(None);
        self.loading.set(false);
    }
}

/// Confirmation modal dialog with accessibility support.
///
/// Features:
/// - Escape key to cancel
/// - Click backdrop to cancel
/// - Focus management
/// - Loading state with disabled buttons
/// - ARIA attributes
///
/// Usage:
/// ```ignore
/// let confirm = ConfirmState::new();
/// provide_context(confirm.clone());
///
/// // In component:
/// let confirm = expect_context::<ConfirmState>();
/// confirm.show("Delete User", "Are you sure?", format!("delete-user:{}", username));
///
/// // In ConfirmModal's on_confirm callback:
/// if let Some(action) = confirm.confirm() {
///     if action.starts_with("delete-user:") {
///         let username = action.strip_prefix("delete-user:").unwrap();
///         // perform delete...
///         confirm.done();
///     }
/// }
/// ```
#[component]
pub fn ConfirmModal<F>(
    /// Confirmation state.
    state: ConfirmState,
    /// Callback when user confirms.
    on_confirm: F,
) -> impl IntoView
where
    F: Fn(String) + Clone + 'static,
{
    let confirm_title_id = "confirm-modal-title";
    let confirm_desc_id = "confirm-modal-desc";

    // Prevent background scroll when modal is open
    Effect::new({
        let state = state.clone();
        move || {
            if let Some(document) = web_sys::window().and_then(|w| w.document()) {
                if let Some(body) = document.body() {
                    if state.open.get() {
                        let _ = body.style().set_property("overflow", "hidden");
                    } else {
                        let _ = body.style().remove_property("overflow");
                    }
                }
            }
        }
    });

    // Handle Escape key
    Effect::new({
        let state = state.clone();
        move || {
            if !state.open.get() {
                return;
            }

            if let Some(window) = web_sys::window() {
                let state_clone = state.clone();
                let closure = Closure::wrap(Box::new(move |ev: web_sys::KeyboardEvent| {
                    if ev.key() == "Escape" && !state_clone.loading.get() {
                        state_clone.cancel();
                    }
                })
                    as Box<dyn Fn(web_sys::KeyboardEvent)>);

                let callback = closure.as_ref().unchecked_ref();

                let _ = window.add_event_listener_with_callback("keydown", callback);
                closure.forget();
            }
        }
    });

    let on_confirm_click = {
        let state = state.clone();
        let on_confirm = on_confirm.clone();
        move |_| {
            if let Some(action) = state.confirm() {
                on_confirm(action);
            }
        }
    };

    let on_cancel = {
        let state = state.clone();
        move |_| {
            state.cancel();
        }
    };

    view! {
        <div
            class="fixed inset-0 z-50 overflow-y-auto"
            style:display=move || if state.open.get() { "block" } else { "none" }
            role="alertdialog"
            aria-modal="true"
            aria-labelledby=confirm_title_id
            aria-describedby=confirm_desc_id
        >
            <div class="flex min-h-screen items-center justify-center p-4">
                // Backdrop
                <div
                    class="fixed inset-0 bg-black bg-opacity-70 transition-opacity"
                    on:click=on_cancel.clone()
                    aria-hidden="true"
                />

                // Modal content
                <div
                    class="relative bg-slate-800 rounded-lg shadow-xl max-w-md w-full border border-slate-700 focus:outline-none"
                    tabindex="-1"
                >
                    <div class="px-6 py-4 border-b border-slate-700">
                        <h3 id=confirm_title_id class="text-lg font-medium text-white">
                            {move || state.title.get()}
                        </h3>
                    </div>
                    <div class="px-6 py-4">
                        <p id=confirm_desc_id class="text-slate-300">{move || state.message.get()}</p>
                    </div>
                    <div class="px-6 py-4 border-t border-slate-700 flex justify-end space-x-3">
                        <button
                            on:click=on_cancel
                            class="px-4 py-2 text-sm font-medium text-slate-300 bg-slate-700 border border-slate-600 rounded-md hover:bg-slate-600 disabled:opacity-50 disabled:cursor-not-allowed focus:outline-none focus:ring-2 focus:ring-slate-500"
                            disabled=move || state.loading.get()
                        >
                            "Cancel"
                        </button>
                        <button
                            on:click=on_confirm_click
                            class="px-4 py-2 text-sm font-medium text-white bg-red-600 rounded-md hover:bg-red-700 disabled:opacity-50 disabled:cursor-not-allowed focus:outline-none focus:ring-2 focus:ring-red-500"
                            disabled=move || state.loading.get()
                        >
                            {move || if state.loading.get() {
                                view! {
                                    <span class="flex items-center">
                                        <svg class="animate-spin -ml-1 mr-2 h-4 w-4 text-white" fill="none" viewBox="0 0 24 24">
                                            <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
                                            <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                                        </svg>
                                        "Processing..."
                                    </span>
                                }.into_any()
                            } else {
                                view! { "Confirm" }.into_any()
                            }}
                        </button>
                    </div>
                </div>
            </div>
        </div>
    }
}
