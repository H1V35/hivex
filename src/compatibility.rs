use serde_json::Value;
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

pub(crate) fn nonnegative_integer(value: Option<&Value>) -> Option<u64> {
    value?.as_u64().or_else(|| {
        let number = value?.as_f64()?;
        (number.is_finite() && number.fract() == 0.0 && number >= 0.0).then_some(number as u64)
    })
}

pub(crate) fn positive_integer(value: &Value) -> Option<u64> {
    nonnegative_integer(Some(value)).filter(|number| *number > 0)
}

pub(crate) fn legacy_unstarted_invocation(report: &Value) -> bool {
    report["cleanup"] == "not-observed"
        && report["code"] == "MODEL_ADMISSION_FAILED"
        && report["diagnostic"]["kind"] == "native-admission"
        && report["diagnostic"]["message"]
            .as_str()
            .is_some_and(|message| {
                regex::Regex::new(r"^Knowledge execution requires verified codex-cli \S+$")
                    .unwrap()
                    .is_match(message)
            })
        && ["interruption", "nativeProcessId", "turnAccepted"]
            .iter()
            .all(|field| report.get(*field).is_none())
        && report["outcome"] == "failed"
        && report.get("usage") == Some(&Value::Null)
}
