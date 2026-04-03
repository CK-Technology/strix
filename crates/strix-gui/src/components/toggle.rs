//! Toggle switch component.

use leptos::prelude::*;

/// A toggle switch for feature flags and settings.
#[component]
pub fn Toggle(
    /// Label text displayed next to the toggle.
    label: &'static str,
    /// Optional description text below the label.
    #[prop(optional)]
    description: Option<&'static str>,
    /// Whether the toggle is checked.
    checked: RwSignal<bool>,
    /// Whether the toggle is disabled.
    #[prop(optional)]
    disabled: bool,
    /// Optional ID for the input element.
    #[prop(optional)]
    id: Option<&'static str>,
) -> impl IntoView {
    let input_id = id.unwrap_or("toggle");

    let toggle = move |_| {
        if !disabled {
            checked.update(|v| *v = !*v);
        }
    };

    let track_class = move || {
        let base = "relative inline-flex h-6 w-11 flex-shrink-0 rounded-full border-2 border-transparent transition-colors duration-200 ease-in-out focus:outline-none focus-visible:ring-2 focus-visible:ring-strix-500 focus-visible:ring-offset-2 focus-visible:ring-offset-navy-900";
        let state = if checked.get() {
            "bg-strix-600"
        } else {
            "bg-navy-600"
        };
        let cursor = if disabled {
            "cursor-not-allowed opacity-50"
        } else {
            "cursor-pointer"
        };
        format!("{} {} {}", base, state, cursor)
    };

    let knob_class = move || {
        let base = "pointer-events-none inline-block h-5 w-5 transform rounded-full bg-white shadow ring-0 transition duration-200 ease-in-out";
        let position = if checked.get() {
            "translate-x-5"
        } else {
            "translate-x-0"
        };
        format!("{} {}", base, position)
    };

    let label_class = if disabled {
        "text-slate-500"
    } else {
        "text-white"
    };

    view! {
        <div class="flex items-start">
            <button
                type="button"
                id=input_id
                role="switch"
                aria-checked=move || checked.get().to_string()
                aria-labelledby=format!("{}-label", input_id)
                disabled=disabled
                class=track_class
                on:click=toggle
            >
                <span aria-hidden="true" class=knob_class></span>
            </button>
            <div class="ml-3">
                <label
                    id=format!("{}-label", input_id)
                    class=format!("text-sm font-medium {}", label_class)
                >
                    {label}
                </label>
                {description.map(|desc| {
                    view! {
                        <p class="text-sm text-slate-400">{desc}</p>
                    }
                })}
            </div>
        </div>
    }
}

/// A compact toggle without label (icon-only or inline use).
#[component]
pub fn ToggleCompact(
    /// Whether the toggle is checked.
    checked: RwSignal<bool>,
    /// Whether the toggle is disabled.
    #[prop(optional)]
    disabled: bool,
    /// Accessible label for screen readers.
    #[prop(optional)]
    aria_label: Option<&'static str>,
) -> impl IntoView {
    let toggle = move |_| {
        if !disabled {
            checked.update(|v| *v = !*v);
        }
    };

    let track_class = move || {
        let base = "relative inline-flex h-5 w-9 flex-shrink-0 rounded-full border-2 border-transparent transition-colors duration-200 ease-in-out focus:outline-none focus-visible:ring-2 focus-visible:ring-strix-500 focus-visible:ring-offset-2 focus-visible:ring-offset-navy-900";
        let state = if checked.get() {
            "bg-strix-600"
        } else {
            "bg-navy-600"
        };
        let cursor = if disabled {
            "cursor-not-allowed opacity-50"
        } else {
            "cursor-pointer"
        };
        format!("{} {} {}", base, state, cursor)
    };

    let knob_class = move || {
        let base = "pointer-events-none inline-block h-4 w-4 transform rounded-full bg-white shadow ring-0 transition duration-200 ease-in-out";
        let position = if checked.get() {
            "translate-x-4"
        } else {
            "translate-x-0"
        };
        format!("{} {}", base, position)
    };

    view! {
        <button
            type="button"
            role="switch"
            aria-checked=move || checked.get().to_string()
            aria-label=aria_label.unwrap_or("Toggle")
            disabled=disabled
            class=track_class
            on:click=toggle
        >
            <span aria-hidden="true" class=knob_class></span>
        </button>
    }
}
