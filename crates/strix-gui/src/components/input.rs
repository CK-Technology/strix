//! Input component with accessibility support.

use leptos::prelude::*;

/// Input sizes.
#[derive(Clone, Copy, Default)]
pub enum InputSize {
    Small,
    #[default]
    Medium,
    Large,
}

/// A styled input field with Strix theme and accessibility support.
#[component]
pub fn Input(
    /// Unique ID for the input (required for accessibility).
    id: &'static str,
    /// Input label.
    label: &'static str,
    /// Input placeholder.
    #[prop(optional)]
    placeholder: Option<&'static str>,
    /// Input type.
    #[prop(default = "text")]
    input_type: &'static str,
    /// Value signal.
    value: RwSignal<String>,
    /// Whether the input is required.
    #[prop(optional)]
    required: bool,
    /// Whether the input is disabled.
    #[prop(optional)]
    disabled: bool,
    /// Input size.
    #[prop(optional)]
    size: InputSize,
    /// Helper text below input.
    #[prop(optional)]
    helper: Option<&'static str>,
    /// Error message.
    #[prop(optional)]
    error: Option<String>,
) -> impl IntoView {
    let size_class = match size {
        InputSize::Small => "px-2 py-1.5 text-xs",
        InputSize::Medium => "px-3 py-2 text-sm",
        InputSize::Large => "px-4 py-3 text-base",
    };

    let has_error = error.is_some();
    let error_display = error;

    // Generate IDs for describedby elements
    let helper_id = format!("{}-helper", id);
    let error_id = format!("{}-error", id);

    // Build aria-describedby value
    let describedby = if has_error {
        Some(error_id.clone())
    } else if helper.is_some() {
        Some(helper_id.clone())
    } else {
        None
    };

    let input_class = move || {
        let base = format!(
            "mt-1 block w-full {} bg-navy-800 border rounded-md shadow-sm text-slate-100 placeholder-slate-500 transition-colors focus:outline-none focus:ring-2 focus:ring-strix-500 focus:border-strix-500 disabled:opacity-50 disabled:cursor-not-allowed",
            size_class
        );
        if has_error {
            format!("{} border-red-500", base)
        } else {
            format!("{} border-navy-600 hover:border-navy-500", base)
        }
    };

    view! {
        <div>
            <label
                for=id
                class="block text-sm font-medium text-slate-300"
            >
                {label}
                {if required {
                    view! { <span class="text-red-400 ml-1" aria-hidden="true">"*"</span> }.into_any()
                } else {
                    view! { <span></span> }.into_any()
                }}
            </label>
            <input
                id=id
                type=input_type
                class=input_class
                placeholder=placeholder.unwrap_or("")
                required=required
                disabled=disabled
                aria-required=if required { Some("true") } else { None }
                aria-invalid=if has_error { Some("true") } else { None }
                aria-describedby=describedby
                prop:value=move || value.get()
                on:input=move |ev| {
                    value.set(event_target_value(&ev));
                }
            />
            {helper.map(|h| view! {
                <p id=helper_id.clone() class="mt-1 text-xs text-slate-400">{h}</p>
            })}
            {error_display.map(|e| view! {
                <p id=error_id.clone() class="mt-1 text-xs text-red-400" role="alert">{e}</p>
            })}
        </div>
    }
}
