//! Responsive sidebar navigation component.

use leptos::prelude::*;

use super::icon::get_icon_by_name;

/// Shared sidebar state for responsive behavior.
#[derive(Clone, Copy)]
pub struct SidebarState {
    /// Whether sidebar is collapsed (icon-only mode on desktop).
    pub collapsed: RwSignal<bool>,
    /// Whether sidebar is open on mobile (overlay mode).
    pub mobile_open: RwSignal<bool>,
}

impl SidebarState {
    /// Create new sidebar state.
    pub fn new() -> Self {
        Self {
            collapsed: RwSignal::new(false),
            mobile_open: RwSignal::new(false),
        }
    }

    /// Toggle collapsed state.
    pub fn toggle_collapsed(&self) {
        self.collapsed.update(|c| *c = !*c);
    }

    /// Toggle mobile menu.
    pub fn toggle_mobile(&self) {
        self.mobile_open.update(|o| *o = !*o);
    }

    /// Close mobile menu.
    pub fn close_mobile(&self) {
        self.mobile_open.set(false);
    }
}

impl Default for SidebarState {
    fn default() -> Self {
        Self::new()
    }
}

/// Responsive sidebar navigation with accessibility support.
#[component]
pub fn Sidebar() -> impl IntoView {
    let state = expect_context::<SidebarState>();

    // Close mobile menu on navigation
    let close_mobile = move |_| {
        state.close_mobile();
    };

    view! {
        <>
            // Mobile overlay backdrop
            <div
                class="fixed inset-0 bg-black/50 z-40 md:hidden transition-opacity"
                style:display=move || if state.mobile_open.get() { "block" } else { "none" }
                on:click=close_mobile
                aria-hidden="true"
            />

            // Sidebar navigation
            <aside
                class=move || {
                    let base = "bg-navy-800 shadow-lg border-r border-navy-700 min-h-screen flex flex-col transition-all duration-300 z-50";
                    let width = if state.collapsed.get() { "w-16" } else { "w-64" };
                    let mobile = if state.mobile_open.get() {
                        "fixed left-0 top-0"
                    } else {
                        "hidden md:flex fixed md:relative left-0 top-0"
                    };
                    format!("{} {} {}", base, width, mobile)
                }
                role="navigation"
                aria-label="Main navigation"
            >
                // Collapse toggle button (desktop only)
                <div class="hidden md:flex justify-end p-2 border-b border-navy-700">
                    <button
                        on:click=move |_| state.toggle_collapsed()
                        class="p-1.5 rounded-md text-slate-400 hover:text-white hover:bg-navy-700 transition-colors focus-visible:ring-2 focus-visible:ring-strix-500 focus-visible:ring-offset-2 focus-visible:ring-offset-navy-800"
                        title=move || if state.collapsed.get() { "Expand sidebar" } else { "Collapse sidebar" }
                        aria-expanded=move || (!state.collapsed.get()).to_string()
                        aria-label=move || if state.collapsed.get() { "Expand sidebar" } else { "Collapse sidebar" }
                    >
                        <svg
                            class=move || format!("w-5 h-5 transition-transform {}", if state.collapsed.get() { "rotate-180" } else { "" })
                            fill="none"
                            stroke="currentColor"
                            viewBox="0 0 24 24"
                            aria-hidden="true"
                        >
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M11 19l-7-7 7-7m8 14l-7-7 7-7"/>
                        </svg>
                    </button>
                </div>

                // Mobile close button
                <div class="md:hidden flex justify-end p-2 border-b border-navy-700">
                    <button
                        on:click=close_mobile
                        class="p-1.5 rounded-md text-slate-400 hover:text-white hover:bg-navy-700 focus-visible:ring-2 focus-visible:ring-strix-500"
                        aria-label="Close navigation menu"
                    >
                        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"/>
                        </svg>
                    </button>
                </div>

                <nav class="flex-1 px-2 py-4 space-y-1 overflow-y-auto">
                    // Main navigation
                    <NavLink href="/" icon="dashboard" collapsed=state.collapsed>"Dashboard"</NavLink>
                    <NavLink href="/buckets" icon="folder" collapsed=state.collapsed>"Object Browser"</NavLink>

                    // IAM Section
                    <NavSection title="Identity" collapsed=state.collapsed>
                        <NavLink href="/users" icon="users" collapsed=state.collapsed>"Users"</NavLink>
                        <NavLink href="/groups" icon="group" collapsed=state.collapsed>"Groups"</NavLink>
                        <NavLink href="/policies" icon="policy" collapsed=state.collapsed>"Policies"</NavLink>
                        <NavLink href="/openid" icon="key" collapsed=state.collapsed>"OpenID"</NavLink>
                    </NavSection>

                    // Access
                    <NavLink href="/access-keys" icon="access-key" collapsed=state.collapsed>"Access Keys"</NavLink>

                    // Monitoring Section
                    <NavSection title="Monitoring" collapsed=state.collapsed>
                        <NavLink href="/metrics" icon="chart" collapsed=state.collapsed>"Metrics"</NavLink>
                        <NavLink href="/logs" icon="log" collapsed=state.collapsed>"Logs"</NavLink>
                        <NavLink href="/audit" icon="audit" collapsed=state.collapsed>"Audit"</NavLink>
                    </NavSection>

                    // Notifications
                    <NavLink href="/events" icon="bell" collapsed=state.collapsed>"Events"</NavLink>

                    // Configuration
                    <NavSection title="Administrator" collapsed=state.collapsed>
                        <NavLink href="/configuration" icon="config" collapsed=state.collapsed>"Configuration"</NavLink>
                        <NavLink href="/settings" icon="settings" collapsed=state.collapsed>"Settings"</NavLink>
                        <NavLink href="/billing" icon="chart" collapsed=state.collapsed>"Billing Exports"</NavLink>
                        <NavLink href="/tenants" icon="group" collapsed=state.collapsed>"Tenants"</NavLink>
                    </NavSection>

                    // License
                    <NavLink href="/license" icon="license" collapsed=state.collapsed>"License"</NavLink>
                </nav>

                // Version info at bottom
                <div class="px-4 py-3 border-t border-slate-700">
                    <Show when=move || !state.collapsed.get()>
                        <p class="text-xs text-slate-500">"Strix v0.1.0"</p>
                    </Show>
                </div>
            </aside>
        </>
    }
}

