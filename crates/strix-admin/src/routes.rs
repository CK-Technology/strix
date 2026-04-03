//! Admin API routes.

use axum::{
    Router, middleware,
    routing::{delete, get, post, put},
};
use std::sync::Arc;

use crate::auth::{auth_middleware, authorize_middleware, csrf_middleware};
use crate::handlers::{self, AdminState};
use strix_iam::IamProvider;

/// Create the admin API router.
///
/// Routes are organized into:
/// - Public routes (no authentication required): /login, /health, /info
/// - Protected routes (require valid JWT token): everything else
pub fn admin_router(state: Arc<AdminState>) -> Router {
    let iam_provider: Arc<dyn IamProvider> = state.iam.clone();

    // Public routes (no auth required)
    let public_routes = Router::new()
        .route("/login", post(handlers::login))
        .route("/login/password", post(handlers::login_with_password))
        .route("/health", get(handlers::health_check))
        .route("/info", get(handlers::get_server_info));

    // Protected routes (require authentication)
    let protected_routes = Router::new()
        // Server config
        .route("/config", get(handlers::get_server_config))
        // Users
        .route("/users", get(handlers::list_users))
        .route("/users", post(handlers::create_user))
        .route("/users/{username}", get(handlers::get_user))
        .route("/users/{username}", delete(handlers::delete_user))
        .route(
            "/users/{username}/status",
            put(handlers::update_user_status),
        )
        // Access keys
        .route(
            "/users/{username}/access-keys",
            get(handlers::list_access_keys),
        )
        .route(
            "/users/{username}/access-keys",
            post(handlers::create_access_key),
        )
        .route(
            "/access-keys/{access_key_id}",
            delete(handlers::delete_access_key),
        )
        .route(
            "/access-keys/{access_key_id}",
            put(handlers::update_access_key),
        )
        // Policies
        .route(
            "/users/{username}/policies",
            get(handlers::list_user_policies),
        )
        .route(
            "/users/{username}/policies",
            post(handlers::attach_user_policy),
        )
        .route(
            "/users/{username}/policies/{policy_name}",
            delete(handlers::detach_user_policy),
        )
        // User groups
        .route("/users/{username}/groups", get(handlers::list_user_groups))
        // Groups
        .route("/groups", get(handlers::list_groups))
        .route("/groups", post(handlers::create_group))
        .route("/groups/{name}", get(handlers::get_group))
        .route("/groups/{name}", delete(handlers::delete_group))
        .route(
            "/groups/{group_name}/members",
            post(handlers::add_user_to_group),
        )
        .route(
            "/groups/{group_name}/members/{username}",
            delete(handlers::remove_user_from_group),
        )
        .route(
            "/groups/{group_name}/policies",
            get(handlers::list_group_policies),
        )
        .route(
            "/groups/{group_name}/policies",
            post(handlers::attach_group_policy),
        )
        .route(
            "/groups/{group_name}/policies/{policy_name}",
            delete(handlers::detach_group_policy),
        )
        // Managed Policies
        .route("/policies", get(handlers::list_managed_policies))
        .route("/policies", post(handlers::create_managed_policy))
        .route("/policies/{name}", get(handlers::get_managed_policy))
        .route("/policies/{name}", delete(handlers::delete_managed_policy))
        // Storage usage
        .route("/usage", get(handlers::get_storage_usage))
        // Tenants
        .route("/tenants", get(handlers::list_tenants))
        .route("/tenants", post(handlers::create_tenant))
        .route("/tenants/{slug}", delete(handlers::delete_tenant))
        // Buckets
        .route("/buckets", get(handlers::list_buckets))
        .route("/buckets", post(handlers::create_bucket))
        .route("/buckets/{name}", get(handlers::get_bucket))
        .route("/buckets/{name}", delete(handlers::delete_bucket))
        .route(
            "/buckets/{name}/versioning",
            get(handlers::get_bucket_versioning),
        )
        .route(
            "/buckets/{name}/versioning",
            put(handlers::set_bucket_versioning),
        )
        // Objects
        .route("/buckets/{bucket}/objects", get(handlers::list_objects))
        .route(
            "/buckets/{bucket}/objects",
            delete(handlers::delete_objects),
        )
        .route(
            "/buckets/{bucket}/objects/{*key}",
            delete(handlers::delete_object),
        )
        // Bucket Policies
        .route("/buckets/{bucket}/policy", get(handlers::get_bucket_policy))
        .route("/buckets/{bucket}/policy", put(handlers::set_bucket_policy))
        .route(
            "/buckets/{bucket}/policy",
            delete(handlers::delete_bucket_policy),
        )
        // Bucket Notifications
        .route(
            "/buckets/{bucket}/notifications",
            get(handlers::get_bucket_notifications),
        )
        .route(
            "/buckets/{bucket}/notifications",
            post(handlers::create_bucket_notification),
        )
        .route(
            "/buckets/{bucket}/notifications/{rule_id}",
            delete(handlers::delete_bucket_notification),
        )
        // Pre-signed URLs
        .route("/presign", post(handlers::generate_presign_url))
        // Audit Log
        .route("/audit", get(handlers::query_audit_log))
        // Policy Simulator
        .route("/simulate-policy", post(handlers::simulate_policy))
        // STS
        .route("/sts/assume-role", post(handlers::assume_role))
        // Middleware is applied in reverse order (last applied runs first)
        // Execution order: audit -> csrf -> auth -> authorize -> handler
        // Apply authorization middleware (runs after auth)
        .route_layer(middleware::from_fn_with_state(
            iam_provider,
            authorize_middleware,
        ))
        // Apply auth middleware (runs after CSRF, before authorize)
        .route_layer(middleware::from_fn_with_state(
            state.auth.clone(),
            auth_middleware,
        ))
        // Apply CSRF middleware (runs after audit)
        .route_layer(middleware::from_fn_with_state(
            state.csrf.clone(),
            csrf_middleware,
        ))
        // Apply audit middleware (runs first)
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            handlers::audit_middleware,
        ));

    // Merge public and protected routes
    public_routes.merge(protected_routes).with_state(state)
}
