pub fn trim_js_whitespace(value: &str) -> &str {
  value.trim_matches(|character: char| {
    character == '\u{feff}' || (character.is_whitespace() && character != '\u{85}')
  })
}
