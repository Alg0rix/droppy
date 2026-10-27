use super::*;
/// Fetch live models for the active provider on a background thread.
///
/// The result is persisted per provider (`ai-models.json`) so the picker shows
/// the real catalog from the first frame after a restart. Failures keep the
/// last known good list and say why in the footer instead of silently
/// reverting to the curated fallback.
pub fn refresh_models(provider: AiProvider, base_url: String) {
    with_runtime(|rt| {
        rt.view.models_loading = true;
        rt.view.status_note = format!("Syncing {} models\u{2026}", provider.label());
    });
    std::thread::spawn(move || {
        let mut sec = load_secrets();
        let mut failure: Option<String> = None;
        let fetched: Vec<String> = match provider {
            AiProvider::ChatGpt => match ensure_fresh_chatgpt_token(&mut sec) {
                Ok((token, acct)) => {
                    let url = format!("{CODEX_BASE_URL}/models?client_version=1.0.0");
                    let auth = format!("Bearer {token}");
                    let mut hdrs = vec![("Authorization", auth.as_str()), ("Accept", "application/json")];
                    if !acct.is_empty() {
                        hdrs.push(("ChatGPT-Account-Id", acct.as_str()));
                    }
                    match crate::http::get(&url, &hdrs, MODEL_CATALOG_LIMIT) {
                        Ok(b) => {
                            let models = parse_codex_models(&String::from_utf8_lossy(&b));
                            if models.is_empty() {
                                failure = Some("empty catalog".into());
                            }
                            models
                        }
                        Err(status) => {
                            failure = Some(if status == 0 { "network error".to_string() } else { format!("HTTP {status}") });
                            Vec::new()
                        }
                    }
                }
                Err(e) => {
                    failure = Some(e);
                    Vec::new()
                }
            },
            AiProvider::OpenCodeGo => {
                let key = decrypt_secret(&sec.opencode_key);
                let url = format!("{OPENCODE_GO_BASE_URL}/models");
                let auth = format!("Bearer {key}");
                let mut hdrs = vec![("Accept", "application/json"), ("x-opencode-session", "teardrop-models")];
                if !key.is_empty() {
                    hdrs.push(("Authorization", auth.as_str()));
                }
                match crate::http::get(&url, &hdrs, MODEL_CATALOG_LIMIT) {
                    Ok(b) => parse_openai_models_list(&String::from_utf8_lossy(&b)),
                    Err(status) => {
                        failure = Some(if status == 0 { "network error".into() } else { format!("HTTP {status}") });
                        Vec::new()
                    }
                }
            }
            AiProvider::OpenAiCompat => {
                let key = decrypt_secret(&sec.openai_key);
                let base = normalize_openai_base_url(&base_url);
                let url = format!("{base}/models");
                let auth = format!("Bearer {key}");
                let mut hdrs = vec![("Accept", "application/json")];
                if !key.is_empty() {
                    hdrs.push(("Authorization", auth.as_str()));
                }
                match crate::http::get(&url, &hdrs, MODEL_CATALOG_LIMIT) {
                    Ok(b) => parse_openai_models_list(&String::from_utf8_lossy(&b)),
                    Err(status) => {
                        failure = Some(if status == 0 { "network error".into() } else { format!("HTTP {status}") });
                        Vec::new()
                    }
                }
            }
            _ => default_models_for(provider),
        };

        let synced = !fetched.is_empty();
        with_runtime(|rt| {
            rt.view.models_loading = false;
            if synced {
                let count = fetched.len();
                rt.models_provider = Some(provider);
                rt.view.available_models = fetched.clone();
                rt.view.status_note = format!("Synced {count} models");
            } else {
                if rt.models_provider != Some(provider) || rt.view.available_models.is_empty() {
                    let cached = load_model_cache(provider);
                    rt.models_provider = Some(provider);
                    rt.view.available_models = if cached.is_empty() { default_models_for(provider) } else { cached };
                }
                rt.view.status_note = match failure {
                    Some(why) => format!("Sync failed ({why}) \u{2014} showing last known catalog"),
                    None => "Using default model catalog".into(),
                };
            }
        });
        if synced {
            save_model_cache(provider, &fetched);
        }
    });
}

