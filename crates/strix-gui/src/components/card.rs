//! Card component.

use leptos::prelude::*;

/// A card container.
#[component]
pub fn Card(
    /// Optional title.
    #[prop(optional)]
    title: Option<&'static str>,
    /// Card content.
    children: Children,
) -> impl IntoView {
    view! {
        <div class="bg-slate-800 shadow-lg rounded-lg border border-slate-700">
            {title.map(|t| {
                view! {
                    <div class="px-4 py-5 border-b border-slate-700 sm:px-6">
                        <h3 class="text-lg leading-6 font-medium text-white">{t}</h3>
                    </div>
                }
            })}
            <div class="px-4 py-5 sm:p-6">
                {children()}
            </div>
        </div>
    }
}
