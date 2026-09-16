use crate::arguments;
use crate::error::{HivexError, Result};
use crate::markdown;
use globset::{GlobBuilder, GlobMatcher};
use icu_collator::{Collator, options::CollatorOptions};
use icu_locale_core::locale;
use serde_json::{Map, Value, json};
use std::cmp::Ordering;
use std::collections::HashSet;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;

const DEFAULT_INCLUDE: [&str; 3] = ["**/*.md", "**/*.markdown", "**/*.mdown"];
const DEFAULT_MAX_BYTES: usize = 16_384;
const MAX_OUTPUT_BYTES: usize = 65_536;
const MAX_SOURCE_BYTES: usize = 32 * 1024 * 1024;
const MAX_CORPUS_BYTES: usize = 64 * 1024 * 1024;
const MAX_DOCUMENTS: usize = 2_048;
const MAX_PATTERNS: usize = 64;
const ORIGIN: &str = "current-worktree";
const PROTECTED_DIRECTORIES: [&str; 3] = [".git", ".hivex", "node_modules"];
const EXCLUDED_DIRECTORIES: [&str; 3] = ["vendor", "dist", "build"];
const SAFE_INTEGER_MAX: u64 = 9_007_199_254_740_991;

#[derive(Clone, Debug, PartialEq, Eq)]
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

