//! Searchable table component with filtering, sorting, and empty state.

use leptos::prelude::*;

/// Configuration for a table column.
#[derive(Clone)]
pub struct TableColumn {
    /// Column header text.
    pub header: &'static str,
    /// Whether this column is sortable.
    pub sortable: bool,
}

impl TableColumn {
    /// Create a new column configuration.
    pub fn new(header: &'static str) -> Self {
        Self {
            header,
            sortable: false,
        }
    }

    /// Make this column sortable.
    pub fn sortable(mut self) -> Self {
        self.sortable = true;
        self
    }
}

/// Sort direction.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum SortDirection {
    #[default]
    None,
    Ascending,
    Descending,
}

impl SortDirection {
    fn cycle(self) -> Self {
        match self {
            SortDirection::None => SortDirection::Ascending,
            SortDirection::Ascending => SortDirection::Descending,
            SortDirection::Descending => SortDirection::None,
        }
    }
}

/// Sort state for a table.
#[derive(Clone, Default)]
pub struct SortState {
    /// Index of the column being sorted (if any).
    pub column: Option<usize>,
    /// Sort direction.
    pub direction: SortDirection,
}

/// Configuration options for SearchableTable.
#[derive(Clone)]
pub struct TableConfig {
    /// Placeholder text for the search input.
    pub search_placeholder: &'static str,
    /// Message to show when no items match the search.
    pub empty_search_message: &'static str,
    /// Message to show when there are no items at all.
    pub empty_message: &'static str,
}

impl Default for TableConfig {
    fn default() -> Self {
        Self {
            search_placeholder: "Search...",
            empty_search_message: "No results found",
            empty_message: "No items",
        }
    }
}

/// A searchable, sortable table header component.
/// Use this with standard Table/TableRow for the body.
#[component]
pub fn SearchableTableHeader(
    /// Column definitions.
    columns: Vec<TableColumn>,
    /// Sort state signal.
    sort_state: RwSignal<SortState>,
    /// Whether sorting is enabled.
    #[prop(optional)]
    sorting_enabled: bool,
) -> impl IntoView {
    let on_sort = move |col_idx: usize| {
        sort_state.update(|state| {
            if state.column == Some(col_idx) {
                state.direction = state.direction.cycle();
                if state.direction == SortDirection::None {
                    state.column = None;
                }
            } else {
                state.column = Some(col_idx);
                state.direction = SortDirection::Ascending;
            }
        });
    };

    view! {
        <thead class="bg-slate-800">
            <tr role="row">
                {columns.iter().enumerate().map(|(idx, col)| {
                    let sortable = col.sortable && sorting_enabled;
                    let header = col.header;

                    view! {
                        <th
                            scope="col"
                            role="columnheader"
                            tabindex=if sortable { Some("0") } else { None }
                            aria-sort=move || {
                                if !sortable { return None; }
                                let sort = sort_state.get();
                                if sort.column == Some(idx) {
                                    match sort.direction {
                                        SortDirection::Ascending => Some("ascending"),
                                        SortDirection::Descending => Some("descending"),
                                        SortDirection::None => None,
                                    }
                                } else {
                                    None
                                }
                            }
                            class=format!(
                                "px-6 py-3 text-left text-xs font-medium text-slate-400 uppercase tracking-wider {}",
                                if sortable { "cursor-pointer hover:text-slate-200 select-none focus-visible:ring-2 focus-visible:ring-strix-500 focus-visible:outline-none" } else { "" }
                            )
                            on:click=move |_| {
                                if sortable {
                                    on_sort(idx);
                                }
                            }
                            on:keydown=move |ev: web_sys::KeyboardEvent| {
                                if sortable && (ev.key() == "Enter" || ev.key() == " ") {
                                    ev.prevent_default();
                                    on_sort(idx);
                                }
                            }
                        >
                            <div class="flex items-center gap-1">
                                <span>{header}</span>
                                {if sortable {
                                    view! {
                                        <span class="text-slate-500">
                                            {move || {
                                                let sort = sort_state.get();
                                                if sort.column == Some(idx) {
                                                    match sort.direction {
                                                        SortDirection::Ascending => view! {
                                                            <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 15l7-7 7 7"/>
                                                            </svg>
                                                        }.into_any(),
                                                        SortDirection::Descending => view! {
                                                            <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7"/>
                                                            </svg>
                                                        }.into_any(),
                                                        SortDirection::None => view! { <span></span> }.into_any(),
                                                    }
                                                } else {
                                                    view! {
                                                        <svg class="w-4 h-4 opacity-30" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M7 16V4m0 0L3 8m4-4l4 4m6 0v12m0 0l4-4m-4 4l-4-4"/>
                                                        </svg>
                                                    }.into_any()
                                                }
                                            }}
                                        </span>
                                    }.into_any()
                                } else {
                                    view! { <span></span> }.into_any()
                                }}
                            </div>
                        </th>
                    }
                }).collect_view()}
            </tr>
        </thead>
    }
}

/// Search input component for tables.
#[component]
pub fn TableSearchInput(
    /// Signal to store the search query.
    query: RwSignal<String>,
    /// Placeholder text.
    #[prop(optional)]
    placeholder: &'static str,
) -> impl IntoView {
    let placeholder = if placeholder.is_empty() {
        "Search..."
    } else {
        placeholder
    };

    view! {
        <div class="relative">
            <div class="absolute inset-y-0 left-0 pl-3 flex items-center pointer-events-none">
                <svg class="h-5 w-5 text-slate-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"/>
                </svg>
            </div>
            <input
                type="text"
                class="block w-full pl-10 pr-3 py-2 border border-slate-600 rounded-md leading-5 bg-slate-700 text-white placeholder-slate-400 focus:outline-none focus:ring-2 focus:ring-strix-500 focus:border-transparent sm:text-sm"
                placeholder=placeholder
                prop:value=move || query.get()
                on:input=move |ev| query.set(event_target_value(&ev))
            />
        </div>
    }
}

