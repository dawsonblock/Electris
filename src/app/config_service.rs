use anyhow::{anyhow, bail, Context, Result};
use electro_core::config::credentials::{credentials_path, load_credentials_file};
use electro_core::types::model_registry::available_models_for_provider;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderSelection {
    pub provider: String,
    pub model: String,
    pub keys: Vec<String>,
    pub base_url: Option<String>,
}

pub fn current_provider_summary(provider: Option<&str>, model: Option<&str>) -> String {
    match (provider, model) {
        (Some(provider), Some(model)) => format!("current model: {provider}:{model}"),
        _ => "no agent initialized".to_string(),
    }
}

pub fn list_available_models(
    current_provider: Option<&str>,
    current_model: Option<&str>,
) -> Vec<String> {
    let mut entries = Vec::new();
    let saved_credentials = load_credentials_file();
    let saved_active = saved_credentials.as_ref().and_then(|creds| {
        creds
            .providers
            .iter()
            .find(|provider| provider.name == creds.active)
            .map(|provider| (provider.name.clone(), provider.model.clone()))
    });

    if let Some(creds) = saved_credentials {
        for provider in creds.providers {
            extend_provider_models(&mut entries, &provider.name, Some(&provider.model));
        }
    }

    #[cfg(feature = "codex-oauth")]
    if electro_codex_oauth::TokenStore::exists() {
        extend_provider_models(&mut entries, "openai-codex", None);
    }

    if entries.is_empty() {
        if let Some(provider) = current_provider {
            extend_provider_models(&mut entries, provider, current_model);
        } else if let Some((provider, model)) = saved_active.as_ref() {
            extend_provider_models(&mut entries, provider, Some(model));
        }
    }

    entries.sort();
    entries.dedup();
    entries
}

pub fn parse_model_target(raw: &str) -> Result<(String, String)> {
    let trimmed = raw.trim();
    let (provider, model) = trimmed
        .split_once(':')
        .ok_or_else(|| anyhow!("model target must be in format provider:model"))?;
    if provider.is_empty() || model.is_empty() {
        bail!("model target must be in format provider:model");
    }
    Ok((provider.to_string(), model.to_string()))
}

pub fn resolve_provider_selection(provider: &str, model: &str) -> Result<ProviderSelection> {
    validate_provider_model(provider, model)?;

    #[cfg(feature = "codex-oauth")]
    if provider == "openai-codex" {
        if electro_codex_oauth::TokenStore::exists() {
            return Ok(ProviderSelection {
                provider: provider.to_string(),
                model: model.to_string(),
                keys: Vec::new(),
                base_url: None,
            });
        }
    }

    let creds = load_credentials_file()
        .ok_or_else(|| anyhow!("no saved credentials found for provider '{provider}'"))?;
    let entry = creds
        .providers
        .iter()
        .find(|configured| configured.name == provider)
        .ok_or_else(|| anyhow!("provider '{provider}' is not configured"))?;

    if entry.keys.is_empty() {
        bail!("provider '{provider}' has no saved keys");
    }

    Ok(ProviderSelection {
        provider: provider.to_string(),
        model: model.to_string(),
        keys: entry.keys.clone(),
        base_url: entry.base_url.clone(),
    })
}

pub async fn persist_model_selection(provider: &str, model: &str) -> Result<()> {
    validate_provider_model(provider, model)?;

    #[cfg(feature = "codex-oauth")]
    if provider == "openai-codex" {
        bail!("persisting openai-codex model selection is not supported");
    }

    let mut creds = load_credentials_file()
        .ok_or_else(|| anyhow!("no credentials file found to persist model selection"))?;
    let entry = creds
        .providers
        .iter_mut()
        .find(|configured| configured.name == provider)
        .ok_or_else(|| anyhow!("provider '{provider}' is not configured"))?;

    entry.model = model.to_string();
    creds.active = provider.to_string();

    let content = toml::to_string_pretty(&creds).context("failed to serialize credentials file")?;
    tokio::fs::write(credentials_path(), content)
        .await
        .context("failed to write credentials file")?;
    Ok(())
}

fn validate_provider_model(provider: &str, model: &str) -> Result<()> {
    let available = available_models_for_provider(provider);
    if available.is_empty() {
        bail!("provider '{provider}' is not supported by /model");
    }
    if !available.contains(&model) {
        bail!(
            "model '{model}' is not available for provider '{provider}'. Available models:\n{}",
            available.join("\n")
        );
    }
    Ok(())
}

fn extend_provider_models(
    entries: &mut Vec<String>,
    provider: &str,
    configured_model: Option<&str>,
) {
    let mut provider_entries: Vec<String> = available_models_for_provider(provider)
        .into_iter()
        .map(|model| format!("{provider}:{model}"))
        .collect();

    if provider_entries.is_empty() {
        if let Some(model) = configured_model {
            provider_entries.push(format!("{provider}:{model}"));
        }
    }

    entries.extend(provider_entries);
}

#[cfg(test)]
mod tests {
    use super::{current_provider_summary, list_available_models, parse_model_target};

    #[test]
    fn parse_model_target_requires_provider_and_model() {
        let err = parse_model_target("anthropic").expect_err("missing model should fail");
        assert!(err
            .to_string()
            .contains("model target must be in format provider:model"));
    }

    #[test]
    fn parse_model_target_splits_provider_and_model() {
        let parsed = parse_model_target("openai:gpt-4.1").expect("target should parse");
        assert_eq!(parsed, ("openai".to_string(), "gpt-4.1".to_string()));
    }

    #[test]
    fn summary_uses_provider_and_model() {
        assert_eq!(
            current_provider_summary(Some("anthropic"), Some("claude-sonnet-4-6")),
            "current model: anthropic:claude-sonnet-4-6"
        );
        assert_eq!(current_provider_summary(None, None), "no agent initialized");
    }

    #[test]
    fn list_available_models_falls_back_to_current_provider() {
        let models = list_available_models(Some("anthropic"), Some("claude-sonnet-4-6"));
        assert!(models
            .iter()
            .any(|entry| entry == "anthropic:claude-sonnet-4-6"));
    }
}
