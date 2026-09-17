pub(crate) mod markdown;
use crate::error::{HivexError, Result};
use globset::{GlobBuilder, GlobMatcher};
#[cfg(not(target_os = "macos"))]
use icu_collator::{Collator, options::CollatorOptions};
#[cfg(not(target_os = "macos"))]
use icu_locale_core::locale;
use serde_json::{Map, Value, json};
use std::cmp::Ordering;
use std::collections::HashSet;
use std::fs;
use std::path::{Component, Path, PathBuf};
#[cfg(not(target_os = "macos"))]
use std::sync::OnceLock;

const DEFAULT_INCLUDE: [&str; 3] = ["**/*.md", "**/*.markdown", "**/*.mdown"];
const MAX_SOURCE_BYTES: usize = 32 * 1024 * 1024;
const MAX_CORPUS_BYTES: usize = 64 * 1024 * 1024;
pub(crate) const MAX_DOCUMENTS: usize = 2_048;
const MAX_PATTERNS: usize = 64;
const PROTECTED_DIRECTORIES: [&str; 3] = [".git", ".hivex", "node_modules"];
const EXCLUDED_DIRECTORIES: [&str; 3] = ["vendor", "dist", "build"];

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Warning {
    pub path: String,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Document {
    pub id: String,
    pub path: String,
    pub title: String,
    pub text: String,
    pub hash: String,
    pub status: Option<String>,
    pub links: Vec<String>,
    pub historical: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Project {
    pub root: PathBuf,
    pub snapshot: String,
    pub current_snapshot: String,
    pub documents: Vec<Document>,
    pub current_documents: Vec<Document>,
    pub historical_documents: Vec<Document>,
    pub warnings: Vec<Warning>,
}

#[derive(Clone, Debug)]
struct Config {
    include: Vec<String>,
    exclude: Vec<String>,
    history: Vec<String>,
}

#[derive(Clone, Debug)]
struct Candidate {
    absolute_path: PathBuf,
    path: String,
}

#[derive(Clone, Debug)]
struct ParsedDocument {
    document: Document,
    raw_links: Vec<String>,
}

fn error(code: &str, message: impl Into<String>) -> HivexError {
    HivexError::new(code, message)
}

fn details(code: &str, message: impl Into<String>, value: Value) -> HivexError {
    error(code, message).with_details(value)
}

fn utf16_units(value: &str) -> Vec<u16> {
    value.encode_utf16().collect()
}

/// Existing snapshot identities use JavaScript's UTF-16 code-unit order.
pub fn compare_serialized_strings(left: &str, right: &str) -> Ordering {
    utf16_units(left).cmp(&utf16_units(right))
}

#[cfg(target_os = "macos")]
mod system_collation {
    // Bun links macOS's ICU 78.1 directly; use the same ICU4C service here.
    use std::ffi::c_char;
    use std::sync::OnceLock;

    #[repr(C)]
    struct UCollator {
        _private: [u8; 0],
    }

    // Safety: these declarations match the stable ICU4C C ABI shipped in the
    // macOS SDK and use an opaque collator pointer only through ICU calls.
    #[link(name = "icucore")]
    unsafe extern "C" {
        fn ucol_open(locale: *const c_char, status: *mut i32) -> *mut UCollator;
        fn ucol_strcollUTF8(
            collator: *const UCollator,
            left: *const c_char,
            left_length: i32,
            right: *const c_char,
            right_length: i32,
            status: *mut i32,
        ) -> i32;
    }

    static COLLATOR: OnceLock<usize> = OnceLock::new();

    fn collator() -> *const UCollator {
        let pointer = *COLLATOR.get_or_init(|| {
            let locale = b"en_US\0";
            let mut status = 0;
            // Safety: `locale` is a static NUL-terminated C string and the
            // status pointer is valid for the duration of the call.
            let collator = unsafe { ucol_open(locale.as_ptr().cast(), &mut status) };
            assert!(
                status <= 0 && !collator.is_null(),
                "macOS ICU could not open the en_US collator"
            );
            collator as usize
        });
        pointer as *const UCollator
    }

    pub(super) fn compare(left: &str, right: &str) -> std::cmp::Ordering {
        let collator = collator();
        let left_length = i32::try_from(left.len()).expect("left path exceeds ICU's byte length");
        let right_length =
            i32::try_from(right.len()).expect("right path exceeds ICU's byte length");
        let mut status = 0;
        // Safety: Rust strings are valid UTF-8, their explicit byte lengths fit
        // ICU's i32 API, and the collator remains alive in the process singleton.
        let result = unsafe {
            ucol_strcollUTF8(
                collator,
                left.as_ptr().cast(),
                left_length,
                right.as_ptr().cast(),
                right_length,
                &mut status,
            )
        };
        assert!(status <= 0, "macOS ICU failed to compare UTF-8 paths");
        match result {
            value if value < 0 => std::cmp::Ordering::Less,
            0 => std::cmp::Ordering::Equal,
            _ => std::cmp::Ordering::Greater,
        }
    }
}

#[cfg(target_os = "macos")]
fn locale_compare(left: &str, right: &str) -> Ordering {
    system_collation::compare(left, right)
}

#[cfg(not(target_os = "macos"))]
fn locale_compare(left: &str, right: &str) -> Ordering {
    static COLLATOR: OnceLock<icu_collator::CollatorBorrowed<'static>> = OnceLock::new();
    let collator = COLLATOR.get_or_init(|| {
        Collator::try_new(locale!("en-US").into(), CollatorOptions::default())
            .expect("compiled ICU collation data")
    });
    collator.compare(left, right)
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(Path::new(std::path::MAIN_SEPARATOR_STR)),
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

fn absolute_root(root: &str) -> Result<PathBuf> {
    if crate::compatibility::trim_js_whitespace(root).is_empty() {
        return Err(error(
            "INVALID_ROOT",
            "Project root must be a non-empty path",
        ));
    }
    let requested = Path::new(root);
    let absolute = if requested.is_absolute() {
        normalize_path(requested)
    } else {
        let current = std::env::current_dir().map_err(|read_error| {
            error(
                "INVALID_ROOT",
                format!("Project root is not readable: {read_error}"),
            )
        })?;
        normalize_path(&current.join(requested))
    };
    let metadata = fs::symlink_metadata(&absolute).map_err(|read_error| {
        details(
            "INVALID_ROOT",
            "Project root is not readable",
            json!({"reason": read_error.to_string(), "root": absolute.to_string_lossy()}),
        )
    })?;
    if metadata.file_type().is_symlink() {
        return Err(error("INVALID_ROOT", "Project root must not be a symlink"));
    }
    if !metadata.is_dir() {
        return Err(error("INVALID_ROOT", "Project root must be a directory"));
    }
    Ok(absolute)
}

fn path_for(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default()
}

fn decode_utf8(bytes: Vec<u8>, path: &str, limit: usize) -> Result<String> {
    if bytes.len() > limit {
        return Err(details(
            "DOCUMENT_TOO_LARGE",
            format!("Markdown source exceeds {limit} bytes"),
            json!({"actualBytes": bytes.len(), "maxBytes": limit, "path": path}),
        ));
    }
    String::from_utf8(bytes).map_err(|_| {
        details(
            "INVALID_UTF8",
            "Markdown source is not valid UTF-8",
            json!({"path": path}),
        )
    })
}

fn read_source(path: &Path, source_path: &str) -> Result<String> {
    let bytes = fs::read(path).map_err(|read_error| {
        details(
            "SOURCE_READ_FAILED",
            "Unable to read Markdown source",
            json!({"path": source_path, "reason": read_error.to_string()}),
        )
    })?;
    decode_utf8(bytes, source_path, MAX_SOURCE_BYTES)
}

fn compile_glob(pattern: &str) -> Option<GlobMatcher> {
    GlobBuilder::new(pattern.trim_start_matches('!'))
        .literal_separator(true)
        .backslash_escape(false)
        .build()
        .ok()
        .map(|glob| glob.compile_matcher())
}

fn validate_pattern(pattern: &Value, field: &str, index: usize) -> Result<String> {
    let Some(value) = pattern.as_str() else {
        return Err(error(
            "INVALID_CONFIG",
            format!("{field}[{index}] must be a non-empty relative glob"),
        ));
    };
    if crate::compatibility::trim_js_whitespace(value).is_empty() {
        return Err(error(
            "INVALID_CONFIG",
            format!("{field}[{index}] must be a non-empty relative glob"),
        ));
    }
    let normalized = value.replace('\\', "/");
    if normalized.starts_with('/')
        || normalized.contains('\0')
        || normalized.split('/').any(|segment| segment == "..")
    {
        return Err(error(
            "INVALID_CONFIG",
            format!("{field}[{index}] must stay inside the project root"),
        ));
    }
    let glob = GlobBuilder::new(normalized.trim_start_matches('!'))
        .literal_separator(true)
        .backslash_escape(false)
        .build()
        .map_err(|glob_error| {
            details(
                "INVALID_CONFIG",
                format!("{field}[{index}] is not a valid glob"),
                json!({"reason": glob_error.to_string()}),
            )
        })?;
    let _ = glob.compile_matcher();
    Ok(normalized)
}

fn parse_patterns(value: Option<&Value>, field: &str, fallback: &[&str]) -> Result<Vec<String>> {
    let Some(value) = value else {
        return Ok(fallback
            .iter()
            .map(|pattern| (*pattern).to_owned())
            .collect());
    };
    let Some(values) = value.as_array() else {
        return Err(error(
            "INVALID_CONFIG",
            format!("{field} must contain at most {MAX_PATTERNS} relative globs"),
        ));
    };
    if values.len() > MAX_PATTERNS {
        return Err(error(
            "INVALID_CONFIG",
            format!("{field} must contain at most {MAX_PATTERNS} relative globs"),
        ));
    }
    values
        .iter()
        .enumerate()
        .map(|(index, pattern)| validate_pattern(pattern, field, index))
        .collect()
}

fn config_from(root: &Path) -> Result<Config> {
    let config_path = root.join("hivex.json");
    let metadata = match fs::symlink_metadata(&config_path) {
        Ok(metadata) => metadata,
        Err(read_error) if read_error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Config {
                exclude: Vec::new(),
                history: Vec::new(),
                include: DEFAULT_INCLUDE
                    .iter()
                    .map(|value| (*value).to_owned())
                    .collect(),
            });
        }
        Err(read_error) => {
            return Err(details(
                "INVALID_CONFIG",
                "Unable to read hivex.json",
                json!({"reason": read_error.to_string()}),
            ));
        }
    };
    if metadata.file_type().is_symlink() {
        return Err(error("INVALID_CONFIG", "hivex.json must not be a symlink"));
    }
    if !metadata.is_file() {
        return Err(error("INVALID_CONFIG", "hivex.json must be a regular file"));
    }
    let bytes = fs::read(&config_path).map_err(|read_error| {
        details(
            "INVALID_CONFIG",
            "Unable to read hivex.json",
            json!({"reason": read_error.to_string()}),
        )
    })?;
    let text = decode_utf8(bytes, "hivex.json", 64 * 1024)?;
    let without_bom = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let value: Value = serde_json::from_str(without_bom).map_err(|parse_error| {
        details(
            "INVALID_CONFIG",
            "hivex.json must contain valid JSON",
            json!({"reason": parse_error.to_string()}),
        )
    })?;
    let Some(object) = value.as_object() else {
        return Err(error("INVALID_CONFIG", "hivex.json must contain an object"));
    };
    if object.contains_key("collections") {
        return Err(error(
            "LEGACY_CONFIGURATION",
            "hivex.json uses legacy collections; replace it with include and exclude globs",
        ));
    }
    if let Some(unknown) = object
        .keys()
        .find(|key| !matches!(key.as_str(), "exclude" | "history" | "include"))
    {
        return Err(error(
            "INVALID_CONFIG",
            format!("hivex.json has unsupported field: {unknown}"),
        ));
    }
    Ok(Config {
        exclude: parse_patterns(object.get("exclude"), "exclude", &[])?,
        history: parse_patterns(object.get("history"), "history", &[])?,
        include: parse_patterns(object.get("include"), "include", &DEFAULT_INCLUDE)?,
    })
}

fn pattern_matches(path: &str, patterns: &[String]) -> bool {
    patterns.iter().any(|pattern| {
        let negated = pattern.bytes().take_while(|byte| *byte == b'!').count() % 2 == 1;
        compile_glob(pattern).is_some_and(|glob| {
            let matches = glob.is_match(path);
            if negated { !matches } else { matches }
        })
    })
}

fn is_excluded_name(name: &str, config: &Config) -> bool {
    if PROTECTED_DIRECTORIES.contains(&name) {
        return true;
    }
    let special = EXCLUDED_DIRECTORIES.contains(&name) || name.starts_with('.');
    if !special {
        return false;
    }
    config
        .include
        .iter()
        .chain(config.history.iter())
        .all(|pattern| !pattern.split('/').any(|segment| segment == name))
}

fn is_excluded_subtree(path: &str, config: &Config) -> bool {
    let subtrees: Vec<_> = config
        .exclude
        .iter()
        .filter(|pattern| pattern.ends_with("/**") && !pattern.starts_with('!'))
        .cloned()
        .collect();
    pattern_matches(&format!("{path}/"), &subtrees)
}

fn collect_candidates(
    current: &Path,
    root: &Path,
    config: &Config,
    warnings: &mut Vec<Warning>,
) -> Vec<Candidate> {
    let mut entries: Vec<_> = match fs::read_dir(current) {
        Ok(entries) => match entries.collect::<std::io::Result<Vec<_>>>() {
            Ok(entries) => entries,
            Err(read_error) => {
                warnings.push(Warning {
                    path: {
                        let path = path_for(root, current);
                        if path.is_empty() {
                            ".".to_owned()
                        } else {
                            path
                        }
                    },
                    message: format!("Unable to inspect directory: {read_error}"),
                });
                return Vec::new();
            }
        },
        Err(read_error) => {
            warnings.push(Warning {
                path: {
                    let path = path_for(root, current);
                    if path.is_empty() {
                        ".".to_owned()
                    } else {
                        path
                    }
                },
                message: format!("Unable to inspect directory: {read_error}"),
            });
            return Vec::new();
        }
    };
    entries.sort_by(|left, right| {
        locale_compare(
            &left.file_name().to_string_lossy(),
            &right.file_name().to_string_lossy(),
        )
    });
    let mut candidates = Vec::new();
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        if is_excluded_name(&name, config) {
            continue;
        }
        let absolute_path = current.join(&name);
        let path = path_for(root, &absolute_path);
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            warnings.push(Warning {
                message: "Skipped symbolic link".to_owned(),
                path,
            });
        } else if file_type.is_dir() {
            if !is_excluded_subtree(&path, config) {
                candidates.extend(collect_candidates(&absolute_path, root, config, warnings));
            }
        } else if file_type.is_file() {
            candidates.push(Candidate {
                absolute_path,
                path,
            });
        }
    }
    candidates
}