/// Send a user prompt (either typed or clicked from starter suggestions).
pub fn send_prompt(prompt: String, settings: Settings, day_context: Option<String>) {
    let prompt = prompt.trim().to_string();
    if prompt.is_empty() || BUSY.swap(true, Ordering::SeqCst) {
        return;
    }

    let provider = settings.ai_provider;
    let model = settings.active_ai_model().to_string();
    let base_url = settings.active_ai_url().to_string();

    let (conv_id, history, memories) = with_runtime(|rt| {
        rt.view.input.clear();
        rt.view.error.clear();
        rt.view.status_note.clear();
        rt.view.show_history = false;
        rt.view.show_setup = false;
        rt.view.busy = true;
        rt.view.scroll = 0;

        let conv = rt.active_conv_mut();
        if conv.messages.is_empty() {
            conv.title = prompt.chars().take(36).collect();
        }
        conv.updated_at = now_secs();
        conv.messages.push(ChatMessage { role: Role::User, text: prompt.clone(), actions: Vec::new(), timestamp: now_secs() });
        let conv_id = conv.id;
        let history = conv.messages.clone();
        let memories = rt.store.memories.clone();
        save_chat_store(&rt.store);
        rt.rebuild_view(provider);
        (conv_id, history, memories)
    });

    std::thread::spawn(move || {
        let mut sec = load_secrets();
        let sys_prompt = build_system_prompt(day_context.as_deref(), &memories);
        let result = call_provider(provider, &model, &base_url, &mut sec, conv_id, &sys_prompt, &history);

        match result {
            Ok(raw_reply) => {
                let (clean_text, directives) = extract_directives(&raw_reply);
                let display_text = if clean_text.is_empty() { "Done!".to_string() } else { clean_text };
                with_runtime(|rt| {
                    rt.view.busy = false;
                    if let Some(conv) = rt.store.conversations.iter_mut().find(|c| c.id == conv_id) {
                        let msg_idx = conv.messages.len();
                        conv.messages.push(ChatMessage { role: Role::Assistant, text: display_text, actions: Vec::new(), timestamp: now_secs() });
                        conv.updated_at = now_secs();
                        if !directives.is_empty() {
                            rt.pending_directives.push((conv_id, msg_idx, directives));
                        }
                    }
                    save_chat_store(&rt.store);
                    rt.rebuild_view(provider);
                });
            }
            Err(err) => {
                with_runtime(|rt| {
                    rt.view.busy = false;
                    rt.view.error = err;
                    rt.rebuild_view(provider);
                });
            }
        }
        BUSY.store(false, Ordering::SeqCst);
    });
}

fn call_provider(provider: AiProvider, model: &str, base_url: &str, sec: &mut SecretStore, conv_id: u64, sys_prompt: &str, history: &[ChatMessage]) -> Result<String, String> {
    // Keep up to the last 16 messages for context window efficiency.
    let start = history.len().saturating_sub(16);
    let recent = &history[start..];

    match provider {
        AiProvider::ChatGpt => {
            let (token, acct) = ensure_fresh_chatgpt_token(sec)?;
            call_responses_endpoint(&format!("{CODEX_BASE_URL}/responses"), &token, Some(&acct), model, sys_prompt, recent, &format!("teardrop-{conv_id}"), false)
        }
        AiProvider::OpenCodeGo => {
            let key = decrypt_secret(&sec.opencode_key);
            if key.is_empty() {
                return Err("OpenCode Go API key not set \u{2014} click Setup \u{2192} Paste Key".into());
            }
            let session = format!("teardrop-{conv_id}");
            match opencode_model_protocol(model) {
                OpenCodeProtocol::Responses => call_responses_endpoint(&format!("{OPENCODE_GO_BASE_URL}/responses"), &key, None, model, sys_prompt, recent, &session, true),
                OpenCodeProtocol::Messages => call_anthropic_endpoint(&format!("{OPENCODE_GO_BASE_URL}/messages"), &key, model, sys_prompt, recent, Some(&session)),
                OpenCodeProtocol::Chat => call_openai_chat_endpoint(&format!("{OPENCODE_GO_BASE_URL}/chat/completions"), &key, model, sys_prompt, recent, Some(&session)),
            }
        }
        AiProvider::OpenAiCompat => {
            let key = decrypt_secret(&sec.openai_key);
            let base = normalize_openai_base_url(base_url);
            if base.contains("api.openai.com") && key.is_empty() {
                return Err("OpenAI API key not set \u{2014} click Setup \u{2192} Paste Key".into());
            }
            call_openai_chat_endpoint(&format!("{base}/chat/completions"), &key, model, sys_prompt, recent, None)
        }
        AiProvider::Gemini => {
            let key = decrypt_secret(&sec.gemini_key);
            if key.is_empty() {
                return Err("Gemini API key not set \u{2014} click Setup \u{2192} Paste Key".into());
            }
            call_gemini_endpoint(&key, model, sys_prompt, recent)
        }
        AiProvider::Claude => {
            let key = decrypt_secret(&sec.claude_key);
            if key.is_empty() {
                return Err("Claude API key not set \u{2014} click Setup \u{2192} Paste Key".into());
            }
            call_anthropic_endpoint("https://api.anthropic.com/v1/messages", &key, model, sys_prompt, recent, None)
        }
        AiProvider::Ollama => {
            let base = base_url.trim().trim_end_matches('/');
            let base = if base.is_empty() { "http://localhost:11434" } else { base };
            call_ollama_endpoint(base, model, sys_prompt, recent)
        }
    }
}

