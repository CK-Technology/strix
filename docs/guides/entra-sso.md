# Entra ID (Azure AD) SSO Setup

This guide walks through configuring Microsoft Entra ID (formerly Azure Active
Directory) as a single sign-on provider for the Strix web console. For the
general OIDC model and how Strix verifies tokens, see [sso-oidc.md](sso-oidc.md).

## App Registration vs. Enterprise Application

You need an **App registration**. This is the OAuth/OIDC client definition where
you configure the redirect URI, client secret, and token claims.

When you create an App registration, Entra automatically creates a matching
**Enterprise application** (the service principal) in your tenant. You only need
to touch the Enterprise application if you want to **restrict who can sign in**
(user/group assignment) or review sign-in logs — it is optional for basic SSO.

| Object | Create it? | Used for |
|--------|-----------|----------|
| **App registration** | Yes (you create this) | Client ID, client secret, redirect URI, token/claim configuration |
| **Enterprise application** | Auto-created | Optional user/group assignment, conditional access, sign-in audit |

## Step 1 — Create the App Registration

1. In the [Azure portal](https://portal.azure.com), go to
   **Entra ID → App registrations → New registration**.
2. **Name:** e.g. `Strix Console`.
3. **Supported account types:** choose based on who signs in:
   - *Accounts in this organizational directory only* (single tenant) — most
     common for internal deployments.
   - *Multitenant* only if you intend to allow external Entra tenants.
4. **Redirect URI:** platform **Web**, value:
   ```
   https://your-strix-console.example.com/api/v1/auth/callback
   ```
   This must match the Strix redirect URI exactly (scheme, host, port, path).
5. Click **Register**.

After registration, record from the **Overview** page:
- **Application (client) ID** → Strix `client_id`
- **Directory (tenant) ID** → used to build the issuer URL

## Step 2 — Create a Client Secret

1. In the App registration, open **Certificates & secrets → Client secrets →
   New client secret**.
2. Add a description and expiry, then **Add**.
3. Copy the secret **Value** immediately (not the Secret ID) → Strix
   `client_secret`. It is shown only once.

> Plan secret rotation before the expiry date; Strix stores the secret encrypted
> at rest and you can update it from the console without a restart.

## Step 3 — API Permissions

**Strix needs no Graph API permissions.** It performs a pure OIDC Authorization
Code flow: it exchanges the code for an **ID token**, verifies the signature
against the tenant JWKS, and reads claims directly from that token. It never
calls Microsoft Graph, so there is nothing to grant admin consent for.

The only scopes Strix requests are the standard OpenID Connect scopes, which the
Microsoft identity platform serves directly (not as delegated Graph
permissions):

| Scope | Type | Admin consent | Provides |
|-------|------|---------------|----------|
| `openid` | OIDC | No | Sign-in, `sub` claim |
| `profile` | OIDC | No | `preferred_username`, `name` |
| `email` | OIDC | No | `email` claim |

The default delegated **Microsoft Graph → User.Read** that Entra auto-adds at
registration is **unused** by Strix — you can leave it or remove it.

You do **not** need application (app-only) permissions, directory roles, or
`User.Read.All`. Add `GroupMember.Read.All` and a groups claim only if a future
deployment maps Entra groups to Strix policies (current builds attach a single
default policy on first login rather than reading group claims).

### Picking the username claim

The Strix username comes from an **ID-token claim**, not a Graph lookup, so the
claim you choose just needs to be present in the token:

| Strix username claim | Required scope | Notes |
|----------------------|----------------|-------|
| `preferred_username` (azure default) | `profile` | Emitted by the v2.0 endpoint for org accounts |
| `email` | `email` | Use when you want email-style usernames |
| `sub` | `openid` | Always present, but opaque/non-human-readable |

## Step 4 — (Optional) Token / Claim Configuration

Strix uses the `preferred_username` claim as the username by default (the
`azure` preset sets this). The v2.0 endpoint includes it for organizational
accounts without extra configuration.

If you want a different username source (for example UPN or email), add an
optional claim under **Token configuration → Add optional claim → ID** and set
the Strix **username claim** accordingly in the console.

## Step 5 — (Optional) Restrict Who Can Sign In

To limit access to specific users/groups:

1. Go to **Entra ID → Enterprise applications → Strix Console → Properties**.
2. Set **Assignment required?** to **Yes**.
3. Under **Users and groups**, assign the allowed users or groups.

Without this, any account in the tenant (per your account-type choice) can
complete SSO; Strix still applies its own auto-provisioning and default-policy
rules.

## Step 6 — Configure Strix

The issuer URL is built from your tenant ID:

```
https://login.microsoftonline.com/<tenant-id>/v2.0
```

### Option A — Environment seed (first boot)

```bash
STRIX_OIDC_ENABLED=true
STRIX_OIDC_PROVIDER=azure
STRIX_OIDC_ISSUER=https://login.microsoftonline.com/<tenant-id>/v2.0
STRIX_OIDC_CLIENT_ID=<application-client-id>
STRIX_OIDC_CLIENT_SECRET=<client-secret-value>
STRIX_OIDC_REDIRECT_URI=https://your-strix-console.example.com/api/v1/auth/callback
```

The `azure` preset defaults the username claim to `preferred_username`. These
variables seed a provider on first boot; the console manages it thereafter.

### Option B — Console (Identity → OpenID)

1. **Add Provider → Azure**. The issuer and claim defaults fill in; supply the
   tenant ID in the issuer URL.
2. Enter display name, **client ID**, and **client secret**.
3. **Test discovery** to confirm
   `https://login.microsoftonline.com/<tenant-id>/v2.0/.well-known/openid-configuration`
   resolves.
4. Save. No restart required.

A **Sign in with Azure AD** button then appears on the login page.

## Verification

1. Open the Strix console login page → click **Sign in with Azure AD**.
2. Authenticate with an Entra account (assigned, if you required assignment).
3. You should land in the console authenticated. With auto-provisioning enabled,
   a Strix user is created from the `preferred_username` claim on first login.

## Troubleshooting

| Symptom | Likely cause |
|---------|--------------|
| `AADSTS50011` redirect URI mismatch | The App registration redirect URI does not exactly match `STRIX_OIDC_REDIRECT_URI`. |
| `AADSTS7000215` invalid client secret | Wrong/expired secret, or the Secret **ID** was copied instead of the **Value**. |
| Login succeeds at Entra but Strix rejects the token | Issuer/audience mismatch — confirm the issuer uses `/v2.0` and the client ID matches. |
| User not created | Auto-provisioning disabled, or `preferred_username` missing — check the claim and `STRIX_OIDC_AUTO_CREATE`. |
| `AADSTS50105` not assigned | Assignment is required on the Enterprise app and the user/group is not assigned. |
