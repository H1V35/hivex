pub(crate) mod codex;
use crate::error::{HivexError, Result};
use crate::execution::Execution;

pub fn select(
    integration: &str,
    binary: String,
    model: Option<&String>,
    effort: Option<&String>,
    provider: Option<&String>,
    deadline_ms: u64,
) -> Result<Execution> {
    if integration != "codex" {
        return Err(HivexError::new(
            "UNSUPPORTED_INTEGRATION",
            format!("Execution integration is not installed: {integration}"),
        ));
    }
    let mut profile = codex::default_profile();
    if let Some(model) = model {
        profile.model = model.clone();
    }
    if let Some(effort) = effort {
        profile.options.insert("effort".into(), effort.clone());
    }
    if let Some(provider) = provider {
        profile.provider = provider.clone();
    }
    if profile.provider != "openai" {
        return Err(HivexError::new(
            "UNSUPPORTED_PROFILE",
            "The Codex integration currently admits only OpenAI through ChatGPT",
        ));
    }
    for value in [&profile.model, &profile.options["effort"]] {
        if value.is_empty() || value.len() > 128 || value.chars().any(char::is_control) {
            return Err(HivexError::new(
                "INVALID_ARGUMENT",
                "Model and effort must be nonempty profile identifiers of at most 128 bytes",
            ));
        }
    }
    Ok(Execution::new(
        Box::new(codex::Codex { binary, profile }),
        deadline_ms,
    ))
}