fn call_responses_endpoint(
    url: &str,
    bearer: &str,
    account_id: Option<&str>,
    model: &str,
    sys_prompt: &str,
    messages: &[ChatMessage],
    session_id: &str,
    is_opencode: bool,
) -> Result<String, String> {
    let input: Vec<Value> = messages
        .iter()
        .map(|m| {
            json!({
                "role": match m.role { Role::User => "user", Role::Assistant => "assistant" },
                "content": m.text,
            })
        })
        .collect();
    let payload = json!({
        "model": model,
        "instructions": sys_prompt,
        "input": input,
        "store": false,
        "stream": true,
    })
    .to_string();

    let auth = format!("Bearer {bearer}");
    let mut headers = vec![("Authorization", auth.as_str()), ("Content-Type", "application/json"), ("Accept", "text/event-stream")];
    if let Some(acct) = account_id.filter(|a| !a.is_empty()) {
        headers.push(("ChatGPT-Account-Id", acct));
    }
    if is_opencode {
        headers.push(("x-opencode-session", session_id));
    } else {
        headers.push(("session-id", session_id));
    }

    match crate::http::post(url, &headers, payload.as_bytes(), 2 * 1024 * 1024) {
        Ok(bytes) => parse_responses_sse_or_json(&String::from_utf8_lossy(&bytes)),
        Err((status, snippet)) => Err(friendly_http_error("Responses API", status, &snippet)),
    }
}

fn call_openai_chat_endpoint(url: &str, api_key: &str, model: &str, sys_prompt: &str, messages: &[ChatMessage], opencode_session: Option<&str>) -> Result<String, String> {
    let mut wire = Vec::with_capacity(messages.len() + 1);
    wire.push(json!({ "role": "system", "content": sys_prompt }));
    for m in messages {
        wire.push(json!({
            "role": match m.role { Role::User => "user", Role::Assistant => "assistant" },
            "content": m.text,
        }));
    }
    let payload = json!({
        "model": model,
        "messages": wire,
        "temperature": 0.4,
    })
    .to_string();

    let auth = format!("Bearer {api_key}");
    let mut headers = vec![("Content-Type", "application/json"), ("Accept", "application/json")];
    if !api_key.is_empty() {
        headers.push(("Authorization", auth.as_str()));
    }
    if let Some(sess) = opencode_session {
        headers.push(("x-opencode-session", sess));
    }

    match crate::http::post(url, &headers, payload.as_bytes(), 2 * 1024 * 1024) {
        Ok(bytes) => parse_openai_chat_response(&String::from_utf8_lossy(&bytes)),
        Err((status, snippet)) => Err(friendly_http_error("Chat API", status, &snippet)),
    }
}

