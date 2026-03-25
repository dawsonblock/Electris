use spine_core::{Command, Intent};

pub(crate) async fn plan(intent: Intent) -> Command {
    let action = intent
        .payload
        .get("action")
        .and_then(|v| v.as_str())
        .unwrap_or("execute")
        .to_string();
    Command::new(&intent.id, action, intent.payload)
}