#[derive(Clone, Debug)]
struct CommandOptions {
    root: String,
    max_bytes: usize,
    from: Option<usize>,
    to: Option<usize>,
    limit: usize,
    cursor: Option<String>,
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
    if root.trim().is_empty() {
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
    GlobBuilder::new(pattern.strip_prefix('!').unwrap_or(pattern))
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
    if value.trim().is_empty() {
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
    let glob = GlobBuilder::new(normalized.strip_prefix('!').unwrap_or(&normalized))
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
        let negated = pattern.starts_with('!');
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

fn positive_integer(value: Option<&String>, label: &str, fallback: Option<usize>) -> Result<usize> {
    let Some(value) = value else {
        return fallback.ok_or_else(|| error("INVALID_ARGUMENT", format!("{label} is required")));
    };
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(error(
            "INVALID_ARGUMENT",
            format!("{label} must be a positive integer"),
        ));
    }
    let number = value.parse::<u64>().map_err(|_| {
        error(
            "INVALID_ARGUMENT",
            format!("{label} must be a positive integer"),
        )
    })?;
    if number == 0 || number > SAFE_INTEGER_MAX {
        return Err(error(
            "INVALID_ARGUMENT",
            format!("{label} must be positive"),
        ));
    }
    Ok(number as usize)
}

fn optional_positive_integer(value: Option<&String>, label: &str) -> Result<Option<usize>> {
    value.map_or(Ok(None), |value| {
        positive_integer(Some(value), label, None).map(Some)
    })
}

fn command_options(args: &[String]) -> Result<(String, Option<String>, CommandOptions)> {
    let parsed = arguments::parse(
        args,
        &["cursor", "from", "limit", "max-bytes", "root", "to"],
        &[],
    )
    .map_err(|mut failure| {
        failure.code = "INVALID_ARGUMENT".to_owned();
        failure
    })?;
    let command = parsed.positionals.first().cloned();
    let id = parsed.positionals.get(1).cloned();
    let extra = parsed.positionals.get(2);
    let Some(command) = command else {
        return Err(error(
            "INVALID_ARGUMENT",
            "Usage: hivex sources | read <id> [options]",
        ));
    };
    if command != "sources" && command != "read" {
        return Err(error(
            "INVALID_ARGUMENT",
            "Usage: hivex sources | read <id> [options]",
        ));
    }
    if command == "sources" && (id.is_some() || extra.is_some()) {
        return Err(error(
            "INVALID_ARGUMENT",
            "sources does not accept a source id",
        ));
    }
    if command == "read" && id.is_none() {
        return Err(error("INVALID_ARGUMENT", "read requires a source id"));
    }
    if command == "read" && extra.is_some() {
        return Err(error("INVALID_ARGUMENT", "read accepts one source id"));
    }
    let max_bytes = positive_integer(
        parsed.values.get("max-bytes"),
        "--max-bytes",
        Some(DEFAULT_MAX_BYTES),
    )?;
    if max_bytes > MAX_OUTPUT_BYTES {
        return Err(error(
            "INVALID_ARGUMENT",
            format!("--max-bytes must be at most {MAX_OUTPUT_BYTES}"),
        ));
    }
    if command == "sources"
        && (parsed.values.contains_key("from") || parsed.values.contains_key("to"))
    {
        return Err(error(
            "INVALID_ARGUMENT",
            "--from and --to are only valid for read",
        ));
    }
    if command == "read"
        && (parsed.values.contains_key("limit") || parsed.values.contains_key("cursor"))
    {
        return Err(error(
            "INVALID_ARGUMENT",
            "--limit and --cursor are only valid for sources",
        ));
    }
    let from = optional_positive_integer(parsed.values.get("from"), "--from")?;
    let to = optional_positive_integer(parsed.values.get("to"), "--to")?;
    let limit = positive_integer(parsed.values.get("limit"), "--limit", Some(20))?;
    let root = parsed.values.get("root").cloned().unwrap_or_else(|| {
        std::env::current_dir().map_or_else(
            |_| ".".to_owned(),
            |path| path.to_string_lossy().into_owned(),
        )
    });
    Ok((
        command,
        id,
        CommandOptions {
            root,
            max_bytes,
            from,
            to,
            limit,
            cursor: parsed.values.get("cursor").cloned(),
        },
    ))
}

fn metadata_value(document: &Document) -> Value {
    let mut value = Map::new();
    value.insert("hash".to_owned(), Value::String(document.hash.clone()));
    value.insert("historical".to_owned(), Value::Bool(document.historical));
    value.insert("id".to_owned(), Value::String(document.id.clone()));
    value.insert(
        "links".to_owned(),
        Value::Array(document.links.iter().cloned().map(Value::String).collect()),
    );
    value.insert("path".to_owned(), Value::String(document.path.clone()));
    value.insert(
        "status".to_owned(),
        document.status.clone().map_or(Value::Null, Value::String),
    );
    value.insert("title".to_owned(), Value::String(document.title.clone()));
    Value::Object(value)
}

fn warnings_value(warnings: &[Warning]) -> Value {
    Value::Array(
        warnings
            .iter()
            .map(|warning| json!({"message": warning.message, "path": warning.path}))
            .collect(),
    )
}

fn bounded_lines(
    lines: &[String],
    start: usize,
    end: usize,
    max_bytes: usize,
) -> Result<(usize, String)> {
    let mut text = String::new();
    let mut prefix = String::new();
    let mut line_end = start - 1;
    for line in start..=end {
        let raw = lines.get(line - 1).map(String::as_str).unwrap_or("");
        let current = if line == lines.len() {
            raw
        } else {
            markdown::line_content(raw)
        };
        let next = format!("{prefix}{current}");
        if next.len() > max_bytes {
            if line_end < start {
                return Err(details(
                    "OUTPUT_LIMIT",
                    "The first requested line exceeds --max-bytes",
                    json!({
                        "line": line,
                        "maxBytes": max_bytes,
                        "requiredBytes": next.len()
                    }),
                ));
            }
            return Ok((line_end, text));
        }
        text = next;
        prefix.push_str(raw);
        line_end = line;
    }
    Ok((line_end, text))
}

fn continuation_value(
    line_end: usize,
    max_bytes: usize,
    requested_end: usize,
    total_lines: usize,
) -> Value {
    if line_end >= total_lines {
        return Value::Null;
    }
    json!({
        "from": line_end + 1,
        "maxBytes": max_bytes,
        "reason": if line_end < requested_end { "max-bytes" } else { "range" },
        "to": total_lines
    })
}

fn read_command(project: &Project, id: &str, options: &CommandOptions) -> Result<Value> {
    let Some(source) = project.documents.iter().find(|document| document.id == id) else {
        return Err(details(
            "SOURCE_NOT_FOUND",
            format!("Markdown source was not selected: {id}"),
            json!({"id": id}),
        ));
    };
    let lines = markdown::raw_markdown_lines(&source.text);
    let start = options.from.unwrap_or(1);
    let requested_end = options.to.unwrap_or(lines.len());
    if start > lines.len() || requested_end > lines.len() || start > requested_end {
        return Err(details(
            "INVALID_RANGE",
            format!("Line range {start}-{requested_end} is outside the source"),
            json!({"id": id, "lineCount": lines.len()}),
        ));
    }
    let (line_end, text) = bounded_lines(&lines, start, requested_end, options.max_bytes)?;
    let continuation = continuation_value(line_end, options.max_bytes, requested_end, lines.len());
    let mut response = Map::new();
    response.insert("command".to_owned(), Value::String("read".to_owned()));
    response.insert("continuation".to_owned(), continuation.clone());
    response.insert("lineEnd".to_owned(), json!(line_end));
    response.insert("lineStart".to_owned(), json!(start));
    response.insert("origin".to_owned(), Value::String(ORIGIN.to_owned()));
    response.insert(
        "snapshot".to_owned(),
        Value::String(project.snapshot.clone()),
    );
    response.insert("source".to_owned(), metadata_value(source));
    response.insert("text".to_owned(), Value::String(text));
    response.insert("truncated".to_owned(), Value::Bool(!continuation.is_null()));
    response.insert("warnings".to_owned(), warnings_value(&project.warnings));
    Ok(Value::Object(response))
}

fn parse_cursor(cursor: &str) -> Option<(&str, usize)> {
    let value = cursor.strip_prefix("s1.")?;
    let (snapshot, start) = value.rsplit_once('.')?;
    if snapshot.len() != 64
        || !snapshot
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return None;
    }
    if start.is_empty() || !start.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let parsed = start.parse::<u64>().ok()?;
    if parsed > SAFE_INTEGER_MAX {
        return None;
    }
    Some((snapshot, parsed as usize))
}

fn sources_response(project: &Project, documents: &[Value], start: usize) -> Value {
    let continuation = if start + documents.len() < project.documents.len() {
        Value::String(format!(
            "s1.{}.{}",
            project.snapshot,
            start + documents.len()
        ))
    } else {
        Value::Null
    };
    let mut response = Map::new();
    response.insert("command".to_owned(), Value::String("sources".to_owned()));
    response.insert("continuation".to_owned(), continuation);
    response.insert("documents".to_owned(), Value::Array(documents.to_owned()));
    response.insert("origin".to_owned(), Value::String(ORIGIN.to_owned()));
    response.insert(
        "snapshot".to_owned(),
        Value::String(project.snapshot.clone()),
    );
    response.insert("totalDocuments".to_owned(), json!(project.documents.len()));
    response.insert("warnings".to_owned(), warnings_value(&project.warnings));
    Value::Object(response)
}

fn list_sources(project: &Project, options: &CommandOptions) -> Result<Value> {
    let start = match options.cursor.as_deref() {
        None => 0,
        Some(cursor) => {
            let Some((snapshot, start)) = parse_cursor(cursor) else {
                return Err(error(
                    "INVALID_CURSOR",
                    "Source continuation belongs to a different or invalid snapshot",
                ));
            };
            if snapshot != project.snapshot {
                return Err(error(
                    "INVALID_CURSOR",
                    "Source continuation belongs to a different or invalid snapshot",
                ));
            }
            if start > 0 && start >= project.documents.len() {
                return Err(error(
                    "INVALID_CURSOR",
                    "Source continuation is outside this snapshot",
                ));
            }
            start
        }
    };
    let end = start
        .saturating_add(options.limit.min(MAX_DOCUMENTS))
        .min(project.documents.len());
    let mut documents = Vec::new();
    for document in &project.documents[start..end] {
        documents.push(metadata_value(document));
        if serde_json::to_vec(&sources_response(project, &documents, start))?.len()
            > options.max_bytes
        {
            documents.pop();
            if documents.is_empty() {
                return Err(error(
                    "OUTPUT_LIMIT",
                    "The next source metadata does not fit; increase --max-bytes or narrow the selected sources",
                ));
            }
            break;
        }
    }
    let response = sources_response(project, &documents, start);
    if serde_json::to_vec(&response)?.len() > options.max_bytes {
        return Err(error(
            "OUTPUT_LIMIT",
            "Source-list metadata exceeds --max-bytes",
        ));
    }
    Ok(response)
}

pub fn command(args: &[String]) -> Result<Value> {
    let (command, id, options) = command_options(args)?;
    let project = load_project(&options.root)?;
    if command == "sources" {
        list_sources(&project, &options)
    } else {
        read_command(&project, id.as_deref().unwrap_or_default(), &options)
    }
}
