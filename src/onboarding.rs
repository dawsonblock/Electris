use std::sync::Arc;
use base64::Engine;
use electro_core::config::credentials::{
    is_placeholder_key, load_credentials_file,
};
use electro_core::types::model_registry::default_model;

/// Validate a provider key by making a minimal API call.
/// Returns Ok(provider_arc) if the key works, Err(message) if not.
pub async fn validate_provider_key(
    config: &electro_core::types::config::ProviderConfig,
) -> anyhow::Result<Arc<dyn electro_core::Provider>, String> {
    let provider = electro_providers::create_provider(config)
        .map_err(|e| format!("Failed to create provider: {}", e))?;
    let provider_arc: Arc<dyn electro_core::Provider> = Arc::from(provider);

    let test_req = electro_core::types::message::CompletionRequest {
        model: config.model.clone().unwrap_or_default(),
        messages: vec![electro_core::types::message::ChatMessage {
            role: electro_core::types::message::Role::User,
            content: electro_core::types::message::MessageContent::Text("Hi".to_string()),
        }],
        tools: Vec::new(),
        max_tokens: Some(1),
        temperature: Some(0.0),
        system: None,
    };

    match provider_arc.complete(test_req).await {
        Ok(_) => Ok(provider_arc),
        Err(e) => {
            let err_str = format!("{}", e);
            let err_lower = err_str.to_lowercase();
            // Auth errors or invalid model errors — reject the reload
            if err_lower.contains("401")
                || err_lower.contains("403")
                || err_lower.contains("unauthorized")
                || err_lower.contains("invalid api key")
                || err_lower.contains("invalid x-api-key")
                || err_lower.contains("authentication")
                || err_lower.contains("permission")
                || err_lower.contains("404")
                || err_lower.contains("not_found")
                || err_lower.contains("model:")
            {
                Err(err_str)
            } else {
                // Non-auth errors (400 max_tokens, 429 rate limit, etc.) mean
                // the key IS valid — the API accepted the auth, just rejected
                // the request params. This is fine for validation.
                tracing::debug!(error = %err_str, "Key validation got non-auth error — key is valid");
                Ok(provider_arc)
            }
        }
    }
}

/// Format a capture timestamp into a human-readable age string.
///
/// Takes an ISO 8601 timestamp and returns e.g. "2h ago", "5m ago", "1d ago".
#[cfg(feature = "browser")]
pub fn format_capture_age(captured_at: &str) -> String {
    let captured = match chrono::DateTime::parse_from_rfc3339(captured_at) {
        Ok(dt) => dt.with_timezone(&chrono::Utc),
        Err(_) => return "unknown".to_string(),
    };
    let elapsed = chrono::Utc::now().signed_duration_since(captured);
    let secs = elapsed.num_seconds();
    if secs < 0 {
        "just now".to_string()
    } else if secs < 60 {
        format!("{}s ago", secs)
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86400)
    }
}

/// Build the onboarding welcome message with a pre-generated setup link.
pub fn onboarding_message_with_link(setup_link: &str) -> String {
    format!(
        "Welcome to ELECTRO!\n\n\
         To get started, open this secure setup link:\n\
         {}\n\n\
         Paste your API key in the form, copy the encrypted blob, \
         and send it back here.\n\n\
         Or just paste your API key directly below — \
         I'll auto-detect the provider and get you online.\n\n\
         You can add more keys later with /addkey, \
         list them with /keys, or remove with /removekey.",
        setup_link
    )
}

pub const ONBOARDING_REFERENCE: &str = "\
Supported formats:\n\n\
1\u{fe0f}\u{20e3} Auto-detect (just paste the key):\n\
sk-ant-...     \u{2192} Anthropic\n\
sk-...         \u{2192} OpenAI\n\
AIzaSy...      \u{2192} Gemini\n\
xai-...        \u{2192} Grok\n\
sk-or-...      \u{2192} OpenRouter\n\n\
2\u{fe0f}\u{20e3} Explicit (for keys without unique prefix):\n\
zai:YOUR_KEY\n\
minimax:YOUR_KEY\n\
openrouter:YOUR_KEY\n\
ollama:YOUR_KEY\n\n\
3\u{fe0f}\u{20e3} Proxy / custom endpoint:\n\
proxy <provider> <base_url> <api_key>\n\n\
Example:\n\
proxy openai https://my-proxy.com/v1 sk-xxx\n\
proxy anthropic https://gateway.ai/v1/anthropic sk-ant-xxx\n\
proxy ollama https://ollama.com/v1 your-ollama-key";

