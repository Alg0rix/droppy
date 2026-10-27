use super::*;
// --------------------------------------------------- Protocol & SSE Parsers

/// OpenCode Go wire protocol per model family (mirrors `Alg0rix/tomo`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenCodeProtocol {
    /// OpenAI Responses API (`/v1/responses`) for `gpt-*`, `grok-*`, `muse-spark*`.
    Responses,
    /// Anthropic Messages API (`/v1/messages`) for `claude-*`, `union-alpha*`, `minimax-*`, `qwen*`.
    Messages,
    /// OpenAI Chat Completions (`/v1/chat/completions`) for `deepseek-*`, `glm-*`, `kimi-*`, etc.
    Chat,
}

pub fn opencode_model_protocol(model: &str) -> OpenCodeProtocol {
    let m = model.trim().to_ascii_lowercase();
    let m = m.strip_prefix("opencode-go/").unwrap_or(&m);
    if m.starts_with("gpt-") || m.starts_with("grok-") || m.starts_with("muse-spark") {
        OpenCodeProtocol::Responses
    } else if m.starts_with("claude-") || m.starts_with("union-alpha") || m.starts_with("minimax-") || m.starts_with("qwen") {
        OpenCodeProtocol::Messages
    } else {
        OpenCodeProtocol::Chat
    }
}

/// Normalize an OpenAI-compatible base URL into its `/v1` root (without trailing slash).
pub fn normalize_openai_base_url(raw: &str) -> String {
    let mut s = raw.trim().trim_end_matches('/').to_string();
    if s.is_empty() {
        return "https://api.openai.com/v1".into();
    }
    if let Some(stripped) = s.strip_suffix("/chat/completions") {
        s = stripped.trim_end_matches('/').to_string();
    }
    if !s.starts_with("https://") && !s.starts_with("http://") {
        s = format!("https://{s}");
    }
    // If the URL has no path after the host (e.g. `https://api.openai.com`), append `/v1`.
    if let Some(rest) = s.strip_prefix("https://").or_else(|| s.strip_prefix("http://")) {
        if !rest.contains('/') {
            s.push_str("/v1");
        }
    }
    s
}

/// Parse a Codex / OpenAI `/responses` payload (supports both SSE streams and JSON objects).
pub fn parse_responses_sse_or_json(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.starts_with('{') {
        if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
            if let Some(err) = v.get("error").and_then(|e| e.get("message").or(Some(e))).and_then(|m| m.as_str()) {
                return Err(err.to_string());
            }
            if let Some(txt) = v.get("output_text").and_then(|t| t.as_str()) {
                if !txt.is_empty() {
                    return Ok(txt.to_string());
                }
            }
            let mut chunks = Vec::new();
            if let Some(items) = v.get("output").and_then(|o| o.as_array()) {
                for item in items {
                    if let Some(content) = item.get("content").and_then(|c| c.as_array()) {
                        for part in content {
                            if let Some(t) = part.get("text").and_then(|t| t.as_str()) {
                                chunks.push(t.to_string());
                            }
                        }
                    }
                }
            }
            if !chunks.is_empty() {
                return Ok(chunks.join(""));
            }
        }
    }

    let mut deltas = Vec::new();
    let mut fallback = Vec::new();
    for line in raw.lines() {
        let line = line.trim();
        let Some(data) = line.strip_prefix("data:") else { continue };
        let data = data.trim();
        if data.is_empty() || data == "[DONE]" {
            continue;
        }
        let Ok(ev) = serde_json::from_str::<Value>(data) else { continue };
        if let Some(err) = ev.get("error").and_then(|e| e.get("message")).and_then(|m| m.as_str()) {
            return Err(err.to_string());
        }
        let kind = ev.get("type").and_then(|t| t.as_str()).unwrap_or("");
        match kind {
            "response.output_text.delta" => {
                if let Some(d) = ev.get("delta").and_then(|d| d.as_str()) {
                    deltas.push(d.to_string());
                }
            }
            "response.output_item.done" if deltas.is_empty() => {
                if let Some(content) = ev.pointer("/item/content").and_then(|c| c.as_array()) {
                    for c in content {
                        if let Some(t) = c.get("text").and_then(|t| t.as_str()) {
                            fallback.push(t.to_string());
                        }
                    }
                }
            }
            "response.completed" | "response.done" if deltas.is_empty() && fallback.is_empty() => {
                if let Some(output) = ev.pointer("/response/output").and_then(|o| o.as_array()) {
                    for item in output {
                        if let Some(content) = item.get("content").and_then(|c| c.as_array()) {
                            for c in content {
                                if let Some(t) = c.get("text").and_then(|t| t.as_str()) {
                                    fallback.push(t.to_string());
                                }
                            }
                        }
                    }
                }
            }
            "error" | "response.failed" => {
                let msg = ev.pointer("/error/message").or_else(|| ev.pointer("/response/error/message")).and_then(|m| m.as_str()).unwrap_or("Responses API error");
                return Err(msg.to_string());
            }
            _ => {}
        }
    }

    if !deltas.is_empty() {
        Ok(deltas.join(""))
    } else if !fallback.is_empty() {
        Ok(fallback.join(""))
    } else {
        Err("Empty response from model".into())
    }
}

