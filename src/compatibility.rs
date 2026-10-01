use std::path::{Component, Path, PathBuf};

pub fn trim_js_whitespace(value: &str) -> &str {
  value.trim_matches(|character: char| {
    character == '\u{feff}' || (character.is_whitespace() && character != '\u{85}')
  })
}

pub fn normalize_path(path: &Path) -> PathBuf {
  let mut normalized = PathBuf::new();
  for component in path.components() {
    match component {
      Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
      Component::RootDir => normalized.push(std::path::MAIN_SEPARATOR_STR),
      Component::CurDir => {}
      Component::ParentDir => {
        if !normalized.pop() && !normalized.is_absolute() {
          normalized.push(component.as_os_str());
        }
      }
      Component::Normal(part) => normalized.push(part),
    }
  }
  normalized
}
