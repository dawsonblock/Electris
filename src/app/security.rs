use electro_core::types::config::ElectroConfig;

pub fn enforce_security_policy(config: &ElectroConfig) {
    let sec = &config.security;

    // P2.9a — sandbox → browser --no-sandbox gate
    // The browser tool reads ELECTRO_BROWSER_ALLOW_NO_SANDBOX at launch
    // time. We only set it when the operator has explicitly lowered the
    // sandbox policy from the secure default ("mandatory").
    let sandbox_mandatory = matches!(sec.sandbox.trim(), "mandatory" | "strict");
    if !sandbox_mandatory {
        // SAFETY: Current thread is the only runner at startup.
        std::env::set_var("ELECTRO_BROWSER_ALLOW_NO_SANDBOX", "1");
        tracing::warn!(
            sandbox = %sec.sandbox,
            "security.sandbox is not 'mandatory' — browser will launch with --no-sandbox. \
             Change security.sandbox to 'mandatory' to enforce browser sandboxing."
        );
    }

    // P2.9b — auditable flag enforcement
    if !sec.audit_log {
        tracing::warn!(
            "security.audit_log = false — tool call audit logging is DISABLED. \
             Set security.audit_log = true to restore traceability."
        );
    }

    // P2.9b — file_scanning flag enforcement
    if !sec.file_scanning {
        tracing::warn!(
            "security.file_scanning = false — uploaded file scanning is DISABLED. \
             Malicious payloads will not be scanned before being processed by tools."
        );
    }

    // P2.9b — skill_signing flag enforcement
    if sec.skill_signing != "required" {
        tracing::warn!(
            "security.skill_signing is not 'required' — skill/plugin integrity \
             is not verified. Set security.skill_signing = 'required' to enforce signing."
        );
    }
}