fn selected(candidates: &[Candidate], config: &Config) -> (Vec<Candidate>, Vec<Candidate>) {
    let mut available: Vec<_> = candidates
        .iter()
        .filter(|candidate| markdown::is_markdown_path(&candidate.path))
        .filter(|candidate| !pattern_matches(&candidate.path, &config.exclude))
        .cloned()
        .collect();
    available.sort_by(|left, right| locale_compare(&left.path, &right.path));
    let current = available
        .iter()
        .filter(|candidate| {
            pattern_matches(&candidate.path, &config.include)
                && !pattern_matches(&candidate.path, &config.history)
        })
        .cloned()
        .collect();
    let historical = available
        .iter()
        .filter(|candidate| pattern_matches(&candidate.path, &config.history))
        .cloned()
        .collect();
    (current, historical)
}

fn parse_candidate(candidate: &Candidate, historical: bool) -> Result<ParsedDocument> {
    let text = read_source(&candidate.absolute_path, &candidate.path)?;
    let description = markdown::describe_markdown(&candidate.path, &text);
    Ok(ParsedDocument {
        document: Document {
            id: candidate.path.clone(),
            path: candidate.path.clone(),
            title: description.title,
            text: text.clone(),
            hash: markdown::hash(&text),
            status: description.status,
            links: Vec::new(),
            historical,
        },
        raw_links: description.links,
    })
}

