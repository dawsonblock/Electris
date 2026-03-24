//! Slack channel integration tests

use electro_core::types::config::ChannelConfig;
use electro_core::Channel;

/// Test Slack channel factory creation
#[test]
#[cfg(feature = "slack")]
fn test_slack_channel_factory() {
    use electro_channels::create_channel;
    
    let config = ChannelConfig {
        enabled: true,
        token: Some("xoxb-test-token".to_string()),
        allowlist: vec!["user1".to_string()],
        file_transfer: true,
        max_file_size: Some("20MB".to_string()),
    };
    
    let channel = create_channel("slack", &config, "/tmp".into());
    assert!(channel.is_ok(), "Slack channel should be created with valid config");
    
    let channel = channel.unwrap();
    assert_eq!(channel.name(), "slack");
}

/// Test Slack channel requires token
#[test]
#[cfg(feature = "slack")]
fn test_slack_channel_requires_token() {
    use electro_channels::slack::SlackChannel;
    
    let config = ChannelConfig {
        enabled: true,
        token: None,
        allowlist: Vec::new(),
        file_transfer: false,
        max_file_size: None,
    };
    
    let result = SlackChannel::new(&config);
    assert!(result.is_err(), "Slack channel should require token");
}

/// Test Slack allowlist functionality
#[test]
#[cfg(feature = "slack")]
fn test_slack_allowlist() {
    use electro_channels::slack::SlackChannel;
    
    let config = ChannelConfig {
        enabled: true,
        token: Some("xoxb-test".to_string()),
        allowlist: vec!["U12345".to_string()],
        file_transfer: false,
        max_file_size: None,
    };
    
    let channel = SlackChannel::new(&config).unwrap();
    assert!(channel.is_allowed("U12345"));
    assert!(!channel.is_allowed("U99999"));
}

/// Test empty allowlist denies all (Slack-specific behavior)
#[test]
#[cfg(feature = "slack")]
fn test_slack_empty_allowlist_denies_all() {
    use electro_channels::slack::SlackChannel;
    
    let config = ChannelConfig {
        enabled: true,
        token: Some("xoxb-test".to_string()),
        allowlist: Vec::new(),
        file_transfer: false,
        max_file_size: None,
    };
    
    let channel = SlackChannel::new(&config).unwrap();
    // Slack denies all when allowlist is empty (no one whitelisted yet)
    assert!(!channel.is_allowed("any_user"));
}

/// Test Slack disabled without feature flag
#[test]
#[cfg(not(feature = "slack"))]
fn test_slack_disabled_without_feature() {
    use electro_channels::create_channel;
    
    let config = ChannelConfig {
        enabled: true,
        token: Some("xoxb-test".to_string()),
        allowlist: Vec::new(),
        file_transfer: false,
        max_file_size: None,
    };
    
    let result = create_channel("slack", &config, "/tmp".into());
    assert!(result.is_err());
}

/// Test Slack file transfer configuration
#[test]
#[cfg(feature = "slack")]
fn test_slack_file_transfer_config() {
    use electro_channels::slack::SlackChannel;
    
    let config = ChannelConfig {
        enabled: true,
        token: Some("xoxb-test".to_string()),
        allowlist: Vec::new(),
        file_transfer: true,
        max_file_size: Some("50MB".to_string()), // 50MB for Slack
    };
    
    let channel = SlackChannel::new(&config);
    assert!(channel.is_ok());
}
