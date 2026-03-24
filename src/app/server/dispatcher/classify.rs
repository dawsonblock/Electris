use electro_core::types::message::InboundMessage;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdminCommand {
    Slash(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InboundKind {
    UserMessage,
    StopCommand,
    AdminCommand(AdminCommand),
    SystemEvent,
}

pub fn classify_inbound(inbound: &InboundMessage) -> InboundKind {
    if inbound.channel == "heartbeat" {
        return InboundKind::SystemEvent;
    }

    let text = inbound.text.as_deref().unwrap_or_default().trim();
    if text.eq_ignore_ascii_case("/stop") {
        return InboundKind::StopCommand;
    }
    if text.starts_with('/') || text.starts_with("enc:v1:") {
        return InboundKind::AdminCommand(AdminCommand::Slash(text.to_string()));
    }

    InboundKind::UserMessage
}
