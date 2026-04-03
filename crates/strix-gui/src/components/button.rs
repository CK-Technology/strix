//! Button component with accessibility support.

use leptos::prelude::*;

/// Button variants.
#[derive(Clone, Copy, Default)]
pub enum ButtonVariant {
    #[default]
    Primary,
    Secondary,
    Danger,
    Ghost,
}

/// Button sizes.
#[derive(Clone, Copy, Default)]
pub enum ButtonSize {
    Small,
    #[default]
    Medium,
    Large,
}

/// A styled button with Strix theme and accessibility support.
#[component]
pub fn Button(
    /// Button variant.
    #[prop(optional)]
    variant: ButtonVariant,
    /// Button size.
    #[prop(optional)]
    size: ButtonSize,
    /// Whether the button is disabled.
    #[prop(optional)]
    disabled: bool,
    /// Click handler.
    #[prop(optional)]
    on_click: Option<Box<dyn Fn() + 'static>>,
    /// Accessible label for icon-only buttons.
    #[prop(optional)]
    aria_label: Option<&'static str>,
    /// For toggle buttons, indicates pressed state.
    #[prop(optional)]
    aria_pressed: Option<bool>,
    /// Button type attribute (button, submit, reset).
    #[prop(default = "button")]
    button_type: &'static str,
    /// Button content.
    children: Children,
) -> impl IntoView {
    let base_class = "inline-flex items-center justify-center font-medium rounded-md transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-strix-500 focus-visible:ring-offset-2 focus-visible:ring-offset-navy-900 disabled:opacity-50 disabled:cursor-not-allowed";

    let size_class = match size {
        ButtonSize::Small => "px-3 py-1.5 text-xs",
        ButtonSize::Medium => "px-4 py-2 text-sm",
        ButtonSize::Large => "px-6 py-3 text-base",
    };

    let variant_class = match variant {
        ButtonVariant::Primary => {
            "border border-transparent text-white bg-strix-600 hover:bg-strix-500 active:bg-strix-700"
        }
        ButtonVariant::Secondary => {
            "border border-navy-600 text-slate-200 bg-navy-700 hover:bg-navy-600 hover:border-navy-500 active:bg-navy-800"
        }
        ButtonVariant::Danger => {
            "border border-transparent text-white bg-red-600 hover:bg-red-500 active:bg-red-700"
        }
        ButtonVariant::Ghost => {
            "border border-transparent text-slate-300 hover:text-white hover:bg-navy-700 active:bg-navy-800"
        }
    };

    let class = format!("{} {} {}", base_class, size_class, variant_class);

    view! {
        <button
            type=button_type
            class=class
            disabled=disabled
            aria-label=aria_label
            aria-pressed=aria_pressed.map(|p| if p { "true" } else { "false" })
            on:click=move |_| {
                if let Some(ref handler) = on_click {
                    handler();
                }
            }
        >
            {children()}
        </button>
    }
}

/// An icon-only button with required aria-label.
#[component]
pub fn IconButton(
    /// Required accessible label.
    label: &'static str,
    /// Button variant.
    #[prop(optional)]
    variant: ButtonVariant,
    /// Whether the button is disabled.
    #[prop(optional)]
    disabled: bool,
    /// Click handler.
    #[prop(optional)]
    on_click: Option<Box<dyn Fn() + 'static>>,
    /// Icon content (SVG).
    children: Children,
) -> impl IntoView {
    let base_class = "inline-flex items-center justify-center p-2 rounded-md transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-strix-500 focus-visible:ring-offset-2 focus-visible:ring-offset-navy-900 disabled:opacity-50 disabled:cursor-not-allowed";

    let variant_class = match variant {
        ButtonVariant::Primary => {
            "text-white bg-strix-600 hover:bg-strix-500 active:bg-strix-700"
        }
        ButtonVariant::Secondary => {
            "text-slate-200 bg-navy-700 hover:bg-navy-600 active:bg-navy-800"
        }
        ButtonVariant::Danger => {
            "text-white bg-red-600 hover:bg-red-500 active:bg-red-700"
        }
        ButtonVariant::Ghost => {
            "text-slate-400 hover:text-white hover:bg-navy-700 active:bg-navy-800"
        }
    };

    let class = format!("{} {}", base_class, variant_class);

    view! {
        <button
            type="button"
            class=class
            disabled=disabled
            aria-label=label
            on:click=move |_| {
                if let Some(ref handler) = on_click {
                    handler();
                }
            }
        >
            {children()}
        </button>
    }
}