fn parse_candidate_with_budget(
    candidate: &Candidate,
    historical: bool,
    source_bytes: usize,
) -> Result<ParsedDocument> {
    let size = fs::symlink_metadata(&candidate.absolute_path)
        .map_err(|read_error| {
            details(
                "SOURCE_READ_FAILED",
                "Unable to read Markdown source",
                json!({"path": candidate.path, "reason": read_error.to_string()}),
            )
        })?
        .len();
    if size > (MAX_CORPUS_BYTES.saturating_sub(source_bytes)) as u64 {
        return Err(error(
            "CORPUS_LIMIT",
            "Selected Markdown exceeds the 64 MiB memory budget; narrow include paths",
        ));
    }
    parse_candidate(candidate, historical)
}

fn percent_decode(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return None;
            }
            let high = (bytes[index + 1] as char).to_digit(16)? as u8;
            let low = (bytes[index + 2] as char).to_digit(16)? as u8;
            decoded.push(high << 4 | low);
            index += 3;
        } else {
            let character = value[index..].chars().next()?;
            let mut buffer = [0; 4];
            decoded.extend_from_slice(character.encode_utf8(&mut buffer).as_bytes());
            index += character.len_utf8();
        }
    }
    String::from_utf8(decoded).ok()
}

fn has_uri_scheme(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() {
        return false;
    }
    for character in chars {
        if character == ':' {
            return true;
        }
        if !(character.is_ascii_alphanumeric() || matches!(character, '+' | '-' | '.')) {
            return false;
        }
    }
    false
}

