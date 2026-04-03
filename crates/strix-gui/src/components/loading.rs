//! Loading state components for consistent UI feedback.

use leptos::prelude::*;

/// Size variants for the loading spinner.
#[derive(Clone, Copy, Default)]
pub enum LoadingSize {
    Small,
    #[default]
    Medium,
    Large,
}

impl LoadingSize {
    fn spinner_class(&self) -> &'static str {
        match self {
            LoadingSize::Small => "w-4 h-4",
            LoadingSize::Medium => "w-6 h-6",
            LoadingSize::Large => "w-8 h-8",
        }
    }

    fn text_class(&self) -> &'static str {
        match self {
            LoadingSize::Small => "text-xs",
            LoadingSize::Medium => "text-sm",
            LoadingSize::Large => "text-base",
        }
    }
}

/// A loading spinner component.
#[component]
pub fn LoadingSpinner(
    /// Size of the spinner.
    #[prop(optional)]
    size: LoadingSize,
) -> impl IntoView {
    let spinner_class = size.spinner_class();

    view! {
        <svg
            class=format!("{} animate-spin text-strix-500", spinner_class)
            fill="none"
            viewBox="0 0 24 24"
            aria-hidden="true"
        >
            <circle
                class="opacity-25"
                cx="12"
                cy="12"
                r="10"
                stroke="currentColor"
                stroke-width="4"
            />
            <path
                class="opacity-75"
                fill="currentColor"
                d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
            />
        </svg>
    }
}

/// A consistent loading fallback for Suspense boundaries.
///
/// Use this component as the fallback for `<Suspense>` to provide
/// consistent loading UI across the application.
#[component]
pub fn LoadingFallback(
    /// Optional message to display (defaults to "Loading...").
    #[prop(optional)]
    message: Option<&'static str>,
    /// Size of the loading indicator.
    #[prop(optional)]
    size: LoadingSize,
    /// Whether to center the loading indicator in its container.
    #[prop(optional, default = true)]
    centered: bool,
) -> impl IntoView {
    let message = message.unwrap_or("Loading...");
    let text_class = size.text_class();

    let container_class = if centered {
        "flex items-center justify-center gap-3 p-8"
    } else {
        "flex items-center gap-3 p-4"
    };

    view! {
        <div class=container_class role="status" aria-live="polite">
            <LoadingSpinner size=size />
            <span class=format!("{} text-slate-400", text_class)>{message}</span>
            <span class="sr-only">{message}</span>
        </div>
    }
}

/// A full-page loading state.
#[component]
pub fn PageLoading(
    /// Optional message to display.
    #[prop(optional)]
    message: Option<&'static str>,
) -> impl IntoView {
    let display_message = message.unwrap_or("Loading...");
    view! {
        <div class="flex-1 flex items-center justify-center bg-slate-900 min-h-[400px]">
            <LoadingFallback message=display_message size=LoadingSize::Large />
        </div>
    }
}

/// A card-sized loading skeleton.
#[component]
pub fn CardSkeleton() -> impl IntoView {
    view! {
        <div class="bg-slate-800 rounded-lg p-6 animate-pulse" aria-hidden="true">
            <div class="h-4 bg-slate-700 rounded w-3/4 mb-4"></div>
            <div class="space-y-3">
                <div class="h-3 bg-slate-700 rounded"></div>
                <div class="h-3 bg-slate-700 rounded w-5/6"></div>
            </div>
        </div>
    }
}

/// A table row loading skeleton.
#[component]
pub fn TableRowSkeleton(
    /// Number of columns to render.
    columns: usize,
) -> impl IntoView {
    view! {
        <tr class="animate-pulse" aria-hidden="true">
            {(0..columns).map(|_| view! {
                <td class="px-6 py-4">
                    <div class="h-4 bg-slate-700 rounded w-3/4"></div>
                </td>
            }).collect_view()}
        </tr>
    }
}
