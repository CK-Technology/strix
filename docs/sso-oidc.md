# SSO & OIDC Integration

Strix supports Single Sign-On (SSO) via OpenID Connect (OIDC) for web console authentication.

## Supported Providers

- Azure AD (Microsoft Entra ID)
- Google Workspace
- Any OIDC-compliant identity provider

## Configuration

### Azure AD Setup

1. Register an application in Azure Portal:
   - Go to Azure Active Directory > App registrations > New registration
   - Name: `Strix Console`
   - Redirect URI: `https://your-strix-server:9001/api/v1/auth/callback`

2. Configure the application:
   - Note the Application (client) ID
   - Create a client secret under Certificates & secrets
   - Note the Directory (tenant) ID

3. Configure Strix:

```bash
# Environment variables
STRIX_OIDC_ENABLED=true
STRIX_OIDC_PROVIDER_URL=https://login.microsoftonline.com/{tenant-id}/v2.0
STRIX_OIDC_CLIENT_ID={client-id}
STRIX_OIDC_CLIENT_SECRET={client-secret}
STRIX_OIDC_REDIRECT_URL=https://your-strix-server:9001/api/v1/auth/callback
```

### Google Workspace Setup

1. Create OAuth credentials in Google Cloud Console:
   - Go to APIs & Services > Credentials
   - Create OAuth client ID
   - Application type: Web application
   - Authorized redirect URIs: `https://your-strix-server:9001/api/v1/auth/callback`

2. Configure Strix:

```bash
STRIX_OIDC_ENABLED=true
STRIX_OIDC_PROVIDER_URL=https://accounts.google.com
STRIX_OIDC_CLIENT_ID={client-id}
STRIX_OIDC_CLIENT_SECRET={client-secret}
STRIX_OIDC_REDIRECT_URL=https://your-strix-server:9001/api/v1/auth/callback
```

### Generic OIDC Provider

For other OIDC providers, ensure they support:
- Authorization Code flow
- `.well-known/openid-configuration` endpoint
- `openid`, `profile`, `email` scopes

```bash
STRIX_OIDC_ENABLED=true
STRIX_OIDC_PROVIDER_URL=https://your-idp.com
STRIX_OIDC_CLIENT_ID={client-id}
STRIX_OIDC_CLIENT_SECRET={client-secret}
STRIX_OIDC_REDIRECT_URL=https://your-strix-server:9001/api/v1/auth/callback
STRIX_OIDC_SCOPES=openid,profile,email
```

## User Mapping

### Automatic User Creation

When SSO is enabled, users are automatically created on first login based on their OIDC claims.

| OIDC Claim | Strix Attribute |
|------------|-----------------|
| `sub` | Internal user ID |
| `preferred_username` or `email` | Username |
| `email` | Email address |
| `name` | Display name |

### Group Mapping

Map OIDC groups to Strix groups for automatic policy assignment:

```bash
# Map OIDC groups to Strix groups
STRIX_OIDC_GROUP_CLAIM=groups
STRIX_OIDC_GROUP_MAPPINGS="azure-admin:strix-admins,azure-users:strix-users"
```

## Admin API Endpoints

### List Identity Providers

```bash
GET /api/v1/identity/providers
```

### Create/Update Provider

```bash
POST /api/v1/identity/providers
{
  "name": "azure-ad",
  "type": "oidc",
  "client_id": "...",
  "client_secret": "...",
  "discovery_url": "https://login.microsoftonline.com/{tenant}/v2.0/.well-known/openid-configuration",
  "enabled": true
}
```

### Delete Provider

```bash
DELETE /api/v1/identity/providers/{name}
```

## CLI Configuration

```bash
# List providers
sx settings get local identity.providers

# Add provider
sx settings set local identity.oidc.azure_ad '{"client_id":"...", "client_secret":"..."}'
```

## Security Considerations

1. **HTTPS Required**: Always use HTTPS for redirect URIs
2. **Client Secret**: Store securely, use environment variables
3. **Token Expiry**: Sessions expire based on OIDC token lifetime
4. **Logout**: Supports OIDC single logout when available

## Troubleshooting

### Login Fails

1. Check redirect URI matches exactly
2. Verify client ID and secret
3. Check OIDC provider logs for errors
4. Ensure required scopes are configured

### User Not Created

1. Verify `preferred_username` or `email` claim exists
2. Check Strix logs for claim parsing errors

### Group Sync Issues

1. Verify `groups` claim is included in tokens
2. Check Azure AD: Token Configuration > Add groups claim
3. For Google: Use Admin SDK for group membership