fn link_path(root: &Path, source: &Document, raw_link: &str) -> Option<String> {
    if raw_link.is_empty() || raw_link.starts_with('#') || has_uri_scheme(raw_link) {
        return None;
    }
    let target_end = raw_link.find(['?', '#']).unwrap_or(raw_link.len());
    let target = &raw_link[..target_end];
    if target.is_empty() {
        return None;
    }
    let decoded = percent_decode(target)?;
    let source_path = root.join(&source.path);
    let parent = source_path.parent().unwrap_or(root);
    let absolute = if Path::new(&decoded).is_absolute() {
        normalize_path(Path::new(&decoded))
    } else {
        normalize_path(&parent.join(decoded))
    };
    let relative = absolute.strip_prefix(root).ok()?;
    if relative.as_os_str().is_empty() {
        return None;
    }
    let relative = relative.to_string_lossy().replace('\\', "/");
    if relative.starts_with("../") || !markdown::is_markdown_path(&relative) {
        return None;
    }
    Some(relative)
}

fn resolve_links(root: &Path, documents: &mut [ParsedDocument]) {
    for parsed in documents {
        let mut links = Vec::new();
        for raw_link in &parsed.raw_links {
            if let Some(link) = link_path(root, &parsed.document, raw_link)
                && !links.iter().any(|existing| existing == &link)
            {
                links.push(link);
            }
        }
        parsed.document.links = links;
    }
}