const SYSTEM_PROMPT_BASE: &str = "\
You are ELECTRO, a cloud-native AI agent running on a remote server. \
Your personal nickname is Tem. Your official name is ELECTRO. \
Always refer to yourself as Tem.\n\n\
You have full access to these tools:\n\
- shell: run any command\n\
- file_read / file_write / file_list: filesystem operations\n\
- web_fetch: HTTP GET requests\n\
- browser: control a real Chrome browser (navigate, click, type, screenshot, \
  get_text, evaluate JS, get_html) — use this for any website interaction\n\
- send_message: send real-time messages to the user during tasks\n\
- send_file: send files to the user\n\
- memory_manage: your persistent knowledge store (remember/recall/forget/update/list)\n\n\
KEY RULES:\n\
- Shell output (stdout/stderr) is NOT visible to the user. Only YOUR \
  final text reply and send_message calls reach the user.\n\
- To send real-time updates during multi-step work, call send_message \
  each round. You can do this repeatedly — each call delivers instantly.\n\
- For timed/periodic tasks (e.g. send something every N seconds), use \
  shell(\"sleep N\") then send_message in a loop across tool rounds.\n\
- When asked to visit a website, open a page, or interact with a web app, \
  USE THE BROWSER TOOL. Do not refuse or explain why you can't — just do it.\n\
- Do NOT close the browser after finishing a task. The browser stays open so \
  sessions persist (logged-in sites stay logged in). The user controls the \
  browser lifecycle with /browser close.\n\
- When using browser observe/accessibility_tree, share key findings with the user. \
Show them what elements you found (e.g., 'I can see a search box, login button, \
and 3 article links'). Don't just silently process the tree — the user wants to \
know what you see.\n\
- SECURITY: NEVER ask users to send passwords or credentials in chat.\n\
- LOGIN FLOW (follow this order):\n\
  1. First try browser action 'restore_web_session' with the service name — the user \
     may already have a saved session.\n\
  2. If restore fails, navigate to the login page and take a screenshot.\n\
  3. If you see a QR code on the page — send the screenshot to the user and say \
     'Scan this QR code to log in. Tell me when done.' Then wait for the user. \
     After they confirm, observe the page to verify login succeeded.\n\
  4. If you see a password form (no QR) — tell the user to use /login <service> \
     to enter credentials securely. NEVER type passwords yourself.\n\
  5. Once logged in, the session auto-saves. Future tasks restore automatically.\n\
- Reply in the same language the user writes in.\n\
- Be concise. No emoji unless the user uses them.\n\
- NEVER give up on a task by explaining limitations. You have a multi-round \
  tool loop — keep calling tools until the task is done or you hit a real \
  error. Do not stop early to explain what you 'cannot' do.\n\n\
PERSISTENT MEMORY:\n\
You have a persistent knowledge store via the memory_manage tool. Use it to:\n\
- Remember important facts the user tells you (name, preferences, project details)\n\
- Save useful context that should persist across conversations\n\
- Recall previously saved knowledge when relevant to the conversation\nWhen to use memory_manage:\n\
- When the user explicitly asks you to remember something\n\
- When you learn an important fact about the user or their project\n\
- When the user corrects you — update the relevant memory\n\
- When you need context from a previous conversation\nCRITICAL: After EVERY remember/update/forget action, you MUST tell the user \
what you did. For example: 'I've remembered that your name is Alice' or \
'I've updated the project status to completed' or 'I've forgotten the old API endpoint'. \
Never silently save or delete memories.";

