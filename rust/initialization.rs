use crate::arguments::parse;
use crate::error::{HivexError, Result};
use serde_json::{Map, Value};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};

const IGNORE_RULES: [&str; 3] = ["!/.hivex/", "/.hivex/*", "!/.hivex/graph.json"];
const IGNORE_BLOCK: &str = "!/.hivex/\n/.hivex/*\n!/.hivex/graph.json\n";
static TEMPLATE_FILES: &[(&str, &[u8])] = &[
    (
        "AGENTS.md",
        include_bytes!("../skills/hivex/assets/project/AGENTS.md"),
    ),
    (
        "docs/adr/README.md",
        include_bytes!("../skills/hivex/assets/project/docs/adr/README.md"),
    ),
    (
        "docs/CONTEXT.md",
        include_bytes!("../skills/hivex/assets/project/docs/CONTEXT.md"),
    ),
    (
        "docs/guidelines/engineering.md",
        include_bytes!("../skills/hivex/assets/project/docs/guidelines/engineering.md"),
    ),
    (
        "docs/guidelines/triage-labels.md",
        include_bytes!("../skills/hivex/assets/project/docs/guidelines/triage-labels.md"),
    ),
    (
        "docs/PRD.md",
        include_bytes!("../skills/hivex/assets/project/docs/PRD.md"),
    ),
    (
        "docs/procedures/issue-tracker.md",
        include_bytes!("../skills/hivex/assets/project/docs/procedures/issue-tracker.md"),
    ),
    (
        "docs/README.md",
        include_bytes!("../skills/hivex/assets/project/docs/README.md"),
    ),
    (
        "hivex.json",
        include_bytes!("../skills/hivex/assets/project/hivex.json"),
    ),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum OperationState {
    Created,
    Preserved,
    Updated,
}

struct FileOperation {
    absolute_path: PathBuf,
    bytes: Vec<u8>,
    path: String,
    state: OperationState,
}

struct Destination {
    absolute_path: PathBuf,
    exists: bool,
}

fn error(code: &str, message: impl Into<String>) -> HivexError {
    HivexError::new(code, message)
}

fn normalize_path(path: &Path) -> PathBuf {
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

fn resolve_path(path: &Path) -> std::io::Result<PathBuf> {
    if path.is_absolute() {
        Ok(normalize_path(path))
    } else {
        Ok(normalize_path(&std::env::current_dir()?.join(path)))
    }
}

fn project_root(requested: &str) -> Result<PathBuf> {
    if requested.trim().is_empty() {
        return Err(error(
            "INVALID_ROOT",
            "Project root must be a non-empty path",
        ));
    }

    let root = resolve_path(Path::new(requested)).map_err(|read_error| {
        error(
            "INVALID_ROOT",
            format!("Project root is not readable: {read_error}"),
        )
    })?;
    let metadata = fs::symlink_metadata(&root).map_err(|read_error| {
        error(
            "INVALID_ROOT",
            format!("Project root is not readable: {read_error}"),
        )
    })?;
    if metadata.file_type().is_symlink() {
        return Err(error("INVALID_ROOT", "Project root must not be a symlink"));
    }
    if !metadata.is_dir() {
        return Err(error("INVALID_ROOT", "Project root must be a directory"));
    }
    Ok(root)
}

fn destination(root: &Path, relative_path: &str) -> Result<Destination> {
    let absolute_path = normalize_path(&root.join(relative_path));
    let relative = absolute_path.strip_prefix(root).map_err(|_| {
        error(
            "INVALID_DESTINATION",
            format!("Initialization path escapes the project root: {relative_path}"),
        )
    })?;
    if relative.as_os_str().is_empty() {
        return Err(error(
            "INVALID_DESTINATION",
            format!("Initialization path escapes the project root: {relative_path}"),
        ));
    }

    let components: Vec<_> = relative.components().collect();
    let mut current = root.to_path_buf();
    for (index, component) in components.iter().enumerate() {
        let Component::Normal(part) = component else {
            return Err(error(
                "INVALID_DESTINATION",
                "Initialization paths must be relative project files",
            ));
        };
        current.push(part);
        let metadata = match fs::symlink_metadata(&current) {
            Ok(metadata) => metadata,
            Err(read_error) if read_error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Destination {
                    absolute_path,
                    exists: false,
                });
            }
            Err(read_error) => {
                return Err(error(
                    "INVALID_DESTINATION",
                    format!("Unable to inspect initialization path {relative_path}: {read_error}"),
                ));
            }
        };
        if metadata.file_type().is_symlink() {
            return Err(error(
                "INVALID_DESTINATION",
                format!("Initialization path must not use symlinks: {relative_path}"),
            ));
        }
        let is_final = index == components.len() - 1;
        if (!is_final && !metadata.is_dir()) || (is_final && !metadata.is_file()) {
            return Err(error(
                "INVALID_DESTINATION",
                format!("Initialization path is not a regular file: {relative_path}"),
            ));
        }
    }
    Ok(Destination {
        absolute_path,
        exists: true,
    })
}

fn split_crlf_lines(text: &str) -> impl Iterator<Item = &str> {
    text.split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
}

fn validate_nested_ignore(root: &Path) -> Result<()> {
    let relative_path = ".hivex/.gitignore";
    let target = destination(root, relative_path)?;
    if !target.exists {
        return Ok(());
    }
    let bytes = fs::read(&target.absolute_path).map_err(|read_error| {
        error(
            "INIT_READ_FAILED",
            format!("Unable to read {relative_path}: {read_error}"),
        )
    })?;
    let text = String::from_utf8_lossy(&bytes);
    if split_crlf_lines(&text).any(|line| !line.trim().is_empty() && !line.starts_with('#')) {
        return Err(error(
            "INIT_IGNORE_CONFLICT",
            ".hivex/.gitignore contains rules that can override snapshot visibility or local state privacy.",
        ));
    }
    Ok(())
}