fn snapshot_for(documents: &[Document], config: &Config) -> String {
    let mut identities: Vec<_> = documents
        .iter()
        .map(|document| format!("{}\0{}", document.id, document.hash))
        .collect();
    identities.sort_by(|left, right| compare_serialized_strings(left, right));
    let identities = identities.join("\n");

    let mut selection = Map::new();
    let sorted = |patterns: &[String]| {
        let mut values = patterns.to_vec();
        values.sort_by(|left, right| compare_serialized_strings(left, right));
        Value::Array(values.into_iter().map(Value::String).collect())
    };
    selection.insert("include".to_owned(), sorted(&config.include));
    selection.insert("exclude".to_owned(), sorted(&config.exclude));
    selection.insert("history".to_owned(), sorted(&config.history));
    let mut ignored = PROTECTED_DIRECTORIES
        .iter()
        .chain(EXCLUDED_DIRECTORIES.iter())
        .map(|value| (*value).to_owned())
        .collect::<Vec<_>>();
    ignored.sort_by(|left, right| compare_serialized_strings(left, right));
    selection.insert(
        "ignoredDirectories".to_owned(),
        Value::Array(ignored.into_iter().map(Value::String).collect()),
    );
    selection.insert(
        "markdownExtensions".to_owned(),
        json!([".md", ".markdown", ".mdown"]),
    );
    markdown::hash(&format!(
        "{}\nselection\0{}",
        identities,
        Value::Object(selection)
    ))
}

