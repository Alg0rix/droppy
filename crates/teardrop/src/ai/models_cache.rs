use super::*;
// ----------------------------------------------------- Model catalog cache

/// Last successfully synced catalog per provider, so a restart shows the real
/// list straight away instead of the curated fallback.
fn models_cache_path() -> PathBuf {
    appdata_dir().join("ai-models.json")
}

/// Stable key for `AiProvider` in the cache file (matches the settings JSON:
/// `chat_gpt`, `open_code_go`, ...).
pub(super) fn provider_key(provider: AiProvider) -> String {
    serde_json::to_value(provider).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_else(|| provider.label().to_string())
}

fn read_model_cache() -> BTreeMap<String, Vec<String>> {
    std::fs::read_to_string(models_cache_path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

pub(super) fn load_model_cache(provider: AiProvider) -> Vec<String> {
    read_model_cache().remove(&provider_key(provider)).unwrap_or_default().into_iter().filter(|m| !m.trim().is_empty()).collect()
}

pub(super) fn save_model_cache(provider: AiProvider, models: &[String]) {
    if models.is_empty() {
        return;
    }
    let mut cache = read_model_cache();
    cache.insert(provider_key(provider), models.to_vec());
    if let Ok(json) = serde_json::to_string_pretty(&cache) {
        let tmp = models_cache_path().with_extension("tmp");
        if std::fs::write(&tmp, json).is_ok() {
            let _ = std::fs::rename(&tmp, models_cache_path());
        }
    }
}