/// Parse an OpenAI `/chat/completions` response (JSON or SSE stream).
pub fn parse_openai_chat_response(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.starts_with("data:") || trimmed.contains("\ndata:") {
        let mut out = String::new();
        for line in trimmed.lines() {
            let Some(data) = line.trim().strip_prefix("data:") else { continue };
            let data = data.trim();
            if data.is_empty() || data == "[DONE]" {
                continue;
            }
            if let Ok(v) = serde_json::from_str::<Value>(data) {
                if let Some(delta) = v.pointer("/choices/0/delta/content").and_then(|c| c.as_str()) {
                    out.push_str(delta);
                }
            }
        }
        if !out.is_empty() {
            return Ok(out);
        }
    }

    let v: Value = serde_json::from_str(trimmed).map_err(|_| "Invalid JSON from server".to_string())?;
    if let Some(err) = v.pointer("/error/message").and_then(|m| m.as_str()) {
        return Err(err.to_string());
    }
    if let Some(content) = v.pointer("/choices/0/message/content") {
        if let Some(s) = content.as_str() {
            return Ok(s.to_string());
        }
        if let Some(arr) = content.as_array() {
            let joined: String = arr.iter().filter_map(|p| p.get("text").and_then(|t| t.as_str())).collect::<Vec<_>>().join("");
            if !joined.is_empty() {
                return Ok(joined);
            }
        }
    }
    if let Some(s) = v.pointer("/message/content").and_then(|c| c.as_str()) {
        return Ok(s.to_string());
    }
    Err("No message content in response".into())
}

/// Parse an Anthropic `/v1/messages` response.
pub fn parse_anthropic_response(raw: &str) -> Result<String, String> {
    let v: Value = serde_json::from_str(raw.trim()).map_err(|_| "Invalid JSON from Claude".to_string())?;
    if let Some(err) = v.pointer("/error/message").and_then(|m| m.as_str()) {
        return Err(err.to_string());
    }
    if let Some(arr) = v.get("content").and_then(|c| c.as_array()) {
        let mut texts = Vec::new();
        for block in arr {
            if let Some(t) = block.get("text").and_then(|t| t.as_str()) {
                texts.push(t);
            }
        }
        if !texts.is_empty() {
            return Ok(texts.join(""));
        }
    }
    Err("Empty response from Claude".into())
}

/// Parse a Google Gemini `generateContent` response.
pub fn parse_gemini_response(raw: &str) -> Result<String, String> {
    let v: Value = serde_json::from_str(raw.trim()).map_err(|_| "Invalid JSON from Gemini".to_string())?;
    if let Some(err) = v.pointer("/error/message").and_then(|m| m.as_str()) {
        return Err(err.to_string());
    }
    if let Some(parts) = v.pointer("/candidates/0/content/parts").and_then(|p| p.as_array()) {
        let mut out = String::new();
        for p in parts {
            if let Some(t) = p.get("text").and_then(|t| t.as_str()) {
                out.push_str(t);
            }
        }
        if !out.is_empty() {
            return Ok(out);
        }
    }
    Err("Empty response from Gemini".into())
}

/// Parse `/codex/models` JSON response into a sorted list of visible model slugs.
pub fn parse_codex_models(raw: &str) -> Vec<String> {
    let Ok(v) = serde_json::from_str::<Value>(raw) else { return Vec::new() };
    let Some(models) = v.get("models").and_then(|m| m.as_array()) else { return Vec::new() };
    let mut ranked: Vec<(i64, String)> = Vec::new();
    for entry in models {
        let Some(slug) = entry.get("slug").and_then(|s| s.as_str()).map(str::trim) else { continue };
        if slug.is_empty() {
            continue;
        }
        let vis = entry.get("visibility").and_then(|v| v.as_str()).unwrap_or("");
        if vis.eq_ignore_ascii_case("hide") || vis.eq_ignore_ascii_case("hidden") {
            continue;
        }
        let prio = entry.get("priority").and_then(|p| p.as_i64()).unwrap_or(999);
        ranked.push((prio, slug.to_string()));
    }
    ranked.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    let mut seen = Vec::new();
    for (_, slug) in ranked {
        if !seen.contains(&slug) {
            seen.push(slug);
        }
    }
    seen
}

/// Parse OpenAI / OpenCode Go `/v1/models` JSON response (`{"data": [{"id": ...}]}`).
pub fn parse_openai_models_list(raw: &str) -> Vec<String> {
    let Ok(v) = serde_json::from_str::<Value>(raw) else { return Vec::new() };
    let mut out = Vec::new();
    if let Some(arr) = v.get("data").or_else(|| v.get("models")).and_then(|d| d.as_array()) {
        for item in arr {
            let id = item.get("id").or_else(|| item.get("name")).and_then(|s| s.as_str()).map(str::trim).unwrap_or("");
            if !id.is_empty() && !out.iter().any(|x| x == id) {
                out.push(id.to_string());
            }
        }
    }
    out
}
