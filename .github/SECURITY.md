# Security Policy

## Supported Versions

Security fixes are provided for the latest released version of TunnelX. If you are using an older build, please upgrade to the latest release before reporting a vulnerability unless the issue also affects the latest version.

## Reporting a Vulnerability

Please do not report security vulnerabilities in public issues.

Report vulnerabilities by opening a private GitHub security advisory for this repository, or contact the maintainer through the repository owner profile if advisories are unavailable.

When reporting, include:

- Affected TunnelX version and operating system
- Provider runtime version, such as frpc or cloudflared, if relevant
- Clear reproduction steps
- Impact and attacker capabilities
- Logs, screenshots, or proof of concept with secrets removed

You should receive an initial response as soon as practical. The fix timeline depends on severity, affected platforms, and release packaging requirements.

## Sensitive Data Handling

TunnelX manages provider-native tunnel configuration and may store sensitive values locally, including:

- frps auth tokens
- tunnel secret keys
- HTTP proxy passwords
- OIDC client secrets
- Cloudflare API tokens
- Cloudflare credentials/config file references
- local certificate/key paths

These values are stored locally in TunnelX's SQLite database or in provider-native runtime files, and may be plaintext depending on the provider's native configuration model. Do not share databases, configs, logs, screenshots, or issue attachments without reviewing and redacting sensitive fields.

## Scope

In scope:

- Vulnerabilities in TunnelX application code
- Unsafe process lifecycle behavior that can leave unmanaged provider runtime processes
- Incorrect handling of local files, credentials, provider configs, or updater artifacts
- Exposure of secrets through logs, UI, generated runtime files, or provider-managed config files

Out of scope:

- Vulnerabilities in provider tools such as frp/frpc or cloudflared unless TunnelX introduces or amplifies the issue
- Issues requiring physical access to an already unlocked user session without privilege escalation
- Social engineering or phishing
- Denial of service against GitHub Actions or release infrastructure
