use crate::documents::compare_serialized_strings;
use crate::documents::markdown::{line_content, raw_markdown_lines};
use crate::error::{HivexError, Result};
use crate::knowledge::model::digest;
use regex::Regex;
use serde_json::{Map, Value};
use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const MAX_BYTES: usize = 256 * 1024;
const MAX_FILE_BYTES: usize = 4 * 1024 * 1024;
const MAX_FILES: usize = 64;
const FILE_CONTEXT_BYTES: usize = 32 * 1024;
const GIT_TIMEOUT: Duration = Duration::from_secs(30);
const PROTECTED_DIRECTORIES: [&str; 4] = [".git", ".hivex", "node_modules", ".codex"];

#[derive(Clone, Debug)]
pub(crate) struct Version {
    pub version: String,
    pub lines: Vec<(usize, String)>,
}

#[derive(Clone, Debug)]
pub(crate) struct ImplementationFile {
    pub path: String,
    pub before: Option<Version>,
    pub after: Option<Version>,
}

#[derive(Clone, Debug)]
pub(crate) struct Implementation {
    pub base_commit: String,
    pub fingerprint: String,
    pub diff: String,
    pub files: Vec<ImplementationFile>,
    pub warnings: Vec<String>,
}

fn error(code: &str, message: impl Into<String>) -> HivexError {
    HivexError::new(code, message)
}

fn implementation_too_large(message: impl Into<String>) -> HivexError {
    error("IMPLEMENTATION_TOO_LARGE", message)
}

fn check_size(size: usize, limit: usize) -> Result<()> {
    if size > limit {
        return Err(implementation_too_large(format!(
            "Implementation exceeds {} bytes; split the change into coherent reviews.",
            limit
        )));
    }
    Ok(())
}

fn trim_message(value: &[u8]) -> String {
    String::from_utf8_lossy(value)
        .trim()
        .chars()
        .take(1024)
        .collect()
}

fn read_limited<R: Read>(mut reader: R, limit: usize) -> std::io::Result<Vec<u8>> {
    let mut output = Vec::new();
    let mut chunk = [0_u8; 8192];
    let mut exceeded = false;
    loop {
        let read = reader.read(&mut chunk)?;
        if read == 0 {
            return if exceeded {
                Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "output exceeds limit",
                ))
            } else {
                Ok(output)
            };
        }
        if output.len().saturating_add(read) <= limit {
            output.extend_from_slice(&chunk[..read]);
        } else {
            exceeded = true;
        }
    }
}

fn run_git(root: &Path, arguments: &[String]) -> Result<Vec<u8>> {
    let mut command = Command::new("git");
    command
        .arg("--literal-pathspecs")
        .args(arguments)
        .current_dir(root)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|spawn_error| error("GIT_COMMAND_FAILED", spawn_error.to_string()))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| error("GIT_COMMAND_FAILED", "Git stdout was unavailable"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| error("GIT_COMMAND_FAILED", "Git stderr was unavailable"))?;
    let stdout_reader = thread::spawn(move || read_limited(stdout, MAX_FILE_BYTES + 1));
    let stderr_reader = thread::spawn(move || read_limited(stderr, MAX_FILE_BYTES + 1));
    let deadline = Instant::now() + GIT_TIMEOUT;
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|wait_error| error("GIT_COMMAND_FAILED", wait_error.to_string()))?
        {
            break Some(status);
        }
        if Instant::now() >= deadline {
            timed_out = true;
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        thread::sleep(Duration::from_millis(10));
    };
    let stdout = stdout_reader
        .join()
        .map_err(|_| error("GIT_COMMAND_FAILED", "Git stdout reader failed"))?;
    let stderr = stderr_reader
        .join()
        .map_err(|_| error("GIT_COMMAND_FAILED", "Git stderr reader failed"))?;
    if timed_out {
        return Err(error("GIT_COMMAND_FAILED", "Git command timed out"));
    }
    let stdout = stdout.map_err(|read_error| {
        if read_error.kind() == std::io::ErrorKind::InvalidData {
            implementation_too_large("Git output exceeds the supported implementation size.")
        } else {
            error("GIT_COMMAND_FAILED", read_error.to_string())
        }
    })?;
    let stderr = stderr.map_err(|read_error| {
        if read_error.kind() == std::io::ErrorKind::InvalidData {
            implementation_too_large("Git output exceeds the supported implementation size.")
        } else {
            error("GIT_COMMAND_FAILED", read_error.to_string())
        }
    })?;
    let status = status.ok_or_else(|| error("GIT_COMMAND_FAILED", "Git command did not finish"))?;
    if !status.success() {
        return Err(error("GIT_COMMAND_FAILED", trim_message(&stderr)));
    }
    Ok(stdout)
}

