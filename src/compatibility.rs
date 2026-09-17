pub const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

pub fn trim_js_whitespace(value: &str) -> &str {
    value.trim_matches(|character: char| {
        character == '\u{feff}' || (character.is_whitespace() && character != '\u{85}')
    })
}

pub fn parse_number(value: &str) -> Option<f64> {
    let value = trim_js_whitespace(value);
    if value.is_empty() {
        return Some(0.0);
    }
    let (radix, digits) = if let Some(digits) = value.strip_prefix("0x") {
        (16, digits)
    } else if let Some(digits) = value.strip_prefix("0X") {
        (16, digits)
    } else if let Some(digits) = value.strip_prefix("0o") {
        (8, digits)
    } else if let Some(digits) = value.strip_prefix("0O") {
        (8, digits)
    } else if let Some(digits) = value.strip_prefix("0b") {
        (2, digits)
    } else if let Some(digits) = value.strip_prefix("0B") {
        (2, digits)
    } else {
        return value.parse::<f64>().ok();
    };
    (!digits.is_empty())
        .then(|| u64::from_str_radix(digits, radix).ok())
        .flatten()
        .map(|number| number as f64)
}