fn has_final_ignore_rules(text: &str) -> bool {
    let mut lines: Vec<_> = split_crlf_lines(text).collect();
    while lines.last() == Some(&"") {
        lines.pop();
    }
    let Some(start) = lines.len().checked_sub(IGNORE_RULES.len()) else {
        return false;
    };
    lines[start..]
        .iter()
        .zip(IGNORE_RULES)
        .all(|(line, rule)| *line == rule)
}

fn ignore_update(existing: Option<&[u8]>) -> Option<Vec<u8>> {
    let Some(existing) = existing else {
        return Some(IGNORE_BLOCK.as_bytes().to_vec());
    };
    let text = String::from_utf8_lossy(existing);
    if has_final_ignore_rules(&text) {
        return None;
    }
    let separator = if existing.is_empty() || text.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    let mut bytes = Vec::with_capacity(existing.len() + separator.len() + IGNORE_BLOCK.len());
    bytes.extend_from_slice(existing);
    bytes.extend_from_slice(separator.as_bytes());
    bytes.extend_from_slice(IGNORE_BLOCK.as_bytes());
    Some(bytes)
}

fn template_operations(root: &Path) -> Result<Vec<FileOperation>> {
    TEMPLATE_FILES
        .iter()
        .map(|(path, bytes)| {
            let target = destination(root, path)?;
            Ok(FileOperation {
                absolute_path: target.absolute_path,
                bytes: bytes.to_vec(),
                path: (*path).to_owned(),
                state: if target.exists {
                    OperationState::Preserved
                } else {
                    OperationState::Created
                },
            })
        })
        .collect()
}

fn ignore_operation(root: &Path) -> Result<FileOperation> {
    let relative_path = ".gitignore";
    let target = destination(root, relative_path)?;
    let existing = if target.exists {
        Some(fs::read(&target.absolute_path).map_err(|read_error| {
            error(
                "INIT_READ_FAILED",
                format!("Unable to read {relative_path}: {read_error}"),
            )
        })?)
    } else {
        None
    };
    let (bytes, state) = match ignore_update(existing.as_deref()) {
        Some(bytes) => (
            bytes,
            if target.exists {
                OperationState::Updated
            } else {
                OperationState::Created
            },
        ),
        None => (existing.unwrap_or_default(), OperationState::Preserved),
    };
    Ok(FileOperation {
        absolute_path: target.absolute_path,
        bytes,
        path: relative_path.to_owned(),
        state,
    })
}

fn write_operations(operations: &[FileOperation]) -> Result<()> {
    for operation in operations {
        if matches!(operation.state, OperationState::Preserved) {
            continue;
        }
        if let Some(parent) = operation.absolute_path.parent() {
            fs::create_dir_all(parent).map_err(|write_error| {
                error(
                    "READ_FAILED",
                    format!("Unable to create initialization directory: {write_error}"),
                )
            })?;
        }
        if matches!(operation.state, OperationState::Created) {
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o644);
            }
            let mut file = options
                .open(&operation.absolute_path)
                .map_err(|write_error| {
                    error(
                        "READ_FAILED",
                        format!("Unable to create {}: {write_error}", operation.path),
                    )
                })?;
            file.write_all(&operation.bytes).map_err(|write_error| {
                error(
                    "READ_FAILED",
                    format!("Unable to write {}: {write_error}", operation.path),
                )
            })?;
        } else {
            fs::write(&operation.absolute_path, &operation.bytes).map_err(|write_error| {
                error(
                    "READ_FAILED",
                    format!("Unable to write {}: {write_error}", operation.path),
                )
            })?;
        }
    }
    Ok(())
}

fn sort_paths(paths: &mut [String]) {
    paths.sort_by(|left, right| {
        left.to_ascii_lowercase()
            .cmp(&right.to_ascii_lowercase())
            .then_with(|| left.cmp(right))
    });
}

fn report(operations: &[FileOperation]) -> Value {
    let paths = |state| {
        let mut paths: Vec<_> = operations
            .iter()
            .filter(|operation| operation.state == state)
            .map(|operation| operation.path.clone())
            .collect();
        sort_paths(&mut paths);
        Value::Array(paths.into_iter().map(Value::String).collect())
    };
    let mut report = Map::new();
    report.insert("command".to_owned(), Value::String("init".to_owned()));
    report.insert("created".to_owned(), paths(OperationState::Created));
    report.insert("modelCalls".to_owned(), Value::from(0));
    report.insert("preserved".to_owned(), paths(OperationState::Preserved));
    report.insert("updated".to_owned(), paths(OperationState::Updated));
    Value::Object(report)
}

pub fn command(args: &[String]) -> Result<Value> {
    let parsed = parse(args, &["root"], &[])
        .map_err(|parse_error| error("INVALID_ARGUMENT", parse_error.to_string()))?;
    if parsed.positionals.len() != 1 || parsed.positionals[0] != "init" {
        return Err(error("INVALID_ARGUMENT", "Use init [--root <project>]"));
    }

    let root = match parsed.values.get("root") {
        Some(requested) => project_root(requested)?,
        None => {
            let current = std::env::current_dir().map_err(|read_error| {
                error(
                    "INVALID_ROOT",
                    format!("Project root is not readable: {read_error}"),
                )
            })?;
            project_root(&current.to_string_lossy())?
        }
    };
    validate_nested_ignore(&root)?;
    let mut operations = template_operations(&root)?;
    operations.push(ignore_operation(&root)?);
    write_operations(&operations)?;
    Ok(report(&operations))
}