fn git_text(root: &Path, arguments: &[String]) -> Result<String> {
    let bytes = run_git(root, arguments)?;
    String::from_utf8(bytes)
        .map_err(|_| error("GIT_COMMAND_FAILED", "Git output was not valid UTF-8"))
}

fn base64(value: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(value.len().div_ceil(3) * 4);
    for chunk in value.chunks(3) {
        let first = chunk[0];
        output.push(TABLE[(first >> 2) as usize] as char);
        let second = chunk.get(1).copied();
        output.push(
            TABLE[((first & 0x03) << 4 | second.map_or(0, |value| value >> 4)) as usize] as char,
        );
        if let Some(second) = second {
            let third = chunk.get(2).copied();
            output.push(
                TABLE[((second & 0x0f) << 2 | third.map_or(0, |value| value >> 6)) as usize]
                    as char,
            );
            output.push(third.map_or('=', |value| TABLE[(value & 0x3f) as usize] as char));
        } else {
            output.push('=');
            output.push('=');
        }
    }
    output
}

fn unsupported_version(bytes: &[u8], label: &str, warnings: &mut Vec<String>) -> Option<Version> {
    warnings.push(format!(
        "Unsupported binary or invalid UTF-8 content: {} ({})",
        label,
        digest(base64(bytes).as_bytes())
    ));
    None
}

fn version(bytes: Vec<u8>, label: &str, warnings: &mut Vec<String>) -> Result<Option<Version>> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err(implementation_too_large(format!(
            "Implementation exceeds {} bytes; split the change into coherent reviews.",
            MAX_FILE_BYTES
        )));
    }
    let content = match String::from_utf8(bytes.clone()) {
        Ok(content) => content,
        Err(_) => return Ok(unsupported_version(&bytes, label, warnings)),
    };
    if content.contains('\0') {
        return Ok(unsupported_version(&bytes, label, warnings));
    }
    Ok(Some(Version {
        lines: raw_markdown_lines(&content)
            .into_iter()
            .enumerate()
            .map(|(index, line)| (index + 1, line_content(&line).to_owned()))
            .collect(),
        version: digest(&bytes),
    }))
}

fn before_version(
    root: &Path,
    base: &str,
    path: &str,
    warnings: &mut Vec<String>,
) -> Result<Option<Version>> {
    let output = git_text(
        root,
        &[
            "ls-tree".to_owned(),
            "-z".to_owned(),
            base.to_owned(),
            "--".to_owned(),
            path.to_owned(),
        ],
    )?;
    let Some(row) = output
        .split('\0')
        .find(|row| row.split_once('\t').is_some_and(|(_, name)| name == path))
    else {
        return Ok(None);
    };
    let Some((header, _)) = row.split_once('\t') else {
        warnings.push(format!(
            "Unsupported base file: {} (undefined undefined)",
            path
        ));
        return Ok(None);
    };
    let mut fields = header.splitn(3, ' ');
    let mode = fields.next();
    let kind = fields.next();
    let object = fields.next();
    if kind != Some("blob") || mode.is_none() || object.is_none() {
        warnings.push(format!(
            "Unsupported base file: {} ({} {})",
            path,
            mode.unwrap_or("undefined"),
            object.unwrap_or("undefined")
        ));
        return Ok(None);
    }
    let mode = mode.expect("checked above");
    let object = object.expect("checked above");
    if !mode.starts_with("100") {
        warnings.push(format!(
            "Unsupported base file: {} ({} {})",
            path, mode, object
        ));
        return Ok(None);
    }
    let bytes = run_git(
        root,
        &["cat-file".to_owned(), "blob".to_owned(), object.to_owned()],
    )?;
    version(bytes, &format!("before {}", path), warnings)
}

fn is_inside(root: &Path, path: &Path) -> bool {
    path.strip_prefix(root).is_ok()
}

