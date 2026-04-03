//! Policy editor component for IAM/bucket policies.

use leptos::prelude::*;

/// Validation result for a policy document.
#[derive(Clone, Default)]
pub struct PolicyValidation {
    /// Whether the policy is valid.
    pub is_valid: bool,
    /// Error message if invalid.
    pub error: Option<String>,
    /// Warnings (valid but potentially problematic).
    pub warnings: Vec<String>,
}

/// Validates a JSON policy document.
pub fn validate_policy(json_str: &str) -> PolicyValidation {
    // Try to parse as JSON first
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(json_str);

    match parsed {
        Err(e) => PolicyValidation {
            is_valid: false,
            error: Some(format!("Invalid JSON: {}", e)),
            warnings: vec![],
        },
        Ok(value) => {
            // Check if it's an object
            if !value.is_object() {
                return PolicyValidation {
                    is_valid: false,
                    error: Some("Policy must be a JSON object".to_string()),
                    warnings: vec![],
                };
            }

            // Safe: we just checked is_object() above
            let Some(obj) = value.as_object() else {
                return PolicyValidation {
                    is_valid: false,
                    error: Some("Policy must be a JSON object".to_string()),
                    warnings: vec![],
                };
            };
            let mut warnings = vec![];

            // Check for required fields
            if !obj.contains_key("Version") {
                warnings.push("Missing 'Version' field (recommended: \"2012-10-17\")".to_string());
            }

            let Some(statement) = obj.get("Statement") else {
                return PolicyValidation {
                    is_valid: false,
                    error: Some("Missing required 'Statement' field".to_string()),
                    warnings,
                };
            };

            // Validate Statement is an array
            let Some(statements) = statement.as_array() else {
                return PolicyValidation {
                    is_valid: false,
                    error: Some("'Statement' must be an array".to_string()),
                    warnings,
                };
            };
            if statements.is_empty() {
                warnings.push("Policy has no statements".to_string());
            }

            // Validate each statement
            for (i, stmt) in statements.iter().enumerate() {
                let Some(stmt_obj) = stmt.as_object() else {
                    return PolicyValidation {
                        is_valid: false,
                        error: Some(format!("Statement {} is not an object", i + 1)),
                        warnings,
                    };
                };

                // Check for Effect
                let Some(effect) = stmt_obj.get("Effect") else {
                    return PolicyValidation {
                        is_valid: false,
                        error: Some(format!("Statement {} missing 'Effect' field", i + 1)),
                        warnings,
                    };
                };
                if let Some(effect_str) = effect.as_str() {
                    if effect_str != "Allow" && effect_str != "Deny" {
                        return PolicyValidation {
                            is_valid: false,
                            error: Some(format!(
                                "Statement {} has invalid Effect '{}' (must be 'Allow' or 'Deny')",
                                i + 1,
                                effect_str
                            )),
                            warnings,
                        };
                    }
                }

                // Check for Action
                if !stmt_obj.contains_key("Action") {
                    return PolicyValidation {
                        is_valid: false,
                        error: Some(format!("Statement {} missing 'Action' field", i + 1)),
                        warnings,
                    };
                }

                // Check for Resource (optional for some policies)
                if !stmt_obj.contains_key("Resource") {
                    warnings.push(format!("Statement {} missing 'Resource' field", i + 1));
                }
            }

            PolicyValidation {
                is_valid: true,
                error: None,
                warnings,
            }
        }
    }
}

