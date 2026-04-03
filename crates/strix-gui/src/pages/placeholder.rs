//! Placeholder pages for features in development.

use leptos::prelude::*;

use crate::components::{Card, Header, Sidebar, ToastContainer};

/// Generic placeholder page component.
#[component]
pub fn PlaceholderPage(
    title: &'static str,
    description: &'static str,
    icon: &'static str,
) -> impl IntoView {
    let icon_svg = match icon {
        "group" => view! {
            <svg class="w-12 h-12" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M17 20h5v-2a3 3 0 00-5.356-1.857M17 20H7m10 0v-2c0-.656-.126-1.283-.356-1.857M7 20H2v-2a3 3 0 015.356-1.857M7 20v-2c0-.656.126-1.283.356-1.857m0 0a5.002 5.002 0 019.288 0M15 7a3 3 0 11-6 0 3 3 0 016 0zm6 3a2 2 0 11-4 0 2 2 0 014 0zM7 10a2 2 0 11-4 0 2 2 0 014 0z"/>
            </svg>
        }.into_any(),
        "policy" => view! {
            <svg class="w-12 h-12" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z"/>
            </svg>
        }.into_any(),
        "key" => view! {
            <svg class="w-12 h-12" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 7a2 2 0 012 2m4 0a6 6 0 01-7.743 5.743L11 17H9v2H7v2H4a1 1 0 01-1-1v-2.586a1 1 0 01.293-.707l5.964-5.964A6 6 0 1121 9z"/>
            </svg>
        }.into_any(),
        "access-key" => view! {
            <svg class="w-12 h-12" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 11V7a4 4 0 118 0m-4 8v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2z"/>
            </svg>
        }.into_any(),
        "chart" => view! {
            <svg class="w-12 h-12" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z"/>
            </svg>
        }.into_any(),
        "log" => view! {
            <svg class="w-12 h-12" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"/>
            </svg>
        }.into_any(),
        "audit" => view! {
            <svg class="w-12 h-12" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2m-6 9l2 2 4-4"/>
            </svg>
        }.into_any(),
        "bell" => view! {
            <svg class="w-12 h-12" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 17h5l-1.405-1.405A2.032 2.032 0 0118 14.158V11a6.002 6.002 0 00-4-5.659V5a2 2 0 10-4 0v.341C7.67 6.165 6 8.388 6 11v3.159c0 .538-.214 1.055-.595 1.436L4 17h5m6 0v1a3 3 0 11-6 0v-1m6 0H9"/>
            </svg>
        }.into_any(),
        "config" => view! {
            <svg class="w-12 h-12" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 6V4m0 2a2 2 0 100 4m0-4a2 2 0 110 4m-6 8a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4m6 6v10m6-2a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4"/>
            </svg>
        }.into_any(),
        "license" => view! {
            <svg class="w-12 h-12" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12l2 2 4-4M7.835 4.697a3.42 3.42 0 001.946-.806 3.42 3.42 0 014.438 0 3.42 3.42 0 001.946.806 3.42 3.42 0 013.138 3.138 3.42 3.42 0 00.806 1.946 3.42 3.42 0 010 4.438 3.42 3.42 0 00-.806 1.946 3.42 3.42 0 01-3.138 3.138 3.42 3.42 0 00-1.946.806 3.42 3.42 0 01-4.438 0 3.42 3.42 0 00-1.946-.806 3.42 3.42 0 01-3.138-3.138 3.42 3.42 0 00-.806-1.946 3.42 3.42 0 010-4.438 3.42 3.42 0 00.806-1.946 3.42 3.42 0 013.138-3.138z"/>
            </svg>
        }.into_any(),
        _ => view! { <div class="w-12 h-12"></div> }.into_any(),
    };

    view! {
        <div class="flex flex-col min-h-screen">
            <Header />
            <div class="flex flex-1">
                <Sidebar />
                <main class="flex-1 p-8 bg-slate-900">
                    <div class="max-w-7xl mx-auto">
                        <h1 class="text-2xl font-semibold text-white mb-8">{title}</h1>

                        <Card>
                            <div class="text-center py-16">
                                <div class="mx-auto text-strix-400 mb-4">
                                    {icon_svg}
                                </div>
                                <h3 class="text-lg font-medium text-white mb-2">{title}</h3>
                                <p class="text-slate-400 max-w-md mx-auto">
                                    {description}
                                </p>
                                <div class="mt-6">
                                    <span class="inline-flex items-center px-3 py-1 rounded-full text-xs font-medium bg-strix-900/50 text-strix-300 border border-strix-700">
                                        "Coming Soon"
                                    </span>
                                </div>
                            </div>
                        </Card>
                    </div>
                </main>
            </div>
            <ToastContainer />
        </div>
    }
}

/// License information page.
#[component]
pub fn License() -> impl IntoView {
    view! {
        <div class="flex flex-col min-h-screen">
            <Header />
            <div class="flex flex-1">
                <Sidebar />
                <main class="flex-1 p-8 bg-slate-900">
                    <div class="max-w-7xl mx-auto">
                        <h1 class="text-2xl font-semibold text-white mb-8">"License"</h1>

                        <Card>
                            <div class="space-y-6">
                                <div class="flex items-center space-x-4">
                                    <div class="flex-shrink-0">
                                        <div class="w-16 h-16 bg-strix-900/50 rounded-lg flex items-center justify-center">
                                            <span class="text-2xl font-bold text-strix-400">"S"</span>
                                        </div>
                                    </div>
                                    <div>
                                        <h2 class="text-xl font-semibold text-white">"Strix Community Edition"</h2>
                                        <p class="text-slate-400">"Free and Open Source"</p>
                                    </div>
                                </div>

                                <div class="border-t border-slate-700 pt-6">
                                    <dl class="grid grid-cols-1 gap-4 sm:grid-cols-2">
                                        <div>
                                            <dt class="text-sm font-medium text-slate-400">"License Type"</dt>
                                            <dd class="mt-1 text-sm text-white">"GNU AGPL v3"</dd>
                                        </div>
                                        <div>
                                            <dt class="text-sm font-medium text-slate-400">"Version"</dt>
                                            <dd class="mt-1 text-sm text-white">"0.1.0"</dd>
                                        </div>
                                        <div>
                                            <dt class="text-sm font-medium text-slate-400">"Features"</dt>
                                            <dd class="mt-1 text-sm text-white">"All features included"</dd>
                                        </div>
                                        <div>
                                            <dt class="text-sm font-medium text-slate-400">"Support"</dt>
                                            <dd class="mt-1 text-sm text-white">"Community support"</dd>
                                        </div>
                                    </dl>
                                </div>

                                <div class="bg-strix-900/30 border border-strix-700 rounded-lg p-4">
                                    <div class="flex">
                                        <div class="flex-shrink-0">
                                            <svg class="w-5 h-5 text-strix-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z"/>
                                            </svg>
                                        </div>
                                        <div class="ml-3">
                                            <h3 class="text-sm font-medium text-strix-300">"No artificial limitations"</h3>
                                            <p class="mt-1 text-sm text-strix-300/70">
                                                "Unlike other S3-compatible storage solutions, Strix provides all features in the community edition. No enterprise vs CE split - everything is free and open source."
                                            </p>
                                        </div>
                                    </div>
                                </div>
                            </div>
                        </Card>
                    </div>
                </main>
            </div>
            <ToastContainer />
        </div>
    }
}