fn after_version(root: &Path, path: &str, warnings: &mut Vec<String>) -> Result<Option<Version>> {
    let absolute = root.join(path);
    let metadata = match fs::symlink_metadata(&absolute) {
        Ok(metadata) => metadata,
        Err(read_error) if read_error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(read_error) => return Err(read_error.into()),
    };
    if metadata.file_type().is_symlink() {
        let target = fs::read_link(&absolute)?;
        let target = target.to_string_lossy();
        warnings.push(format!(
            "Unsupported working symlink: {} ({})",
            path,
            digest(target.as_bytes())
        ));
        return Ok(None);
    }
    let canonical = fs::canonicalize(&absolute).ok();
    if !metadata.is_file()
        || canonical
            .as_deref()
            .is_none_or(|path| !is_inside(root, path))
    {
        warnings.push(format!("Unsupported working file: {}", path));
        return Ok(None);
    }
    let size = usize::try_from(metadata.len()).unwrap_or(usize::MAX);
    if size > MAX_FILE_BYTES {
        return Err(implementation_too_large(format!(
            "Implementation exceeds {} bytes; split the change into coherent reviews.",
            MAX_FILE_BYTES
        )));
    }
    version(fs::read(&absolute)?, &format!("after {}", path), warnings)
}

fn version_value(value: &Version) -> Value {
    let mut object = Map::new();
    object.insert("version".to_owned(), Value::String(value.version.clone()));
    object.insert(
        "lines".to_owned(),
        Value::Array(
            value
                .lines
                .iter()
                .map(|(number, text)| {
                    Value::Array(vec![Value::from(*number), Value::String(text.clone())])
                })
                .collect(),
        ),
    );
    Value::Object(object)
}

fn file_value(file: &ImplementationFile) -> Value {
    let mut object = Map::new();
    object.insert("path".to_owned(), Value::String(file.path.clone()));
    object.insert(
        "before".to_owned(),
        file.before.as_ref().map_or(Value::Null, version_value),
    );
    object.insert(
        "after".to_owned(),
        file.after.as_ref().map_or(Value::Null, version_value),
    );
    Value::Object(object)
}

pub(crate) fn implementation_value(implementation: &Implementation) -> Value {
    let mut object = Map::new();
    object.insert(
        "baseCommit".to_owned(),
        Value::String(implementation.base_commit.clone()),
    );
    object.insert(
        "diff".to_owned(),
        Value::String(implementation.diff.clone()),
    );
    object.insert(
        "files".to_owned(),
        Value::Array(implementation.files.iter().map(file_value).collect()),
    );
    object.insert(
        "warnings".to_owned(),
        Value::Array(
            implementation
                .warnings
                .iter()
                .cloned()
                .map(Value::String)
                .collect(),
        ),
    );
    object.insert(
        "fingerprint".to_owned(),
        Value::String(implementation.fingerprint.clone()),
    );
    Value::Object(object)
}

fn fingerprint_value(
    base_commit: &str,
    diff: &str,
    files: &[ImplementationFile],
    warnings: &[String],
) -> Value {
    let mut object = Map::new();
    object.insert(
        "baseCommit".to_owned(),
        Value::String(base_commit.to_owned()),
    );
    object.insert("diff".to_owned(), Value::String(diff.to_owned()));
    object.insert(
        "files".to_owned(),
        Value::Array(files.iter().map(file_value).collect()),
    );
    object.insert(
        "warnings".to_owned(),
        Value::Array(warnings.iter().cloned().map(Value::String).collect()),
    );
    Value::Object(object)
}

#[derive(Clone, Copy)]
struct Hunk {
    before_start: usize,
    before_length: usize,
    after_start: usize,
    after_length: usize,
}

fn excerpt(version: Option<Version>, hunks: &[Hunk], before: bool) -> Option<Version> {
    version.map(|version| Version {
        lines: version
            .lines
            .into_iter()
            .filter(|(number, _)| {
                hunks.iter().any(|hunk| {
                    let (start, length) = if before {
                        (hunk.before_start, hunk.before_length)
                    } else {
                        (hunk.after_start, hunk.after_length)
                    };
                    *number >= start && *number < start.saturating_add(length)
                })
            })
            .collect(),
        version: version.version,
    })
}