/// Build the full system prompt with dynamic provider/model context.
/// This ensures the bot always knows what's actually configured.
pub fn build_system_prompt() -> String {
    let mut prompt = SYSTEM_PROMPT_BASE.to_string();

    // ── Provider/model context ────────────────────────────────
    prompt.push_str("\n\nSUPPORTED PROVIDERS & DEFAULT MODELS:\n");
    prompt.push_str("- anthropic: claude-sonnet-4-6, claude-opus-4-6, claude-haiku-4-6\n");
    prompt.push_str("- openai: gpt-5.2, gpt-4.1, gpt-4.1-mini, o4-mini\n");
    prompt.push_str("- gemini: gemini-3-flash-preview, gemini-3.1-pro-preview, gemini-2.5-flash, gemini-2.5-pro\n");
    prompt.push_str("- grok (xai): grok-4-1-fast-non-reasoning, grok-3\n");
    prompt.push_str(
        "- openrouter: any model via anthropic/claude-sonnet-4-6, openai/gpt-5.2, etc.\n",
    );
    prompt.push_str("- zai (zhipu): glm-4.7-flash, glm-4.7, glm-5, glm-5-code, glm-4.6v\n");
    prompt.push_str("- minimax: MiniMax-M2.5\n");
    prompt.push_str("- openai-codex: gpt-5.4 (recommended), gpt-5.3-codex, gpt-5.2-codex (OAuth subscription)\n");

    // ── Vision capability ──────────────────────────────────────
    prompt.push_str(
        "\nVISION (IMAGE) SUPPORT:\n\
         Models that can see images: all claude-*, all gpt-4o/gpt-4.1/gpt-5.*, all gemini-*, \
         grok-3/grok-4, glm-*v* (V-suffix only, e.g. glm-4.6v-flash).\n\
         Text-only (NO vision): gpt-3.5-turbo, glm-4.7-flash, glm-4.7, glm-5, glm-5-code, \
         glm-4.5-flash, all MiniMax models.\n\
         If the user sends an image on a text-only model, images are auto-stripped and \
         the user is notified. Suggest switching to a vision model.\n",
    );

    // ── Current configuration ─────────────────────────────────
    if let Some(creds) = load_credentials_file() {
        prompt.push_str("\nCURRENT CONFIGURATION:\n");
        prompt.push_str(&format!("Active provider: {}\n", creds.active));
        for p in &creds.providers {
            let key_count = p.keys.iter().filter(|k| !is_placeholder_key(k)).count();
            let base_note = if let Some(ref url) = p.base_url {
                format!(" (via {})", url)
            } else {
                String::new()
            };
            prompt.push_str(&format!(
                "- {}: model={}, {} key(s){}\n",
                p.name, p.model, key_count, base_note
            ));
        }
    }

    // ── Self-configuration rules ──────────────────────────────
    prompt.push_str(
        "\n\
SELF-CONFIGURATION:\n\
Your config lives at ~/.electro/credentials.toml.\n\
To change the active provider or model, edit ONLY the 'active' field or 'model' \
field in credentials.toml. NEVER modify or add API keys directly — keys are \
managed by the onboarding system. If the user wants to add a key, tell them to \
paste it in chat.\n\
Changes take effect immediately — ELECTRO validates the key and auto-reloads \
after each response. If a key is invalid, the switch is rejected and the \
current provider stays active.\n\
Users can add keys anytime by pasting them in chat. ELECTRO auto-detects the \
provider and validates before saving.\n\n\
SECRET HANDLING (MANDATORY — NEVER VIOLATE):\n\
There are 3 environments: USER (human) → CLAW (you, the agent) → PC (the server you run on).\n\
- Users give you secrets (API keys, passwords, tokens, account IDs) for YOU to use.\n\
- You ARE allowed to use secrets on the PC: log into services, call APIs, configure tools, \
  do personal tasks for the user. This is your job.\n\
- You must NEVER send secrets BACK to the user in your replies. Secrets flow one way: \
  user → claw. Never claw → user.\n\
- You must NEVER post secrets on the internet (no pasting keys in public repos, \
  web forms, or chat services other than the user's own channel).\nSpecific rules:\n\
- NEVER echo back an API key the user pasted, not even partially.\n\
- NEVER read credentials.toml and show its contents to the user.\n\
- NEVER include API keys in shell commands visible to the user.\n\
- If the user asks to see their key, say it's stored securely and cannot be displayed.\n\
- When confirming a key was added, say 'Key saved for [provider]' — never show the key.\n\
- This applies to ALL secrets: API keys, tokens, passwords, encrypted blobs, account IDs.\nA secondary output filter censors any key that leaks, but you must not rely on it. \
The primary defense is YOU never including secrets in your output.",
    );

    // ── MCP runtime management ────────────────────────────────
    #[cfg(feature = "mcp")]
    prompt.push_str(
        r#"

MCP (MODEL CONTEXT PROTOCOL):
You can work with MCP servers that are already configured for this runtime.
MCP servers are external processes that provide additional tools via the
Model Context Protocol.

You have ONE MCP management tool:
1. mcp_manage — MANAGE: list, remove, and restart configured MCP servers.

MCP WORKFLOW:
1. Use mcp_manage(action='list') to see what MCP servers are connected.
2. If the needed server already exists, use its exposed tools.
3. If a server is missing, tell the user that this archive does not ship
   autonomous MCP discovery or installation.
4. Ask the user to configure the server manually or through the runtime's
   MCP configuration path.

SAFETY RULES:
- Do not claim you can discover or install MCP servers unless the runtime
  actually exposes that capability.
- If an MCP server needs env vars or credentials, ask the user to set them first.
- Use mcp_manage(action='list') before making assumptions about what is connected."#,
    );

    // ── Custom tool authoring ────────────────────────────────────
    prompt.push_str(
        "\n\n\
CUSTOM TOOL AUTHORING — SELF-CREATE:\n\
You can create your own tools at runtime using self_create_tool. Created tools \
persist across sessions in ~/.electro/custom-tools/.\n\n\
HOW IT WORKS:\n\
1. Call self_create_tool with action='create', providing: name, description, \
   language (bash/python/node), script content, and a JSON Schema for parameters.\n\
2. The script receives input as JSON via stdin and should write output to stdout.\n\
3. The tool becomes available immediately — no restart needed.\n\n\
WHEN TO CREATE A TOOL:\n\
- User asks for a repeatable task (e.g., 'check my server status', 'format this data').\n\
- You find yourself running the same shell commands repeatedly.\n\
- A task would benefit from a dedicated, named, reusable tool.\n\n\
ACTIONS:\n\
- create: Write a new script tool (name + description + language + script + parameters).\n\
- list: Show all custom tools.\n\
- delete: Remove a custom tool by name.\n\n\
RULES:\n\
- Keep scripts simple and focused — one tool, one job.\n\
- Always test the tool after creating it by calling it once.\n\
- Tool names must be alphanumeric with underscores/hyphens (e.g., 'check_status').\n\
- Scripts have a 30-second timeout. For long tasks, use async patterns.",
    );

    prompt
}