/// Collapsible navigation section.
#[component]
fn NavSection(title: &'static str, collapsed: RwSignal<bool>, children: Children) -> impl IntoView {
    let expanded = RwSignal::new(true);

    view! {
        <div class="pt-2">
            <Show
                when=move || !collapsed.get()
                fallback=|| view! { <div class="border-t border-slate-700 my-2"></div> }
            >
                <button
                    class="w-full flex items-center justify-between px-2 py-1 text-xs font-semibold text-slate-500 uppercase tracking-wider hover:text-slate-400"
                    on:click=move |_| expanded.update(|e| *e = !*e)
                >
                    <span>{title}</span>
                    <svg
                        class=move || format!("w-4 h-4 transition-transform {}", if expanded.get() { "rotate-0" } else { "-rotate-90" })
                        fill="none"
                        stroke="currentColor"
                        viewBox="0 0 24 24"
                    >
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7"/>
                    </svg>
                </button>
            </Show>
            <div
                class="mt-1 space-y-1"
                style:display=move || if expanded.get() || collapsed.get() { "block" } else { "none" }
            >
                {children()}
            </div>
        </div>
    }
}

/// Navigation link component.
#[component]
fn NavLink(
    href: &'static str,
    icon: &'static str,
    collapsed: RwSignal<bool>,
    children: Children,
) -> impl IntoView {
    let icon_svg = get_icon_by_name(icon);
    let state = expect_context::<SidebarState>();
    let rendered_children = children();

    let on_click = move |_| {
        state.close_mobile();
    };

    view! {
        <a
            href=href
            on:click=on_click
            class=move || {
                let base = "group flex items-center py-2 text-sm font-medium rounded-md text-slate-300 hover:bg-slate-700 hover:text-white transition-colors";
                let padding = if collapsed.get() { "px-2 justify-center" } else { "px-2" };
                format!("{} {}", base, padding)
            }
            title=move || if collapsed.get() { Some(href) } else { None }
        >
            <span class=move || {
                let base = "text-slate-400 group-hover:text-strix-400 transition-colors";
                let margin = if collapsed.get() { "" } else { "mr-3" };
                format!("{} {}", base, margin)
            }>
                {icon_svg}
            </span>
            <span style:display=move || if collapsed.get() { "none" } else { "inline" }>
                {rendered_children}
            </span>
        </a>
    }
}