fn file_context(
    root: &Path,
    base: &str,
    file: ImplementationFile,
    warnings: &mut Vec<String>,
) -> Result<ImplementationFile> {
    if serde_json::to_vec(&file_value(&file))?.len() <= FILE_CONTEXT_BYTES {
        return Ok(file);
    }
    let arguments = vec![
        "diff".to_owned(),
        "--no-ext-diff".to_owned(),
        "--no-textconv".to_owned(),
        "--no-renames".to_owned(),
        "--no-color".to_owned(),
        "--src-prefix=a/".to_owned(),
        "--dst-prefix=b/".to_owned(),
        "--unified=3".to_owned(),
        base.to_owned(),
        "--".to_owned(),
        file.path.clone(),
    ];
    let diff = git_text(root, &arguments)?;
    let hunk_regex =
        Regex::new(r"@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@").expect("valid hunk expression");
    let hunks = hunk_regex
        .captures_iter(&diff)
        .filter_map(|captures| {
            Some(Hunk {
                before_start: captures.get(1)?.as_str().parse().ok()?,
                before_length: captures
                    .get(2)
                    .and_then(|value| value.as_str().parse().ok())
                    .unwrap_or(1),
                after_start: captures.get(3)?.as_str().parse().ok()?,
                after_length: captures
                    .get(4)
                    .and_then(|value| value.as_str().parse().ok())
                    .unwrap_or(1),
            })
        })
        .collect::<Vec<_>>();
    if hunks.is_empty() {
        return Ok(file);
    }
    warnings.push(format!(
        "Only changed ranges are supplied for {}; unchanged code is omitted.",
        file.path
    ));
    Ok(ImplementationFile {
        path: file.path,
        before: excerpt(file.before, &hunks, true),
        after: excerpt(file.after, &hunks, false),
    })
}

fn patch(root: &Path, base: &str, paths: &[String]) -> Result<String> {
    if paths.is_empty() {
        return Ok(String::new());
    }
    let mut arguments = vec![
        "diff".to_owned(),
        "--no-ext-diff".to_owned(),
        "--no-textconv".to_owned(),
        "--no-renames".to_owned(),
        "--no-color".to_owned(),
        "--src-prefix=a/".to_owned(),
        "--dst-prefix=b/".to_owned(),
        "--unified=3".to_owned(),
        base.to_owned(),
        "--".to_owned(),
    ];
    arguments.extend(paths.iter().cloned());
    let bytes = run_git(root, &arguments)?;
    String::from_utf8(bytes)
        .map_err(|_| error("GIT_COMMAND_FAILED", "Git diff output was not valid UTF-8"))
}

fn changed_paths(root: &Path, base: &str) -> Result<(Vec<String>, HashSet<String>)> {
    let tracked = git_text(
        root,
        &[
            "diff".to_owned(),
            "--no-ext-diff".to_owned(),
            "--no-textconv".to_owned(),
            "--no-renames".to_owned(),
            "--name-only".to_owned(),
            "-z".to_owned(),
            base.to_owned(),
            "--".to_owned(),
        ],
    )?;
    let untracked = git_text(
        root,
        &[
            "ls-files".to_owned(),
            "--others".to_owned(),
            "--exclude-standard".to_owned(),
            "-z".to_owned(),
        ],
    )?;
    let tracked = tracked
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    let untracked_paths = untracked
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    let untracked_set = untracked_paths.iter().cloned().collect::<HashSet<_>>();
    let mut paths = tracked;
    paths.extend(untracked_paths);
    paths.sort_by(|left, right| compare_serialized_strings(left, right));
    paths.dedup();
    paths.retain(|path| {
        !path.is_empty()
            && path
                .split('/')
                .all(|part| !PROTECTED_DIRECTORIES.contains(&part))
    });
    Ok((paths, untracked_set))
}