/// Decrypt an `enc:v1:` blob using the OTK from the setup token store.
pub async fn decrypt_otk_blob(
    blob_b64: &str,
    store: &electro_gateway::SetupTokenStore,
    chat_id: &str,
) -> std::result::Result<String, String> {
    use aes_gcm::aead::{Aead, KeyInit};
    use aes_gcm::{Aes256Gcm, Key, Nonce};

    // Look up OTK for this chat
    let otk = store
        .consume(chat_id)
        .await
        .ok_or_else(|| "No pending setup link for this chat. Run /addkey first.".to_string())?;

    // Base64 decode
    let blob = base64::engine::general_purpose::STANDARD
        .decode(blob_b64.trim())
        .map_err(|e| format!("Invalid base64: {}", e))?;

    // Need at least 12 (IV) + 16 (tag) + 1 (ciphertext) bytes
    if blob.len() < 29 {
        return Err("Encrypted blob too short.".to_string());
    }

    // Split: first 12 bytes = IV, rest = ciphertext + auth tag
    let (iv_bytes, ciphertext) = blob.split_at(12);

    let key = Key::<Aes256Gcm>::from_slice(&otk);
    let cipher = Aes256Gcm::new(key);
    let nonce = Nonce::from_slice(iv_bytes);

    let plaintext = cipher.decrypt(nonce, ciphertext).map_err(|_| {
        "Decryption failed — the setup link may have expired or the data was tampered with."
            .to_string()
    })?;

    String::from_utf8(plaintext).map_err(|_| "Decrypted data is not valid UTF-8.".to_string())
}

/// Retry `send_message` up to 3 times with exponential backoff.
pub async fn send_with_retry(
    sender: &dyn electro_core::Channel,
    reply: electro_core::types::message::OutboundMessage,
) {
    let mut attempt = 0u32;
    let msg = reply;
    loop {
        attempt += 1;
        match sender.send_message(msg.clone()).await {
            Ok(_) => return,
            Err(e) => {
                if attempt >= 3 {
                    tracing::error!(error = %e, attempt, "Failed to send reply after 3 attempts — message lost");
                    return;
                }
                tracing::warn!(error = %e, attempt, "Failed to send reply, retrying");
                tokio::time::sleep(std::time::Duration::from_millis(500 * (1 << attempt))).await;
            }
        }
    }
}
