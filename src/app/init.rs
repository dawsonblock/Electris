use anyhow::Result;
use electro_core::paths;
use electro_core::types::config::ElectroConfig;
use electro_core::{Memory, UsageStore, Vault};
use std::sync::Arc;

pub struct CoreStack {
    pub memory: Arc<dyn Memory>,
    pub usage_store: Arc<dyn UsageStore>,
    pub vault: Option<Arc<dyn Vault>>,
    pub setup_tokens: electro_gateway::SetupTokenStore,
    #[allow(dead_code)]
    pub memory_url: String,
}

pub async fn init_core_stack(config: &ElectroConfig) -> Result<CoreStack> {
    // ── Memory backend ──
    let memory_url = config.memory.path.clone().unwrap_or_else(|| {
        let data_dir = paths::electro_home();
        std::fs::create_dir_all(&data_dir).ok();
        format!("sqlite:{}/memory.db?mode=rwc", data_dir.display())
    });

    let memory: Arc<dyn Memory> = Arc::from(
        electro_memory::create_memory_backend(&config.memory.backend, &memory_url).await?,
    );

    // ── Usage store ──
    let usage_store: Arc<dyn UsageStore> =
        Arc::new(electro_memory::SqliteUsageStore::new(&memory_url).await?);

    // ── Vault ──
    let vault: Option<Arc<dyn Vault>> = match electro_vault::LocalVault::new().await {
        Ok(v) => Some(Arc::new(v) as Arc<dyn Vault>),
        Err(e) => {
            tracing::warn!(error = %e, "Vault initialization failed — secure features disabled");
            None
        }
    };

    // ── Setup tokens (OTK) ──
    let setup_tokens = electro_gateway::SetupTokenStore::new();

    Ok(CoreStack {
        memory,
        usage_store,
        vault,
        setup_tokens,
        memory_url,
    })
}

pub async fn check_hive_enabled() -> bool {
    // Try to find config file
    let config_content = std::fs::read_to_string(paths::electro_home().join("config.toml"))
        .ok()
        .or_else(|| std::fs::read_to_string("electro.toml").ok());

    if let Some(content) = config_content {
        #[derive(serde::Deserialize, Default)]
        struct HiveCheck {
            #[serde(default)]
            hive: HiveEnabled,
        }
        #[derive(serde::Deserialize, Default)]
        struct HiveEnabled {
            #[serde(default)]
            enabled: bool,
        }
        toml::from_str::<HiveCheck>(&content)
            .map(|c| c.hive.enabled)
            .unwrap_or(false)
    } else {
        false
    }
}

#[cfg(feature = "hive")]
#[allow(dead_code)]
pub async fn load_hive_config() -> electro_hive::HiveConfig {
    let config_content = std::fs::read_to_string(paths::electro_home().join("config.toml"))
        .ok()
        .or_else(|| std::fs::read_to_string("electro.toml").ok());

    if let Some(content) = config_content {
        #[derive(serde::Deserialize, Default)]
        struct HiveWrapper {
            #[serde(default)]
            hive: electro_hive::HiveConfig,
        }
        toml::from_str::<HiveWrapper>(&content)
            .map(|w| w.hive)
            .unwrap_or_default()
    } else {
        electro_hive::HiveConfig::default()
    }
}

#[cfg(not(feature = "hive"))]
#[allow(dead_code)]
pub async fn load_hive_config() -> () {
    ()
}