pub fn capture_implementation(root: &Path, base: &str) -> Result<Value> {
    let actual_root = fs::canonicalize(root).map_err(|read_error| {
        error(
            "INVALID_ROOT",
            format!("Review from the Git project root: {read_error}"),
        )
    })?;
    let git_root = git_text(
        &actual_root,
        &["rev-parse".to_owned(), "--show-toplevel".to_owned()],
    )?;
    let git_root = fs::canonicalize(git_root.trim()).map_err(|read_error| {
        error(
            "INVALID_ROOT",
            format!("Review from the Git project root: {read_error}"),
        )
    })?;
    if git_root != actual_root {
        return Err(error("INVALID_ROOT", "Review from the Git project root."));
    }
    let base_commit = git_text(
        &actual_root,
        &[
            "rev-parse".to_owned(),
            "--verify".to_owned(),
            "--end-of-options".to_owned(),
            format!("{base}^{{commit}}"),
        ],
    )?
    .trim()
    .to_owned();
    let (paths, untracked) = changed_paths(&actual_root, &base_commit)?;
    if paths.len() > MAX_FILES {
        return Err(implementation_too_large(
            "Implementation exceeds 64 files; split the change into coherent reviews.",
        ));
    }
    let mut warnings = Vec::new();
    let mut files = Vec::with_capacity(paths.len());
    for path in &paths {
        let before = before_version(&actual_root, &base_commit, path, &mut warnings)?;
        let after = after_version(&actual_root, path, &mut warnings)?;
        files.push(file_context(
            &actual_root,
            &base_commit,
            ImplementationFile {
                path: path.clone(),
                before,
                after,
            },
            &mut warnings,
        )?);
    }
    let mut diff = patch(&actual_root, &base_commit, &paths)?;
    for path in &paths {
        if untracked.contains(path) {
            diff.push_str(&format!(
                "\nNew untracked file: {}\n",
                serde_json::to_string(path)?
            ));
        }
    }
    let packet = {
        let mut object = Map::new();
        object.insert("baseCommit".to_owned(), Value::String(base_commit.clone()));
        object.insert("diff".to_owned(), Value::String(diff.clone()));
        object.insert(
            "files".to_owned(),
            Value::Array(files.iter().map(file_value).collect()),
        );
        object.insert(
            "warnings".to_owned(),
            Value::Array(warnings.iter().cloned().map(Value::String).collect()),
        );
        Value::Object(object)
    };
    check_size(serde_json::to_vec(&packet)?.len(), MAX_BYTES)?;
    let fingerprint = digest(
        serde_json::to_string(&fingerprint_value(&base_commit, &diff, &files, &warnings))?
            .as_bytes(),
    );
    let implementation = Implementation {
        base_commit,
        fingerprint,
        diff,
        files,
        warnings,
    };
    Ok(implementation_value(&implementation))
}

pub(crate) fn parse_implementation(value: &Value) -> Result<Implementation> {
    let object = value
        .as_object()
        .ok_or_else(|| error("INVALID_REVIEW", "Implementation packet is invalid"))?;
    let base_commit = object
        .get("baseCommit")
        .and_then(Value::as_str)
        .ok_or_else(|| error("INVALID_REVIEW", "Implementation packet is invalid"))?
        .to_owned();
    let diff = object
        .get("diff")
        .and_then(Value::as_str)
        .ok_or_else(|| error("INVALID_REVIEW", "Implementation packet is invalid"))?
        .to_owned();
    let warnings = object
        .get("warnings")
        .and_then(Value::as_array)
        .ok_or_else(|| error("INVALID_REVIEW", "Implementation packet is invalid"))?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(ToOwned::to_owned)
                .ok_or_else(|| error("INVALID_REVIEW", "Implementation packet is invalid"))
        })
        .collect::<Result<Vec<_>>>()?;
    let files = object
        .get("files")
        .and_then(Value::as_array)
        .ok_or_else(|| error("INVALID_REVIEW", "Implementation packet is invalid"))?
        .iter()
        .map(parse_file)
        .collect::<Result<Vec<_>>>()?;
    let fingerprint = object
        .get("fingerprint")
        .and_then(Value::as_str)
        .ok_or_else(|| error("INVALID_REVIEW", "Implementation packet is invalid"))?
        .to_owned();
    Ok(Implementation {
        base_commit,
        fingerprint,
        diff,
        files,
        warnings,
    })
}

