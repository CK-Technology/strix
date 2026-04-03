//! OpenID Connect configuration page.

use leptos::prelude::*;

use crate::components::{Card, Header, Sidebar, ToastContainer};

/// OpenID Connect configuration page.
#[component]
pub fn OpenId() -> impl IntoView {
    view! {
        <div class="flex flex-col min-h-screen">
            <Header />
            <div class="flex flex-1">
                <Sidebar />
                <main class="flex-1 p-8 bg-slate-900">
                    <div class="max-w-7xl mx-auto">
                        <h1 class="text-2xl font-semibold text-white mb-8">"OpenID Connect (SSO)"</h1>

                        // Status banner
                        <div class="mb-8 bg-strix-900/30 border border-strix-700 rounded-lg p-4">
                            <div class="flex">
                                <div class="flex-shrink-0">
                                    <svg class="w-5 h-5 text-strix-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"/>
                                    </svg>
                                </div>
                                <div class="ml-3">
                                    <h3 class="text-sm font-medium text-strix-300">"Single Sign-On Ready"</h3>
                                    <p class="mt-1 text-sm text-strix-300/70">
                                        "Strix supports OpenID Connect (OIDC) authentication with Azure AD/Entra ID, Google, and other OIDC providers."
                                    </p>
                                </div>
                            </div>
                        </div>

                        // Supported Providers
                        <div class="mb-8">
                            <Card title="Supported Identity Providers">
                                <div class="grid grid-cols-1 md:grid-cols-3 gap-6">
                                    <ProviderCard
                                        name="Azure AD / Entra ID"
                                        description="Microsoft's enterprise identity service"
                                        icon="azure"
                                    />
                                    <ProviderCard
                                        name="Google"
                                        description="Google Workspace and consumer accounts"
                                        icon="google"
                                    />
                                    <ProviderCard
                                        name="Generic OIDC"
                                        description="Any OpenID Connect compliant provider"
                                        icon="key"
                                    />
                                </div>
                            </Card>
                        </div>

                        // Configuration Guide
                        <div class="mb-8">
                            <Card title="Configuration">
                                <div class="space-y-6">
                                    <div>
                                        <h4 class="text-sm font-medium text-strix-400 mb-2">"Azure AD / Entra ID Setup"</h4>
                                        <ol class="text-sm text-slate-400 space-y-2 list-decimal list-inside">
                                            <li>"Register an application in Azure AD (Microsoft Entra admin center)"</li>
                                            <li>"Set the Redirect URI to: " <code class="bg-slate-700 px-1 rounded">"https://your-strix-domain/auth/callback/azure"</code></li>
                                            <li>"Note the Application (client) ID and Directory (tenant) ID"</li>
                                            <li>"Create a client secret in Certificates & secrets"</li>
                                            <li>"Configure Strix with the OIDC provider settings (see below)"</li>
                                        </ol>
                                    </div>

                                    <div>
                                        <h4 class="text-sm font-medium text-strix-400 mb-2">"Google Setup"</h4>
                                        <ol class="text-sm text-slate-400 space-y-2 list-decimal list-inside">
                                            <li>"Go to Google Cloud Console and create a new project"</li>
                                            <li>"Navigate to APIs & Services > Credentials"</li>
                                            <li>"Create an OAuth 2.0 Client ID (Web application type)"</li>
                                            <li>"Add the Redirect URI: " <code class="bg-slate-700 px-1 rounded">"https://your-strix-domain/auth/callback/google"</code></li>
                                            <li>"Configure Strix with the client ID and secret"</li>
                                        </ol>
                                    </div>
                                </div>
                            </Card>
                        </div>

                        // Environment Variables
                        <Card title="Environment Variables">
                            <p class="text-sm text-slate-400 mb-4">
                                "Configure SSO via environment variables or the configuration file."
                            </p>
                            <div class="overflow-x-auto">
                                <table class="min-w-full divide-y divide-slate-700">
                                    <thead>
                                        <tr>
                                            <th class="px-4 py-3 text-left text-xs font-medium text-slate-400 uppercase tracking-wider">"Variable"</th>
                                            <th class="px-4 py-3 text-left text-xs font-medium text-slate-400 uppercase tracking-wider">"Description"</th>
                                            <th class="px-4 py-3 text-left text-xs font-medium text-slate-400 uppercase tracking-wider">"Example"</th>
                                        </tr>
                                    </thead>
                                    <tbody class="divide-y divide-slate-700">
                                        <EnvVarRow
                                            var="STRIX_OIDC_ISSUER"
                                            desc="OIDC Issuer URL"
                                            example="https://login.microsoftonline.com/{tenant}/v2.0"
                                        />
                                        <EnvVarRow
                                            var="STRIX_OIDC_CLIENT_ID"
                                            desc="OAuth Client ID"
                                            example="your-client-id"
                                        />
                                        <EnvVarRow
                                            var="STRIX_OIDC_CLIENT_SECRET"
                                            desc="OAuth Client Secret"
                                            example="your-client-secret"
                                        />
                                        <EnvVarRow
                                            var="STRIX_OIDC_REDIRECT_URI"
                                            desc="OAuth Callback URL"
                                            example="https://strix.example.com/auth/callback"
                                        />
                                        <EnvVarRow
                                            var="STRIX_OIDC_USERNAME_CLAIM"
                                            desc="Claim to use for username"
                                            example="preferred_username"
                                        />
                                        <EnvVarRow
                                            var="STRIX_OIDC_AUTO_CREATE"
                                            desc="Auto-create users on first login"
                                            example="true"
                                        />
                                    </tbody>
                                </table>
                            </div>
                        </Card>

                        // Features
                        <div class="mt-8">
                            <Card title="SSO Features">
                                <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
                                    <FeatureItem
                                        title="Automatic User Provisioning"
                                        description="Users are automatically created on first SSO login with appropriate permissions."
                                    />
                                    <FeatureItem
                                        title="Group Mapping"
                                        description="Map IdP groups to Strix policies for automatic role assignment."
                                    />
                                    <FeatureItem
                                        title="Session Management"
                                        description="SSO sessions integrate with token-based authentication."
                                    />
                                    <FeatureItem
                                        title="Multiple Providers"
                                        description="Configure multiple identity providers simultaneously."
                                    />
                                </div>
                            </Card>
                        </div>
                    </div>
                </main>
            </div>
            <ToastContainer />
        </div>
    }
}

