//! Settings page.

use leptos::prelude::*;

use crate::api::{SmtpConfigInfo, SmtpConfigPayload};
use crate::components::{Card, Header, LoadingFallback, LoadingSize, Sidebar, ToastContainer};
use crate::state::{AppState, ToastKind};

/// Settings page component.
#[component]
pub fn Settings() -> impl IntoView {
    let app_state = expect_context::<AppState>();
    let app_state2 = app_state.clone();
    let server_info_error = RwSignal::new(Option::<String>::None);
    let config_error = RwSignal::new(Option::<String>::None);

    let server_info = LocalResource::new(move || {
        let api = app_state.api.clone();
        let app_state = app_state.clone();
        async move {
            match api.get_server_info().await {
                Ok(info) => {
                    server_info_error.set(None);
                    Ok(info)
                }
                Err(e) => {
                    app_state.handle_error(&e);
                    let msg = e.to_string();
                    server_info_error.set(Some(msg.clone()));
                    Err(msg)
                }
            }
        }
    });

    let server_config = LocalResource::new(move || {
        let api = app_state2.api.clone();
        let app_state = app_state2.clone();
        async move {
            match api.get_server_config().await {
                Ok(config) => {
                    config_error.set(None);
                    Ok(config)
                }
                Err(e) => {
                    app_state.handle_error(&e);
                    let msg = e.to_string();
                    config_error.set(Some(msg.clone()));
                    Err(msg)
                }
            }
        }
    });

    view! {
        <div class="flex flex-col min-h-screen">
            <Header />
            <div class="flex flex-1">
                <Sidebar />
                <main class="flex-1 p-8 bg-slate-900">
                    <div class="max-w-7xl mx-auto">
                        <h1 class="text-2xl font-semibold text-white mb-8">"Settings"</h1>

                        <div class="space-y-8">
                            <Card title="Server Configuration">
                                <Suspense fallback=|| view! { <LoadingFallback size=LoadingSize::Small /> }>
                                    {move || {
                                        server_info.get().and_then(|info| {
                                            match &*info {
                                                Ok(i) => Some(view! {
                                                <dl class="divide-y divide-slate-700">
                                                    <div class="py-3 flex justify-between text-sm">
                                                        <dt class="text-slate-400">"Server Version"</dt>
                                                        <dd class="text-white">{i.version.clone()}</dd>
                                                    </div>
                                                    <div class="py-3 flex justify-between text-sm">
                                                        <dt class="text-slate-400">"Deployment Mode"</dt>
                                                        <dd class="text-white">{i.mode.clone()}</dd>
                                                    </div>
                                                    <div class="py-3 flex justify-between text-sm">
                                                        <dt class="text-slate-400">"Region"</dt>
                                                        <dd class="text-white">{i.region.clone()}</dd>
                                                    </div>
                                                </dl>
                                                }),
                                                Err(_) => None,
                                            }
                                        })
                                    }}
                                </Suspense>
                                {move || server_info_error.get().map(|e| view! {
                                    <div class="mt-3 rounded-md bg-red-900/40 border border-red-700 p-3 text-sm text-red-200">
                                        {format!("Server info unavailable: {}", e)}
                                    </div>
                                })}
                            </Card>

                            <Card title="API Endpoints">
                                <Suspense fallback=|| view! { <LoadingFallback size=LoadingSize::Small /> }>
                                    {move || {
                                        server_config.get().and_then(|config| {
                                            match &*config {
                                                Ok(c) => Some(view! {
                                                    <div class="space-y-4">
                                                        <div>
                                                            <label class="block text-sm font-medium text-slate-300">"S3 API Endpoint"</label>
                                                            <p class="mt-1 text-sm font-mono bg-slate-700 text-strix-300 p-2 rounded">{c.s3_address.clone()}</p>
                                                        </div>
                                                        <div>
                                                            <label class="block text-sm font-medium text-slate-300">"Admin API Endpoint"</label>
                                                            <p class="mt-1 text-sm font-mono bg-slate-700 text-strix-300 p-2 rounded">{c.console_address.clone()}</p>
                                                        </div>
                                                        <div>
                                                            <label class="block text-sm font-medium text-slate-300">"Metrics Endpoint"</label>
                                                            <p class="mt-1 text-sm font-mono bg-slate-700 text-strix-300 p-2 rounded">{c.metrics_address.clone()}</p>
                                                        </div>
                                                    </div>
                                                }),
                                                Err(_) => None,
                                            }
                                        })
                                    }}
                                </Suspense>
                                {move || config_error.get().map(|e| view! {
                                    <div class="mt-3 rounded-md bg-red-900/40 border border-red-700 p-3 text-sm text-red-200">
                                        {format!("Configuration unavailable: {}", e)}
                                    </div>
                                })}
                            </Card>

                            <SmtpSettings />

                            <Card title="Quick Start">
                                <div class="space-y-4">
                                    <p class="text-sm text-slate-300">
                                        "Configure the Strix CLI (sx) to connect to this server:"
                                    </p>
                                    <pre class="text-sm bg-slate-950 text-strix-300 p-4 rounded-md overflow-x-auto border border-slate-700">
                                        <code>
                                            "sx alias set strix http://localhost:9000 ACCESS_KEY SECRET_KEY\n"
                                            "sx ls strix/\n"
                                            "sx mb strix/my-bucket\n"
                                            "sx cp file.txt strix/my-bucket/"
                                        </code>
                                    </pre>
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

/// Editable SMTP relay form state.
#[derive(Clone, Copy)]
struct SmtpForm {
    enabled: RwSignal<bool>,
    host: RwSignal<String>,
    port: RwSignal<String>,
    username: RwSignal<String>,
    password: RwSignal<String>,
    has_password: RwSignal<bool>,
    from_address: RwSignal<String>,
    from_name: RwSignal<String>,
    use_starttls: RwSignal<bool>,
    alert_on_delivery_failure: RwSignal<bool>,
    send_usage_reports: RwSignal<bool>,
    usage_report_schedule: RwSignal<String>,
    alert_on_audit_events: RwSignal<bool>,
    alert_recipients: RwSignal<String>,
    test_to: RwSignal<String>,
    error: RwSignal<Option<String>>,
}

impl SmtpForm {
    fn new() -> Self {
        Self {
            enabled: RwSignal::new(false),
            host: RwSignal::new(String::new()),
            port: RwSignal::new("587".to_string()),
            username: RwSignal::new(String::new()),
            password: RwSignal::new(String::new()),
            has_password: RwSignal::new(false),
            from_address: RwSignal::new(String::new()),
            from_name: RwSignal::new(String::new()),
            use_starttls: RwSignal::new(true),
            alert_on_delivery_failure: RwSignal::new(false),
            send_usage_reports: RwSignal::new(false),
            usage_report_schedule: RwSignal::new("weekly".to_string()),
            alert_on_audit_events: RwSignal::new(false),
            alert_recipients: RwSignal::new(String::new()),
            test_to: RwSignal::new(String::new()),
            error: RwSignal::new(None),
        }
    }

    /// Populate fields from the loaded configuration.
    fn load(&self, c: &SmtpConfigInfo) {
        self.enabled.set(c.enabled);
        self.host.set(c.host.clone());
        self.port.set(c.port.to_string());
        self.username.set(c.username.clone());
        // Password is write-only; leave blank to preserve the stored value.
        self.password.set(String::new());
        self.has_password.set(c.has_password);
        self.from_address.set(c.from_address.clone());
        self.from_name.set(c.from_name.clone().unwrap_or_default());
        self.use_starttls.set(c.use_starttls);
        self.alert_on_delivery_failure.set(c.alert_on_delivery_failure);
        self.send_usage_reports.set(c.send_usage_reports);
        self.usage_report_schedule.set(c.usage_report_schedule.clone());
        self.alert_on_audit_events.set(c.alert_on_audit_events);
        self.alert_recipients.set(c.alert_recipients.join(", "));
        self.error.set(None);
    }

    /// Build the API payload from current field values, validating inputs.
    fn to_payload(self) -> Result<SmtpConfigPayload, String> {
        let host = self.host.get().trim().to_string();
        let from_address = self.from_address.get().trim().to_string();
        if self.enabled.get() {
            if host.is_empty() {
                return Err("SMTP host is required when sending is enabled".to_string());
            }
            if from_address.is_empty() {
                return Err("From address is required when sending is enabled".to_string());
            }
        }
        let port: u16 = self
            .port
            .get()
            .trim()
            .parse()
            .map_err(|_| "Port must be a number between 1 and 65535".to_string())?;
        if port == 0 {
            return Err("Port must be between 1 and 65535".to_string());
        }
        let from_name = self.from_name.get().trim().to_string();
        let recipients: Vec<String> = self
            .alert_recipients
            .get()
            .split([',', ' ', '\n'])
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();

        Ok(SmtpConfigPayload {
            enabled: self.enabled.get(),
            host,
            port,
            username: self.username.get().trim().to_string(),
            password: self.password.get(),
            from_address,
            from_name: if from_name.is_empty() { None } else { Some(from_name) },
            use_starttls: self.use_starttls.get(),
            alert_on_delivery_failure: self.alert_on_delivery_failure.get(),
            send_usage_reports: self.send_usage_reports.get(),
            usage_report_schedule: self.usage_report_schedule.get(),
            alert_on_audit_events: self.alert_on_audit_events.get(),
            alert_recipients: recipients,
        })
    }
}

/// SMTP relay configuration card (root-only; the API enforces privileges).
#[component]
fn SmtpSettings() -> impl IntoView {
    let app_state = expect_context::<AppState>();
    let api = app_state.api.clone();
    let form = SmtpForm::new();
    let saving = RwSignal::new(false);
    let testing = RwSignal::new(false);

    let config = {
        let api = api.clone();
        LocalResource::new(move || {
            let api = api.clone();
            async move {
                match api.get_smtp_config().await {
                    Ok(c) => {
                        form.load(&c);
                        Ok(())
                    }
                    Err(e) => Err(e.to_string()),
                }
            }
        })
    };

    let on_save = StoredValue::new({
        let api = api.clone();
        let app_state = app_state.clone();
        move || {
            form.error.set(None);
            let payload = match form.to_payload() {
                Ok(p) => p,
                Err(e) => {
                    form.error.set(Some(e));
                    return;
                }
            };
            let api = api.clone();
            let app_state = app_state.clone();
            saving.set(true);
            wasm_bindgen_futures::spawn_local(async move {
                match api.set_smtp_config(payload).await {
                    Ok(()) => {
                        app_state.show_toast("SMTP settings saved".to_string(), ToastKind::Success);
                        // Saving a non-empty password means one is now stored.
                        if !form.password.get().is_empty() {
                            form.has_password.set(true);
                        }
                        form.password.set(String::new());
                    }
                    Err(e) => form.error.set(Some(e.to_string())),
                }
                saving.set(false);
            });
        }
    });

    let on_test = StoredValue::new({
        let api = api.clone();
        let app_state = app_state.clone();
        move || {
            let to = form.test_to.get().trim().to_string();
            let to = if to.is_empty() { None } else { Some(to) };
            let api = api.clone();
            let app_state = app_state.clone();
            testing.set(true);
            wasm_bindgen_futures::spawn_local(async move {
                match api.send_test_email(to).await {
                    Ok(()) => {
                        app_state.show_toast("Test email sent".to_string(), ToastKind::Success);
                    }
                    Err(e) => {
                        app_state.show_toast(
                            format!("Test email failed: {}", e),
                            ToastKind::Error,
                        );
                    }
                }
                testing.set(false);
            });
        }
    });

    let input_class = "mt-1 block w-full px-3 py-2 bg-slate-700 border border-slate-600 rounded-md shadow-sm text-white placeholder-slate-400 focus:outline-none focus:ring-strix-500 focus:border-strix-500 sm:text-sm";
    let check_class = "h-4 w-4 rounded border-slate-600 bg-slate-700 text-strix-600 focus:ring-strix-500";

    view! {
        <Card title="Email (SMTP Relay)">
            <Suspense fallback=|| view! { <LoadingFallback size=LoadingSize::Small /> }>
                {move || {
                    config.get().map(|res| match &*res {
                        Ok(()) => view! {
                            <div class="space-y-4">
                                <p class="text-sm text-slate-400">
                                    "Configure an SMTP relay (e.g. SMTP2Go) to deliver alerts and scheduled reports. "
                                    "The password is write-only and encrypted at rest."
                                </p>

                                <label class="flex items-center gap-2 text-sm text-slate-300">
                                    <input
                                        type="checkbox"
                                        class=check_class
                                        prop:checked=move || form.enabled.get()
                                        on:change=move |ev| form.enabled.set(event_target_checked(&ev))
                                    />
                                    "Enable email sending"
                                </label>

                                <div class="grid grid-cols-1 sm:grid-cols-3 gap-4">
                                    <div class="sm:col-span-2">
                                        <label class="block text-sm font-medium text-slate-300">"SMTP Host"</label>
                                        <input
                                            type="text"
                                            class=input_class
                                            placeholder="mail.smtp2go.com"
                                            prop:value=move || form.host.get()
                                            on:input=move |ev| form.host.set(event_target_value(&ev))
                                        />
                                    </div>
                                    <div>
                                        <label class="block text-sm font-medium text-slate-300">"Port"</label>
                                        <input
                                            type="text"
                                            class=input_class
                                            placeholder="587"
                                            prop:value=move || form.port.get()
                                            on:input=move |ev| form.port.set(event_target_value(&ev))
                                        />
                                    </div>
                                </div>

                                <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
                                    <div>
                                        <label class="block text-sm font-medium text-slate-300">"Username"</label>
                                        <input
                                            type="text"
                                            class=input_class
                                            prop:value=move || form.username.get()
                                            on:input=move |ev| form.username.set(event_target_value(&ev))
                                        />
                                    </div>
                                    <div>
                                        <label class="block text-sm font-medium text-slate-300">"Password"</label>
                                        <input
                                            type="password"
                                            class=input_class
                                            placeholder=move || if form.has_password.get() { "Leave blank to keep current password" } else { "Required" }
                                            prop:value=move || form.password.get()
                                            on:input=move |ev| form.password.set(event_target_value(&ev))
                                        />
                                    </div>
                                </div>

                                <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
                                    <div>
                                        <label class="block text-sm font-medium text-slate-300">"From Address"</label>
                                        <input
                                            type="text"
                                            class=input_class
                                            placeholder="alerts@example.com"
                                            prop:value=move || form.from_address.get()
                                            on:input=move |ev| form.from_address.set(event_target_value(&ev))
                                        />
                                    </div>
                                    <div>
                                        <label class="block text-sm font-medium text-slate-300">"From Name (optional)"</label>
                                        <input
                                            type="text"
                                            class=input_class
                                            placeholder="Strix"
                                            prop:value=move || form.from_name.get()
                                            on:input=move |ev| form.from_name.set(event_target_value(&ev))
                                        />
                                    </div>
                                </div>

                                <label class="flex items-center gap-2 text-sm text-slate-300">
                                    <input
                                        type="checkbox"
                                        class=check_class
                                        prop:checked=move || form.use_starttls.get()
                                        on:change=move |ev| form.use_starttls.set(event_target_checked(&ev))
                                    />
                                    "Use STARTTLS (port 587). Disable for implicit TLS (port 465)."
                                </label>

                                <div>
                                    <label class="block text-sm font-medium text-slate-300">"Alert Recipients"</label>
                                    <input
                                        type="text"
                                        class=input_class
                                        placeholder="ops@example.com, security@example.com"
                                        prop:value=move || form.alert_recipients.get()
                                        on:input=move |ev| form.alert_recipients.set(event_target_value(&ev))
                                    />
                                    <p class="mt-1 text-xs text-slate-400">"Comma-separated. Defaults to the From address when empty."</p>
                                </div>

                                <fieldset class="border-t border-slate-700 pt-4 space-y-3">
                                    <legend class="text-sm font-medium text-slate-300">"Triggers"</legend>
                                    <label class="flex items-center gap-2 text-sm text-slate-300">
                                        <input
                                            type="checkbox"
                                            class=check_class
                                            prop:checked=move || form.alert_on_delivery_failure.get()
                                            on:change=move |ev| form.alert_on_delivery_failure.set(event_target_checked(&ev))
                                        />
                                        "Alert on notification delivery failure"
                                    </label>
                                    <label class="flex items-center gap-2 text-sm text-slate-300">
                                        <input
                                            type="checkbox"
                                            class=check_class
                                            prop:checked=move || form.alert_on_audit_events.get()
                                            on:change=move |ev| form.alert_on_audit_events.set(event_target_checked(&ev))
                                        />
                                        "Alert on security/audit events (denied requests, privileged changes)"
                                    </label>
                                    <div class="flex flex-wrap items-center gap-3">
                                        <label class="flex items-center gap-2 text-sm text-slate-300">
                                            <input
                                                type="checkbox"
                                                class=check_class
                                                prop:checked=move || form.send_usage_reports.get()
                                                on:change=move |ev| form.send_usage_reports.set(event_target_checked(&ev))
                                            />
                                            "Send scheduled storage usage reports"
                                        </label>
                                        <select
                                            class="px-3 py-2 bg-slate-700 border border-slate-600 rounded-md text-white text-sm focus:outline-none focus:ring-strix-500 focus:border-strix-500"
                                            prop:value=move || form.usage_report_schedule.get()
                                            on:change=move |ev| form.usage_report_schedule.set(event_target_value(&ev))
                                        >
                                            <option value="daily">"Daily"</option>
                                            <option value="weekly">"Weekly"</option>
                                        </select>
                                    </div>
                                </fieldset>

                                {move || form.error.get().map(|err| view! {
                                    <div class="p-3 bg-red-900/50 border border-red-700 rounded-md">
                                        <p class="text-sm text-red-300">{err}</p>
                                    </div>
                                })}

                                <div class="flex flex-wrap items-center justify-between gap-3 border-t border-slate-700 pt-4">
                                    <div class="flex items-center gap-2">
                                        <input
                                            type="text"
                                            class="px-3 py-2 bg-slate-700 border border-slate-600 rounded-md text-white text-sm placeholder-slate-400 focus:outline-none focus:ring-strix-500 focus:border-strix-500"
                                            placeholder="test recipient (optional)"
                                            prop:value=move || form.test_to.get()
                                            on:input=move |ev| form.test_to.set(event_target_value(&ev))
                                        />
                                        <button
                                            on:click=move |_| on_test.with_value(|f| f())
                                            prop:disabled=move || testing.get()
                                            class="px-4 py-2 text-sm font-medium text-slate-200 bg-slate-700 border border-slate-600 rounded-md hover:bg-slate-600 disabled:opacity-50"
                                        >
                                            {move || if testing.get() { "Sending…" } else { "Send Test Email" }}
                                        </button>
                                    </div>
                                    <button
                                        on:click=move |_| on_save.with_value(|f| f())
                                        prop:disabled=move || saving.get()
                                        class="px-4 py-2 text-sm font-medium text-white bg-strix-600 rounded-md hover:bg-strix-700 disabled:opacity-50"
                                    >
                                        {move || if saving.get() { "Saving…" } else { "Save SMTP Settings" }}
                                    </button>
                                </div>
                            </div>
                        }.into_any(),
                        Err(_) => view! {
                            <div class="rounded-md bg-slate-700/40 border border-slate-600 p-3 text-sm text-slate-300">
                                "Email configuration is available to root users only."
                            </div>
                        }.into_any(),
                    })
                }}
            </Suspense>
        </Card>
    }
}