fn call_anthropic_endpoint(url: &str, api_key: &str, model: &str, sys_prompt: &str, messages: &[ChatMessage], opencode_session: Option<&str>) -> Result<String, String> {
    let wire: Vec<Value> = messages
        .iter()
        .map(|m| {
            json!({
                "role": match m.role { Role::User => "user", Role::Assistant => "assistant" },
                "content": m.text,
            })
        })
        .collect();
    let payload = json!({
        "model": model,
        "max_tokens": 1024,
        "system": sys_prompt,
        "messages": wire,
    })
    .to_string();

    let auth = format!("Bearer {api_key}");
    let mut headers = vec![("Content-Type", "application/json"), ("Accept", "application/json"), ("anthropic-version", "2023-06-01"), ("x-api-key", api_key)];
    if let Some(sess) = opencode_session {
        headers.push(("Authorization", auth.as_str()));
        headers.push(("x-opencode-session", sess));
    }

    match crate::http::post(url, &headers, payload.as_bytes(), 2 * 1024 * 1024) {
        Ok(bytes) => parse_anthropic_response(&String::from_utf8_lossy(&bytes)),
        Err((status, snippet)) => Err(friendly_http_error("Claude", status, &snippet)),
    }
}

fn call_gemini_endpoint(api_key: &str, model: &str, sys_prompt: &str, messages: &[ChatMessage]) -> Result<String, String> {
    let url = format!("https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent");
    let contents: Vec<Value> = messages
        .iter()
        .map(|m| {
            json!({
                "role": match m.role { Role::User => "user", Role::Assistant => "model" },
                "parts": [{ "text": m.text }],
            })
        })
        .collect();
    let payload = json!({
        "systemInstruction": { "parts": [{ "text": sys_prompt }] },
        "contents": contents,
    })
    .to_string();

    let headers = [("Content-Type", "application/json"), ("Accept", "application/json"), ("x-goog-api-key", api_key)];
    match crate::http::post(&url, &headers, payload.as_bytes(), 2 * 1024 * 1024) {
        Ok(bytes) => parse_gemini_response(&String::from_utf8_lossy(&bytes)),
        Err((status, snippet)) => Err(friendly_http_error("Gemini", status, &snippet)),
    }
}

fn call_ollama_endpoint(base_url: &str, model: &str, sys_prompt: &str, messages: &[ChatMessage]) -> Result<String, String> {
    let url = format!("{base_url}/api/chat");
    let mut wire = Vec::with_capacity(messages.len() + 1);
    wire.push(json!({ "role": "system", "content": sys_prompt }));
    for m in messages {
        wire.push(json!({
            "role": match m.role { Role::User => "user", Role::Assistant => "assistant" },
            "content": m.text,
        }));
    }
    let payload = json!({
        "model": model,
        "messages": wire,
        "stream": false,
    })
    .to_string();

    let headers = [("Content-Type", "application/json"), ("Accept", "application/json")];
    match crate::http::post(&url, &headers, payload.as_bytes(), 2 * 1024 * 1024) {
        Ok(bytes) => parse_openai_chat_response(&String::from_utf8_lossy(&bytes)),
        Err((0, _)) => Err("Could not reach Ollama \u{2014} is `ollama serve` running?".into()),
        Err((status, snippet)) => Err(friendly_http_error("Ollama", status, &snippet)),
    }
}

fn friendly_http_error(label: &str, status: u32, snippet: &str) -> String {
    if status == 0 {
        return format!("{label}: network error \u{2014} check connection or URL");
    }
    if let Ok(v) = serde_json::from_str::<Value>(snippet) {
        if let Some(msg) = v.pointer("/error/message").or_else(|| v.pointer("/detail")).or_else(|| v.get("message")).and_then(|m| m.as_str()) {
            let clean: String = msg.chars().take(110).collect();
            return format!("{label} ({status}): {clean}");
        }
    }
    match status {
        401 | 403 => format!("{label} ({status}): unauthorized \u{2014} check API key or sign-in"),
        404 => format!("{label} (404): model or endpoint not found"),
        429 => format!("{label} (429): rate limit reached \u{2014} wait a moment"),
        _ => format!("{label}: HTTP {status}"),
    }
}