/// Provider card component.
#[component]
fn ProviderCard(
    name: &'static str,
    description: &'static str,
    icon: &'static str,
) -> impl IntoView {
    let icon_svg = match icon {
        "azure" => view! {
            <svg class="w-10 h-10" viewBox="0 0 24 24" fill="currentColor">
                <path d="M5.483 21.3H24L14.025 4.013l-3.038 8.347 5.836 6.938L5.483 21.3zM13.23 2.7L6.105 8.677 0 19.253h5.505v.014L13.23 2.7z"/>
            </svg>
        }.into_any(),
        "google" => view! {
            <svg class="w-10 h-10" viewBox="0 0 24 24" fill="currentColor">
                <path d="M12.48 10.92v3.28h7.84c-.24 1.84-.853 3.187-1.787 4.133-1.147 1.147-2.933 2.4-6.053 2.4-4.827 0-8.6-3.893-8.6-8.72s3.773-8.72 8.6-8.72c2.6 0 4.507 1.027 5.907 2.347l2.307-2.307C18.747 1.44 16.133 0 12.48 0 5.867 0 .307 5.387.307 12s5.56 12 12.173 12c3.573 0 6.267-1.173 8.373-3.36 2.16-2.16 2.84-5.213 2.84-7.667 0-.76-.053-1.467-.173-2.053H12.48z"/>
            </svg>
        }.into_any(),
        "key" => view! {
            <svg class="w-10 h-10" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 7a2 2 0 012 2m4 0a6 6 0 01-7.743 5.743L11 17H9v2H7v2H4a1 1 0 01-1-1v-2.586a1 1 0 01.293-.707l5.964-5.964A6 6 0 1121 9z"/>
            </svg>
        }.into_any(),
        _ => view! { <div class="w-10 h-10"></div> }.into_any(),
    };

    view! {
        <div class="bg-slate-800/50 rounded-lg p-6 border border-slate-700 text-center">
            <div class="text-strix-400 flex justify-center mb-4">
                {icon_svg}
            </div>
            <h4 class="text-white font-medium mb-2">{name}</h4>
            <p class="text-sm text-slate-400">{description}</p>
        </div>
    }
}

/// Environment variable table row.
#[component]
fn EnvVarRow(
    var: &'static str,
    desc: &'static str,
    example: &'static str,
) -> impl IntoView {
    view! {
        <tr class="hover:bg-slate-800">
            <td class="px-4 py-3 whitespace-nowrap text-sm font-mono text-strix-400">{var}</td>
            <td class="px-4 py-3 text-sm text-slate-400">{desc}</td>
            <td class="px-4 py-3 whitespace-nowrap text-sm font-mono text-slate-500">{example}</td>
        </tr>
    }
}

/// Feature item component.
#[component]
fn FeatureItem(
    title: &'static str,
    description: &'static str,
) -> impl IntoView {
    view! {
        <div class="flex items-start">
            <div class="flex-shrink-0">
                <svg class="w-5 h-5 text-strix-400 mt-0.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7"/>
                </svg>
            </div>
            <div class="ml-3">
                <h5 class="text-sm font-medium text-white">{title}</h5>
                <p class="text-sm text-slate-400">{description}</p>
            </div>
        </div>
    }
}
