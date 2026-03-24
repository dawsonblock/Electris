//! Discord channel integration tests

use electro_core::types::config::ChannelConfig;
use electro_core::Channel;

/// Test Discord channel factory creation
#[test]
#[cfg(feature = "discord")]
fn test_discord_channel_factory() {
    use electro_channels::create_channel;
    
    let config = ChannelConfig {
        enabled: true,
        token: Some("test_token_12345".to_string()),
        allowlist: vec!["user1".to_string()],
        file_transfer: true,
        max_file_size: Some("10MB".to_string()),
    };
    
    let channel = create_channel("discord", &config, "/tmp".into());
    assert!(channel.is_ok(), "Discord channel should be created with valid config");
    
    let channel = channel.unwrap();
    assert_eq!(channel.name(), "discord");
}

/// Test Discord channel without token fails
#[test]
#[cfg(feature = "discord")]
fn test_discord_channel_requires_token() {
    use electro_channels::discord::DiscordChannel;
    
    let config = ChannelConfig {
        enabled: true,
        token: None,
        allowlist: Vec::new(),
        file_transfer: false,
        max_file_size: None,
    };
    
    let result = DiscordChannel::new(&config);
    assert!(result.is_err(), "Discord channel should require token");
}

/// Test allowlist functionality
#[test]
#[cfg(feature = "discord")]
fn test_discord_allowlist() {
    use electro_channels::discord::DiscordChannel;
    
    let config = ChannelConfig {
        enabled: true,
        token: Some("test_token".to_string()),
        allowlist: vec!["allowed_user".to_string()],
        file_transfer: false,
        max_file_size: None,
    };
    
    let channel = DiscordChannel::new(&config).unwrap();
    assert!(channel.is_allowed("allowed_user"));
    assert!(!channel.is_allowed("blocked_user"));
}

/// Test empty allowlist denies all (Discord-specific behavior)
#[test]
#[cfg(feature = "discord")]
fn test_discord_empty_allowlist_denies_all() {
    use electro_channels::discord::DiscordChannel;
    
    let config = ChannelConfig {
        enabled: true,
        token: Some("test_token".to_string()),
        allowlist: Vec::new(),
        file_transfer: false,
        max_file_size: None,
    };
    
    let channel = DiscordChannel::new(&config).unwrap();
    // Discord denies all when allowlist is empty (no one whitelisted yet)
    assert!(!channel.is_allowed("any_user"));
}

/// Test Discord disabled without feature flag
#[test]
#[cfg(not(feature = "discord"))]
fn test_discord_disabled_without_feature() {
    use electro_channels::create_channel;
    
    let config = ChannelConfig {
        enabled: true,
        token: Some("test_token".to_string()),
        allowlist: Vec::new(),
        file_transfer: false,
        max_file_size: None,
    };
    
    let result = create_channel("discord", &config, "/tmp".into());
    assert!(result.is_err());
}
