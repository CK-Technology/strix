//! Table components with accessibility support.

use leptos::prelude::*;

/// A styled table container with accessibility features.
///
/// - Uses proper `scope="col"` on headers for screen readers
/// - Includes ARIA role="table" for assistive technologies
/// - Caption support for table description
#[component]
pub fn Table(
    /// Column headers.
    headers: Vec<&'static str>,
    /// Optional table caption for accessibility.
    #[prop(optional)]
    caption: Option<&'static str>,
    /// Table rows.
    children: Children,
) -> impl IntoView {
    view! {
        <div class="overflow-x-auto" role="region" aria-label=caption.unwrap_or("Data table") tabindex="0">
            <table class="min-w-full divide-y divide-navy-700" role="table">
                {caption.map(|c| view! {
                    <caption class="sr-only">{c}</caption>
                })}
                <thead class="bg-navy-800">
                    <tr role="row">
                        {headers
                            .into_iter()
                            .map(|h| {
                                view! {
                                    <th
                                        scope="col"
                                        role="columnheader"
                                        class="px-6 py-3 text-left text-xs font-medium text-slate-400 uppercase tracking-wider"
                                    >
                                        {h}
                                    </th>
                                }
                            })
                            .collect_view()}
                    </tr>
                </thead>
                <tbody class="bg-navy-800 divide-y divide-navy-700" role="rowgroup">
                    {children()}
                </tbody>
            </table>
        </div>
    }
}

/// A table row with accessibility support.
#[component]
pub fn TableRow(
    /// Whether this row is selected/active.
    #[prop(optional)]
    selected: bool,
    /// Table cells.
    children: Children,
) -> impl IntoView {
    let class = if selected {
        "bg-strix-900/30 text-slate-100"
    } else {
        "hover:bg-navy-700 text-slate-200"
    };

    view! {
        <tr
            class=class
            role="row"
            aria-selected=selected.then_some("true")
        >
            {children()}
        </tr>
    }
}

/// Screen reader only text helper.
#[component]
pub fn SrOnly(children: Children) -> impl IntoView {
    view! {
        <span class="sr-only">{children()}</span>
    }
}