/// A JSON policy editor with syntax highlighting and validation.
#[component]
pub fn PolicyEditor(
    /// The policy JSON content.
    value: RwSignal<String>,
    /// Label for the editor.
    #[prop(optional)]
    label: Option<&'static str>,
    /// Placeholder text.
    #[prop(optional)]
    placeholder: Option<&'static str>,
    /// Whether to show validation in real-time.
    #[prop(optional)]
    validate: bool,
    /// Whether the editor is read-only.
    #[prop(optional)]
    readonly: bool,
    /// Number of rows for the textarea.
    #[prop(optional)]
    rows: Option<u32>,
) -> impl IntoView {
    let validation = RwSignal::new(PolicyValidation::default());
    let rows = rows.unwrap_or(15);

    // Validate on change
    if validate {
        Effect::new(move || {
            let content = value.get();
            if content.is_empty() {
                validation.set(PolicyValidation::default());
            } else {
                validation.set(validate_policy(&content));
            }
        });
    }

    let format_json = move |_| {
        let content = value.get();
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Ok(formatted) = serde_json::to_string_pretty(&parsed) {
                value.set(formatted);
            }
        }
    };

    let placeholder_text = placeholder.unwrap_or(r#"{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Action": ["s3:GetObject"],
      "Resource": ["arn:aws:s3:::bucket/*"]
    }
  ]
}"#);

    view! {
        <div class="space-y-2">
            {label.map(|l| view! {
                <label class="block text-sm font-medium text-slate-300">{l}</label>
            })}

            <div class="relative">
                <textarea
                    class=move || {
                        let base = "block w-full px-3 py-2 border rounded-md font-mono text-sm bg-slate-900 text-slate-100 placeholder-slate-500 focus:outline-none focus:ring-2 focus:ring-strix-500 resize-y";
                        let border = if validate {
                            let v = validation.get();
                            if !v.is_valid && v.error.is_some() {
                                "border-red-500"
                            } else if !v.warnings.is_empty() {
                                "border-yellow-500"
                            } else if v.is_valid {
                                "border-green-500"
                            } else {
                                "border-slate-600"
                            }
                        } else {
                            "border-slate-600"
                        };
                        format!("{} {}", base, border)
                    }
                    rows=rows
                    placeholder=placeholder_text
                    readonly=readonly
                    prop:value=move || value.get()
                    on:input=move |ev| value.set(event_target_value(&ev))
                />

                // Format button
                <Show when=move || !readonly>
                    <button
                        type="button"
                        on:click=format_json
                        class="absolute top-2 right-2 px-2 py-1 text-xs bg-slate-700 text-slate-300 rounded hover:bg-slate-600 transition-colors"
                        title="Format JSON"
                    >
                        "Format"
                    </button>
                </Show>
            </div>

            // Validation feedback
            <Show when=move || validate>
                {move || {
                    let v = validation.get();
                    let is_valid = v.is_valid;
                    let has_error = v.error.is_some();
                    let error_msg = v.error.clone();
                    let warnings = v.warnings.clone();

                    view! {
                        <div class="space-y-1">
                            // Error message
                            {error_msg.map(|err| view! {
                                <div class="flex items-start gap-2 text-sm text-red-400">
                                    <svg class="w-4 h-4 mt-0.5 flex-shrink-0" fill="currentColor" viewBox="0 0 20 20">
                                        <path fill-rule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zM8.707 7.293a1 1 0 00-1.414 1.414L8.586 10l-1.293 1.293a1 1 0 101.414 1.414L10 11.414l1.293 1.293a1 1 0 001.414-1.414L11.414 10l1.293-1.293a1 1 0 00-1.414-1.414L10 8.586 8.707 7.293z" clip-rule="evenodd"/>
                                    </svg>
                                    <span>{err}</span>
                                </div>
                            })}

                            // Warnings
                            {warnings.iter().map(|warning| {
                                let w = warning.clone();
                                view! {
                                    <div class="flex items-start gap-2 text-sm text-yellow-400">
                                        <svg class="w-4 h-4 mt-0.5 flex-shrink-0" fill="currentColor" viewBox="0 0 20 20">
                                            <path fill-rule="evenodd" d="M8.257 3.099c.765-1.36 2.722-1.36 3.486 0l5.58 9.92c.75 1.334-.213 2.98-1.742 2.98H4.42c-1.53 0-2.493-1.646-1.743-2.98l5.58-9.92zM11 13a1 1 0 11-2 0 1 1 0 012 0zm-1-8a1 1 0 00-1 1v3a1 1 0 002 0V6a1 1 0 00-1-1z" clip-rule="evenodd"/>
                                        </svg>
                                        <span>{w}</span>
                                    </div>
                                }
                            }).collect_view()}

                            // Success indicator
                            {if is_valid && !has_error && !value.get().is_empty() {
                                Some(view! {
                                    <div class="flex items-center gap-2 text-sm text-green-400">
                                        <svg class="w-4 h-4" fill="currentColor" viewBox="0 0 20 20">
                                            <path fill-rule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zm3.707-9.293a1 1 0 00-1.414-1.414L9 10.586 7.707 9.293a1 1 0 00-1.414 1.414l2 2a1 1 0 001.414 0l4-4z" clip-rule="evenodd"/>
                                        </svg>
                                        <span>"Valid policy document"</span>
                                    </div>
                                })
                            } else {
                                None
                            }}
                        </div>
                    }
                }}
            </Show>
        </div>
    }
}

/// A read-only policy viewer with syntax highlighting.
#[component]
pub fn PolicyViewer(
    /// The policy JSON content.
    value: String,
) -> impl IntoView {
    let formatted = serde_json::from_str::<serde_json::Value>(&value)
        .ok()
        .and_then(|v| serde_json::to_string_pretty(&v).ok())
        .unwrap_or(value);

    view! {
        <pre class="p-4 bg-slate-900 rounded-md border border-slate-700 overflow-x-auto">
            <code class="text-sm font-mono text-slate-100">{formatted}</code>
        </pre>
    }
}