fn load_project_inner(root: &str) -> Result<Project> {
    let root = absolute_root(root)?;
    let config = config_from(&root)?;
    let mut warnings = Vec::new();
    let candidates = collect_candidates(&root, &root, &config, &mut warnings);
    let (current, historical) = selected(&candidates, &config);
    let historical_paths: HashSet<_> = historical
        .iter()
        .map(|candidate| candidate.path.clone())
        .collect();
    let selected_candidates = current.into_iter().chain(historical).collect::<Vec<_>>();
    let mut parsed = Vec::new();
    let mut source_bytes = 0usize;
    for candidate in selected_candidates.iter().take(MAX_DOCUMENTS) {
        match parse_candidate_with_budget(
            candidate,
            historical_paths.contains(&candidate.path),
            source_bytes,
        ) {
            Ok(parsed_document) => {
                source_bytes += parsed_document.document.text.len();
                parsed.push(parsed_document);
            }
            Err(failure) => warnings.push(Warning {
                path: candidate.path.clone(),
                message: failure.message,
            }),
        }
    }
    if selected_candidates.len() > MAX_DOCUMENTS {
        warnings.push(Warning {
            path: ".".to_owned(),
            message: format!("Only the first {MAX_DOCUMENTS} Markdown sources were loaded"),
        });
    }
    resolve_links(&root, &mut parsed);
    let mut documents: Vec<_> = parsed
        .into_iter()
        .map(|parsed_document| parsed_document.document)
        .collect();
    documents.sort_by(|left, right| locale_compare(&left.path, &right.path));
    let current_documents = documents
        .iter()
        .filter(|document| !document.historical)
        .cloned()
        .collect::<Vec<_>>();
    let historical_documents = documents
        .iter()
        .filter(|document| document.historical)
        .cloned()
        .collect::<Vec<_>>();
    let snapshot = snapshot_for(&documents, &config);
    let current_snapshot = snapshot_for(&current_documents, &config);
    Ok(Project {
        root,
        snapshot,
        current_snapshot,
        documents,
        current_documents,
        historical_documents,
        warnings,
    })
}

pub fn load_project(root: &str) -> Result<Project> {
    load_project_inner(root)
}