/// Empty state component for tables.
#[component]
pub fn TableEmptyState(
    /// Number of columns (for colspan).
    columns: usize,
    /// Message to display.
    message: &'static str,
    /// Optional icon (SVG path).
    #[prop(optional)]
    icon: Option<&'static str>,
) -> impl IntoView {
    view! {
        <tr>
            <td
                colspan=columns.to_string()
                class="px-6 py-12 text-center"
            >
                <div class="flex flex-col items-center">
                    {icon.map(|path| view! {
                        <svg class="w-12 h-12 text-slate-500 mb-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d=path/>
                        </svg>
                    })}
                    <p class="text-slate-400">{message}</p>
                </div>
            </td>
        </tr>
    }
}

/// Results count indicator.
#[component]
pub fn TableResultsCount(
    /// Number of filtered/displayed items.
    shown: usize,
    /// Total number of items.
    total: usize,
    /// Label for the items (e.g., "users", "buckets").
    #[prop(optional)]
    label: &'static str,
) -> impl IntoView {
    let label = if label.is_empty() { "items" } else { label };

    view! {
        <div class="text-sm text-slate-400">
            {if shown == total {
                format!("{} {}", total, label)
            } else {
                format!("Showing {} of {} {}", shown, total, label)
            }}
        </div>
    }
}

// === Responsive Card Components ===

/// A mobile-friendly card representation of a data row.
///
/// Use this as an alternative to TableRow on small screens.
#[component]
pub fn ResponsiveCard(
    /// Card title (typically the primary identifier).
    title: String,
    /// Optional subtitle or description.
    #[prop(optional)]
    subtitle: Option<String>,
    /// Optional status badge.
    #[prop(optional)]
    status: Option<CardStatus>,
    /// Card content - typically key-value pairs.
    children: Children,
) -> impl IntoView {
    view! {
        <div class="bg-navy-800 rounded-lg border border-navy-700 p-4 hover:border-navy-600 transition-colors">
            <div class="flex justify-between items-start mb-3">
                <div>
                    <h3 class="text-sm font-medium text-slate-100">{title}</h3>
                    {subtitle.map(|s| view! {
                        <p class="text-xs text-slate-400 mt-0.5">{s}</p>
                    })}
                </div>
                {status.map(|s| view! {
                    <StatusBadge status=s />
                })}
            </div>
            <div class="space-y-2">
                {children()}
            </div>
        </div>
    }
}

/// A key-value row for use inside ResponsiveCard.
#[component]
pub fn CardField(
    /// Field label.
    label: &'static str,
    /// Field value.
    children: Children,
) -> impl IntoView {
    view! {
        <div class="flex justify-between items-center text-sm">
            <span class="text-slate-400">{label}</span>
            <span class="text-slate-200">{children()}</span>
        </div>
    }
}

/// Card status variants.
#[derive(Clone, Copy)]
pub enum CardStatus {
    Active,
    Inactive,
    Warning,
    Error,
}

/// Status badge component.
#[component]
pub fn StatusBadge(status: CardStatus) -> impl IntoView {
    let (text, class) = match status {
        CardStatus::Active => ("Active", "bg-green-500/20 text-green-400 border-green-500/30"),
        CardStatus::Inactive => ("Inactive", "bg-slate-500/20 text-slate-400 border-slate-500/30"),
        CardStatus::Warning => ("Warning", "bg-yellow-500/20 text-yellow-400 border-yellow-500/30"),
        CardStatus::Error => ("Error", "bg-red-500/20 text-red-400 border-red-500/30"),
    };

    view! {
        <span class=format!("px-2 py-0.5 text-xs font-medium rounded-full border {}", class)>
            {text}
        </span>
    }
}

/// A responsive container that shows a table on large screens and cards on mobile.
///
/// Usage:
/// ```ignore
/// <ResponsiveDataView>
///     <TableView slot>
///         <Table headers=vec!["Name", "Email"]>
///             // table rows...
///         </Table>
///     </TableView>
///     <CardView slot>
///         // cards...
///     </CardView>
/// </ResponsiveDataView>
/// ```
#[component]
pub fn ResponsiveDataView(
    /// Content shown on larger screens (table).
    table_view: Children,
    /// Content shown on mobile (cards).
    card_view: Children,
) -> impl IntoView {
    view! {
        <>
            // Table view - hidden on mobile, shown on md and up
            <div class="hidden md:block">
                {table_view()}
            </div>
            // Card view - shown on mobile, hidden on md and up
            <div class="md:hidden space-y-3">
                {card_view()}
            </div>
        </>
    }
}

/// Empty state for card views.
#[component]
pub fn CardEmptyState(
    /// Message to display.
    message: &'static str,
) -> impl IntoView {
    view! {
        <div class="bg-navy-800 rounded-lg border border-navy-700 p-8 text-center">
            <svg class="w-12 h-12 text-slate-500 mx-auto mb-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M20 13V6a2 2 0 00-2-2H6a2 2 0 00-2 2v7m16 0v5a2 2 0 01-2 2H6a2 2 0 01-2-2v-5m16 0h-2.586a1 1 0 00-.707.293l-2.414 2.414a1 1 0 01-.707.293h-3.172a1 1 0 01-.707-.293l-2.414-2.414A1 1 0 006.586 13H4"/>
            </svg>
            <p class="text-slate-400">{message}</p>
        </div>
    }
}