fn parse_version(value: &Value) -> Result<Option<Version>> {
    if value.is_null() {
        return Ok(None);
    }
    let object = value
        .as_object()
        .ok_or_else(|| error("INVALID_REVIEW", "Implementation version is invalid"))?;
    let version = object
        .get("version")
        .and_then(Value::as_str)
        .ok_or_else(|| error("INVALID_REVIEW", "Implementation version is invalid"))?
        .to_owned();
    let lines = object
        .get("lines")
        .and_then(Value::as_array)
        .ok_or_else(|| error("INVALID_REVIEW", "Implementation version is invalid"))?
        .iter()
        .map(|line| {
            let line = line
                .as_array()
                .ok_or_else(|| error("INVALID_REVIEW", "Implementation version is invalid"))?;
            let number = line
                .first()
                .and_then(Value::as_u64)
                .and_then(|number| usize::try_from(number).ok())
                .ok_or_else(|| error("INVALID_REVIEW", "Implementation version is invalid"))?;
            let text = line
                .get(1)
                .and_then(Value::as_str)
                .ok_or_else(|| error("INVALID_REVIEW", "Implementation version is invalid"))?
                .to_owned();
            Ok((number, text))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Some(Version { version, lines }))
}

fn parse_file(value: &Value) -> Result<ImplementationFile> {
    let object = value
        .as_object()
        .ok_or_else(|| error("INVALID_REVIEW", "Implementation file is invalid"))?;
    let path = object
        .get("path")
        .and_then(Value::as_str)
        .filter(|path| !path.is_empty())
        .ok_or_else(|| error("INVALID_REVIEW", "Implementation file is invalid"))?
        .to_owned();
    Ok(ImplementationFile {
        path,
        before: parse_version(object.get("before").unwrap_or(&Value::Null))?,
        after: parse_version(object.get("after").unwrap_or(&Value::Null))?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn root() -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("hivex-implementation-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).expect("temporary root");
        root
    }

    fn git(root: &Path, args: &[&str]) {
        let status = Command::new("git")
            .args(args)
            .current_dir(root)
            .status()
            .expect("git");
        assert!(status.success(), "git command failed: {args:?}");
    }

    fn initial_repository() -> PathBuf {
        let root = root();
        git(&root, &["init", "-q"]);
        git(&root, &["config", "user.name", "Fixture"]);
        git(&root, &["config", "user.email", "fixture@example.invalid"]);
        fs::write(root.join("cache.ts"), "export const purge = true;\n").expect("source");
        git(&root, &["add", "cache.ts"]);
        git(&root, &["commit", "-qm", "Initial"]);
        root
    }

    #[test]
    fn captures_before_after_lines_and_untracked_files() {
        let root = initial_repository();
        fs::write(root.join("cache.ts"), "export const purge = false;\n").expect("change");
        fs::write(root.join("new.ts"), "export const added = true;\n").expect("untracked");
        let value = capture_implementation(&root, "HEAD").expect("implementation");
        let files = value["files"].as_array().expect("files");
        assert_eq!(files.len(), 2);
        assert_eq!(files[0]["path"], "cache.ts");
        assert_eq!(
            files[0]["before"]["lines"][0][1],
            "export const purge = true;"
        );
        assert_eq!(
            files[0]["after"]["lines"][0][1],
            "export const purge = false;"
        );
        assert_eq!(files[1]["path"], "new.ts");
        assert_eq!(value["baseCommit"].as_str().map(str::len), Some(40));
        assert_eq!(value["fingerprint"].as_str().map(str::len), Some(64));
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn bounds_large_file_context_to_changed_hunks() {
        let root = initial_repository();
        let before = (0..2_000)
            .map(|index| format!("const line{index} = 1;\n"))
            .collect::<String>();
        fs::write(root.join("large.ts"), &before).expect("large source");
        git(&root, &["add", "large.ts"]);
        git(&root, &["commit", "-qm", "Large baseline"]);
        let after = before.replace("const line1000 = 1;", "const line1000 = 2;");
        fs::write(root.join("large.ts"), after).expect("large change");
        let value = capture_implementation(&root, "HEAD").expect("implementation");
        let file = value["files"]
            .as_array()
            .expect("files")
            .iter()
            .find(|file| file["path"] == "large.ts")
            .expect("large file");
        assert!(file["after"]["lines"].as_array().expect("excerpt").len() < 20);
        assert!(
            value["warnings"]
                .as_array()
                .expect("warnings")
                .iter()
                .any(|warning| warning
                    .as_str()
                    .is_some_and(|warning| warning.contains("unchanged code")))
        );
        fs::remove_dir_all(root).expect("cleanup");
    }
}
