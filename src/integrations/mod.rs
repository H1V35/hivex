mod codex;
use crate::error::{HivexError, Result};
use crate::execution::Execution;

#[derive(Clone, Copy, Default)]
pub struct ProfileOverrides<'a> {
  pub model: Option<&'a String>,
  pub effort: Option<&'a String>,
  pub provider: Option<&'a String>,
}

pub fn select(
  integration: &str,
  binary: String,
  overrides: ProfileOverrides<'_>,
  deadline_ms: u64,
) -> Result<Execution> {
  if integration != "codex" {
    return Err(HivexError::new(
      "UNSUPPORTED_INTEGRATION",
      format!("Execution integration is not installed: {integration}"),
    ));
  }
  let mut profile = codex::default_profile();
  if let Some(model) = overrides.model {
    profile.model.clone_from(model);
  }
  if let Some(effort) = overrides.effort {
    profile.options.insert("effort".into(), effort.clone());
  }
  if let Some(provider) = overrides.provider {
    profile.provider.clone_from(provider);
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
