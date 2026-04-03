//! Application state management.

use gloo_storage::{SessionStorage, Storage};
use leptos::prelude::*;

use crate::api::{ApiClient, ApiError};
use crate::components::ConfirmState;

/// Global application state.
#[derive(Clone)]
pub struct AppState {
    /// Whether the user is authenticated.
    pub is_authenticated: RwSignal<bool>,
    /// Current username (if authenticated).
    pub username: RwSignal<Option<String>>,
    /// API client for making requests.
    pub api: ApiClient,
    /// Toast message queue.
    pub toasts: RwSignal<Vec<Toast>>,
    /// Counter for generating unique toast IDs.
    toast_counter: RwSignal<u32>,
    /// Confirmation dialog state.
    pub confirm: ConfirmState,
}

/// Storage key for session data.
const SESSION_KEY: &str = "strix_session";

impl AppState {
    /// Create a new app state, restoring from session storage if available.
    pub fn new() -> Self {
        let stored_session: Option<StoredSession> = SessionStorage::get(SESSION_KEY).ok();

        let is_authenticated = RwSignal::new(stored_session.is_some());
        let username = RwSignal::new(stored_session.as_ref().map(|s| s.username.clone()));

        let api = if let Some(session) = stored_session {
            ApiClient::new_with_token(&session.token)
        } else {
            ApiClient::new()
        };

        Self {
            is_authenticated,
            username,
            api,
            toasts: RwSignal::new(Vec::new()),
            toast_counter: RwSignal::new(0),
            confirm: ConfirmState::new(),
        }
    }

    /// Log in with a JWT token (obtained from successful API login).
    pub fn login(&self, username: String, token: String) {
        // Store in session storage (cleared when browser closes)
        let session = StoredSession {
            username: username.clone(),
            token: token.clone(),
        };
        let _ = SessionStorage::set(SESSION_KEY, &session);

        // Update state
        self.is_authenticated.set(true);
        self.username.set(Some(username));
        self.api.set_token(&token);
    }

    /// Log out.
    pub fn logout(&self) {
        SessionStorage::delete(SESSION_KEY);
        self.is_authenticated.set(false);
        self.username.set(None);
        self.api.clear_token();
    }

    /// Show a toast message. Returns the toast ID for manual dismissal.
    pub fn show_toast(&self, message: String, kind: ToastKind) -> u32 {
        let id = self.toast_counter.get();
        self.toast_counter.update(|c| *c += 1);

        let toast = Toast {
            id,
            message,
            kind,
            duration_ms: kind.default_duration(),
        };

        self.toasts.update(|toasts| {
            toasts.push(toast);
            // Limit queue to 5 toasts
            if toasts.len() > 5 {
                toasts.remove(0);
            }
        });

        id
    }

    /// Show a toast with custom duration (0 = no auto-dismiss).
    pub fn show_toast_with_duration(&self, message: String, kind: ToastKind, duration_ms: u32) -> u32 {
        let id = self.toast_counter.get();
        self.toast_counter.update(|c| *c += 1);

        let toast = Toast {
            id,
            message,
            kind,
            duration_ms,
        };

        self.toasts.update(|toasts| {
            toasts.push(toast);
            if toasts.len() > 5 {
                toasts.remove(0);
            }
        });

        id
    }

    /// Dismiss a specific toast by ID.
    pub fn dismiss_toast(&self, id: u32) {
        self.toasts.update(|toasts| {
            toasts.retain(|t| t.id != id);
        });
    }

    /// Clear all toast messages.
    pub fn clear_toasts(&self) {
        self.toasts.set(Vec::new());
    }

    /// Handle an API error, showing appropriate toast and redirecting on auth failure.
    /// Returns true if the error was handled (e.g., redirect to login).
    pub fn handle_error(&self, error: &ApiError) -> bool {
        match error {
            ApiError::Unauthorized => {
                // Clear session and redirect to login
                self.logout();
                self.show_toast("Session expired. Please log in again.".to_string(), ToastKind::Warning);
                // Navigate to login
                if let Some(window) = web_sys::window() {
                    let _ = window.location().set_href("/login");
                }
                true
            }
            ApiError::RateLimited(secs) => {
                self.show_toast(
                    format!("Too many requests. Please wait {} seconds.", secs),
                    ToastKind::Warning,
                );
                true
            }
            ApiError::Network(msg) => {
                self.show_toast(format!("Network error: {}", msg), ToastKind::Error);
                false
            }
            ApiError::Api(msg) => {
                self.show_toast(msg.clone(), ToastKind::Error);
                false
            }
            ApiError::Parse(msg) => {
                self.show_toast(format!("Error parsing response: {}", msg), ToastKind::Error);
                false
            }
        }
    }

    /// Execute an async API call with automatic error handling.
    /// Shows toast on error and redirects to login on 401.
    pub fn handle_result<T>(&self, result: Result<T, ApiError>) -> Option<T> {
        match result {
            Ok(value) => Some(value),
            Err(ref e) => {
                self.handle_error(e);
                None
            }
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

/// Stored session data (no secrets - only JWT token which expires).
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct StoredSession {
    username: String,
    /// JWT token (expires automatically, no raw secrets stored).
    token: String,
}

/// A toast notification.
#[derive(Clone)]
pub struct Toast {
    /// Unique ID for this toast.
    pub id: u32,
    /// Message to display.
    pub message: String,
    /// Kind of toast (affects styling).
    pub kind: ToastKind,
    /// Auto-dismiss duration in milliseconds (0 = no auto-dismiss).
    pub duration_ms: u32,
}

/// Kind of toast notification.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Success,
    Error,
    Info,
    Warning,
}

impl ToastKind {
    /// Default auto-dismiss duration for each toast kind.
    pub fn default_duration(self) -> u32 {
        match self {
            ToastKind::Success => 3000,  // 3 seconds
            ToastKind::Info => 4000,     // 4 seconds
            ToastKind::Warning => 5000,  // 5 seconds
            ToastKind::Error => 0,       // No auto-dismiss for errors
        }
    }
}
