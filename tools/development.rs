//! Development-only package preparation and verification. Never included in the distribution.
use rusqlite::Connection;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const TARGET: &str = "aarch64-apple-darwin";

struct Temporary(PathBuf);
impl Temporary {
    fn new() -> Result<Self> {
        let path = std::env::temp_dir().join(format!("hivex-package-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&path)?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
        Ok(Self(path))
    }
}
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn run(command: &mut Command) -> Result<String> {
    let output = command.output()?;
    if !output.status.success() {
        return Err(format!("{command:?}: {}", String::from_utf8_lossy(&output.stderr)).into());
    }
    Ok(String::from_utf8(output.stdout)?)
}

fn digest(path: &Path) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(fs::read(path)?)))
}

fn write_json(path: &Path, value: &Value) -> Result<()> {
    fs::write(path, format!("{}\n", serde_json::to_string_pretty(value)?))?;
    Ok(())
}

fn safe_path(text: &str) -> Result<PathBuf> {
    let path = PathBuf::from(text);
    if text.is_empty()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(format!("Unsafe package path: {text}").into());
    }
    Ok(path)
}

fn copy_public(source: &Path, target: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(source)?;
    if metadata.is_symlink() {
        return Err(format!("Package source is a symlink: {}", source.display()).into());
    }
    if metadata.is_dir() {
        fs::create_dir_all(target)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            copy_public(&entry.path(), &target.join(entry.file_name()))?;
        }
    } else if metadata.is_file() {
        fs::create_dir_all(target.parent().ok_or("Missing parent")?)?;
        fs::copy(source, target)?;
    } else {
        return Err(format!("Non-regular package source: {}", source.display()).into());
    }
    Ok(())
}

fn file_list(root: &Path, directory: &Path) -> Result<BTreeSet<String>> {
    let mut files = BTreeSet::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_dir() {
            files.extend(file_list(root, &entry.path())?);
        } else if kind.is_file() {
            files.insert(
                entry
                    .path()
                    .strip_prefix(root)?
                    .to_str()
                    .ok_or("Invalid path")?
                    .into(),
            );
        } else {
            return Err("Non-regular archive content".into());
        }
    }
    Ok(files)
}

fn licenses(metadata: &Value, package_id: &str) -> Result<String> {
    let mut reachable = BTreeSet::new();
    let mut pending = vec![package_id.to_owned()];
    while let Some(id) = pending.pop() {
        if !reachable.insert(id.clone()) {
            continue;
        }
        let node = metadata["resolve"]["nodes"]
            .as_array()
            .ok_or("Missing dependency graph")?
            .iter()
            .find(|node| node["id"] == id)
            .ok_or("Missing dependency node")?;
        for dependency in node["deps"].as_array().ok_or("Missing dependencies")? {
            pending.push(
                dependency["pkg"]
                    .as_str()
                    .ok_or("Missing package id")?
                    .into(),
            );
        }
    }
    let mut result = String::from("# Third-party licenses\n\n");
    for package in metadata["packages"].as_array().ok_or("Missing packages")? {
        if package["source"].is_null()
            || !reachable.contains(package["id"].as_str().unwrap_or_default())
        {
            continue;
        }
        let manifest = Path::new(
            package["manifest_path"]
                .as_str()
                .ok_or("Missing manifest")?,
        );
        let mut texts = Vec::new();
        let mut entries = fs::read_dir(manifest.parent().ok_or("Missing parent")?)?
            .collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let name = entry.file_name().to_string_lossy().to_uppercase();
            if entry.file_type()?.is_file()
                && ["LICENSE", "COPYING", "NOTICE"]
                    .iter()
                    .any(|prefix| name.starts_with(prefix))
            {
                texts.push(fs::read_to_string(entry.path())?);
            }
        }
        if texts.is_empty() {
            return Err(format!("No license text for {}", package["name"]).into());
        }
        result.push_str(&format!(
            "## {} {} ({})\n\n{}\n\n",
            package["name"].as_str().unwrap_or_default(),
            package["version"].as_str().unwrap_or_default(),
            package["license"].as_str().unwrap_or_default(),
            texts.join("\n\n")
        ));
    }
    Ok(result)
}

fn metadata(root: &Path) -> Result<Value> {
    Ok(serde_json::from_str(&run(Command::new("cargo")
        .current_dir(root)
        .args([
            "metadata",
            "--locked",
            "--format-version",
            "1",
            "--filter-platform",
            TARGET,
        ]))?)?)
}

fn pack(root: &Path, manifest: &Value, archive: &Path) -> Result<()> {
    if !cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        return Err("The distribution is validated only on macOS ARM64".into());
    }
    if fs::symlink_metadata(archive).is_ok() {
        return Err(format!(
            "Refusing to replace an existing artifact: {}",
            archive.display()
        )
        .into());
    }
    validate_manifest(manifest)?;
    let metadata = metadata(root)?;
    let package = metadata["packages"]
        .as_array()
        .ok_or("Missing packages")?
        .iter()
        .find(|package| package["name"] == "hivex")
        .ok_or("Missing Hivex package")?;
    if package["version"] != manifest["version"] {
        return Err("Cargo/npm versions differ".into());
    }
    let mut flags: Vec<String> = match std::env::var("CARGO_ENCODED_RUSTFLAGS") {
        Ok(value) => value.split('\u{1f}').map(str::to_owned).collect(),
        Err(_) => std::env::var("RUSTFLAGS")
            .unwrap_or_default()
            .split_whitespace()
            .map(str::to_owned)
            .collect(),
    };
    flags.push(format!("--remap-path-prefix={}=hivex", root.display()));
    let registries: BTreeSet<_> = metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|package| !package["source"].is_null())
        .filter_map(|package| {
            Path::new(package["manifest_path"].as_str()?)
                .parent()?
                .parent()
        })
        .collect();
    for directory in registries {
        flags.push(format!(
            "--remap-path-prefix={}=crates",
            directory.display()
        ));
    }
    run(Command::new("cargo")
        .current_dir(root)
        .args([
            "build",
            "--bin",
            "hivex",
            "--release",
            "--locked",
            "--target",
            TARGET,
            "--target-dir",
        ])
        .arg(root.join("target"))
        .env("CARGO_ENCODED_RUSTFLAGS", flags.join("\u{1f}")))?;
    let binary = root.join("target").join(TARGET).join("release/hivex");
    let temporary = Temporary::new()?;
    let staged = temporary.0.join("package");
    fs::create_dir_all(staged.join("bin"))?;
    fs::copy(&binary, staged.join("bin/hivex"))?;
    fs::set_permissions(staged.join("bin/hivex"), fs::Permissions::from_mode(0o755))?;
    fs::write(
        staged.join("THIRD-PARTY-NOTICES.txt"),
        licenses(&metadata, package["id"].as_str().ok_or("Missing id")?)?,
    )?;
    for file in manifest["files"]
        .as_array()
        .ok_or("Missing public allowlist")?
    {
        let file = file.as_str().ok_or("Invalid public path")?;
        if ["bin/hivex", "THIRD-PARTY-NOTICES.txt"].contains(&file) {
            continue;
        }
        let relative = safe_path(file.trim_end_matches('/'))?;
        copy_public(&root.join(&relative), &staged.join(&relative))?;
    }
    write_json(&staged.join("package.json"), manifest)?;
    fs::create_dir_all(archive.parent().ok_or("Missing archive directory")?)?;
    run(Command::new("/usr/bin/tar")
        .env("COPYFILE_DISABLE", "1")
        .args(["-czf"])
        .arg(archive)
        .arg("-C")
        .arg(&temporary.0)
        .arg("package"))?;
    let report = json!({"archive":archive,"files":file_list(&staged,&staged)?,"nativeSha256":digest(&binary)?,"sha256":digest(archive)?,"target":TARGET,"version":manifest["version"]});
    write_json(
        &PathBuf::from(format!("{}.json", archive.display())),
        &report,
    )?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn validate_manifest(manifest: &Value) -> Result<()> {
    if manifest["name"] != "@h1v35/hivex"
        || manifest["os"] != json!(["darwin"])
        || manifest["cpu"] != json!(["arm64"])
        || manifest["bin"] != json!({"hivex":"bin/hivex"})
    {
        return Err("Invalid native package metadata".into());
    }
    for field in [
        "dependencies",
        "devDependencies",
        "optionalDependencies",
        "scripts",
        "packageManager",
    ] {
        if !manifest[field].is_null() {
            return Err(
                format!("Unexpected development/runtime dependency metadata: {field}").into(),
            );
        }
    }
    Ok(())
}

fn cli(binary: &Path, root: &Path, arguments: &[&str]) -> Result<Value> {
    Ok(serde_json::from_str(&run(Command::new(binary)
        .env("PATH", "/usr/bin:/bin")
        .args(arguments)
        .arg("--root")
        .arg(root))?)?)
}

fn verify(root: &Path, manifest: &Value, archive: &Path) -> Result<()> {
    let report: Value = serde_json::from_slice(&fs::read(format!("{}.json", archive.display()))?)?;
    if report["sha256"] != digest(archive)? || report["version"] != manifest["version"] {
        return Err("Archive identity differs from its report/current manifest".into());
    }
    let listing = run(Command::new("/usr/bin/tar").arg("-tzf").arg(archive))?;
    let mut files = BTreeSet::new();
    for entry in listing.lines() {
        let path = safe_path(entry.trim_end_matches('/'))?;
        let relative = path.strip_prefix("package")?;
        if !entry.ends_with('/')
            && !files.insert(relative.to_str().ok_or("Invalid path")?.to_owned())
        {
            return Err("Duplicate archive entry".into());
        }
    }
    let expected: BTreeSet<String> = serde_json::from_value(report["files"].clone())?;
    if files != expected {
        return Err("Archive file list differs from its report".into());
    }
    // Reject links and special files before extracting an archive.
    let details = run(Command::new("/usr/bin/tar").arg("-tvzf").arg(archive))?;
    if details
        .lines()
        .any(|line| !line.starts_with('-') && !line.starts_with('d'))
    {
        return Err("Archive contains links or special files".into());
    }
    for file in &files {
        let allowed = file == "package.json"
            || manifest["files"]
                .as_array()
                .ok_or("Missing allowlist")?
                .iter()
                .any(|value| {
                    let entry = value.as_str().unwrap_or_default();
                    file == entry || (entry.ends_with('/') && file.starts_with(entry))
                });
        if !allowed
            || file
                .split('/')
                .any(|part| [".hivex", "node_modules", "evidence", "target"].contains(&part))
            || file.contains(".sqlite")
        {
            return Err(format!("Unexpected archive file: {file}").into());
        }
    }
    for skill in [
        "hivex",
        "hivex-design",
        "hivex-document",
        "hivex-implement",
        "hivex-review",
        "hivex-git",
    ] {
        if !files.contains(&format!("skills/{skill}/SKILL.md")) {
            return Err("Missing skill".into());
        }
    }
    for file in [
        "THIRD-PARTY-NOTICES.txt",
        "templates/README.md",
        "bin/hivex",
        "README.md",
        "docs/guide.md",
        "skills/hivex/assets/project/CLAUDE.md",
        "LICENSE",
    ] {
        if !files.contains(file) {
            return Err(format!("Missing package file: {file}").into());
        }
    }
    let temporary = Temporary::new()?;
    run(Command::new("/usr/bin/tar")
        .arg("-xzf")
        .arg(archive)
        .arg("-C")
        .arg(&temporary.0))?;
    let package = temporary.0.join("package");
    let installed: Value = serde_json::from_slice(&fs::read(package.join("package.json"))?)?;
    validate_manifest(&installed)?;
    if installed != *manifest || file_list(&package, &package)? != files {
        return Err("Extracted package differs".into());
    }
    let binary = package.join("bin/hivex");
    if fs::metadata(&binary)?.permissions().mode() & 0o111 == 0
        || report["nativeSha256"] != digest(&binary)?
    {
        return Err("Invalid executable".into());
    }
    if cli(&binary, &temporary.0, &["--help"])?["application"] != "hivex" {
        return Err("Invalid help".into());
    }
    let initialized = temporary.0.join("initialized");
    fs::create_dir_all(initialized.join("node_modules/@h1v35"))?;
    std::os::unix::fs::symlink(&package, initialized.join("node_modules/@h1v35/hivex"))?;
    cli(&binary, &initialized, &["init"])?;
    if fs::read_to_string(initialized.join("CLAUDE.md"))? != "@AGENTS.md\n" {
        return Err("Missing shared agent instructions".into());
    }
    for skill in [
        "hivex",
        "hivex-design",
        "hivex-document",
        "hivex-implement",
        "hivex-review",
        "hivex-git",
    ] {
        for family in [".agents", ".claude"] {
            let linked = initialized.join(family).join("skills").join(skill);
            if family == ".agents"
                && fs::read_link(&linked)?
                    != Path::new(&format!("../../node_modules/@h1v35/hivex/skills/{skill}"))
            {
                return Err("Skill link bypasses the project dependency".into());
            }
            if fs::read_link(&linked)?.is_absolute()
                || fs::canonicalize(linked)?
                    != fs::canonicalize(package.join("skills").join(skill))?
            {
                return Err("Invalid installed skill link".into());
            }
        }
    }
    if cli(&binary, &initialized, &["init"])?["created"] != json!([]) {
        return Err("Initialization is not repeatable".into());
    }
    let project = temporary.0.join("project");
    fs::create_dir_all(project.join(".hivex"))?;
    let source = "# Policy\nUse bounded work.\nPreserve the budget.\n";
    fs::write(project.join("notes.md"), source)?;
    let database = Connection::open(project.join(".hivex/knowledge.sqlite"))?;
    database.execute_batch(&fs::read_to_string(
        root.join("tests/fixtures/knowledge-cache-v1.sql"),
    )?)?;
    let graph_before: String =
        database.query_row("SELECT data FROM graph", [], |row| row.get(0))?;
    let cache_before = cache_rows(&database)?;
    let no_model = temporary.0.join("no-model");
    let answer = cli(
        &binary,
        &project,
        &[
            "ask",
            "bounded",
            "--source",
            "notes.md",
            "--max-calls",
            "1",
            "--codex",
            no_model.to_str().ok_or("Invalid path")?,
        ],
    )?;
    if answer["answer"] != "Use bounded work and preserve the budget."
        || answer["status"] != "ready"
        || answer["work"]["calls"] != 1
        || answer["work"]["id"] != "84171802-e68b-43d8-b327-9ff47d302375"
    {
        return Err("Retained v1 answer/budget changed".into());
    }
    let graph_after: String = database.query_row("SELECT data FROM graph", [], |row| row.get(0))?;
    if graph_after != graph_before || cache_rows(&database)? != cache_before {
        return Err("Retained graph/cache changed".into());
    }
    if cli(&binary, &project, &["read", "notes.md"])?["text"] != source
        || cli(&binary, &project, &["sources"])?["totalDocuments"] != 1
    {
        return Err("Source contract changed".into());
    }
    cli(&binary, &project, &["snapshot", "export"])?;
    cli(&binary, &project, &["warnings"])?;
    let verification = json!({"archive":archive,"sha256":report["sha256"],"nativeSha256":report["nativeSha256"],"target":TARGET,"checks":["public-allowlist-six-skills","extracted-native-executable","no-runtime-or-development-js-dependencies","no-bun-node-in-path","installed-init-and-skills","retained-v1-answer-budget","unchanged-graph-cache","source-read-snapshot"]});
    write_json(
        &PathBuf::from(format!("{}.verification.json", archive.display())),
        &verification,
    )?;
    println!("{}", serde_json::to_string_pretty(&verification)?);
    Ok(())
}

fn cache_rows(database: &Connection) -> Result<Vec<(String, String)>> {
    Ok(database
        .prepare("SELECT key,value FROM model_cache ORDER BY key")?
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<std::result::Result<_, _>>()?)
}

fn main() -> Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let manifest: Value = serde_json::from_slice(&fs::read(root.join("package.json"))?)?;
    let arguments: Vec<String> = std::env::args().collect();
    let archive = arguments.get(2).map(PathBuf::from).unwrap_or_else(|| {
        root.join(format!(
            "dist/h1v35-hivex-{}.tgz",
            manifest["version"].as_str().unwrap_or_default()
        ))
    });
    let archive = if archive.is_absolute() {
        archive
    } else {
        std::env::current_dir()?.join(archive)
    };
    match arguments.get(1).map(String::as_str) {
        Some("pack") => pack(&root, &manifest, &archive),
        Some("verify") => verify(&root, &manifest, &archive),
        _ => Err("Usage: cargo run --bin hivex-dev -- <pack|verify> [archive.tgz]".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn package_preparation_preserves_existing_artifacts_and_dangling_links() {
        if !cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            return;
        }
        let temporary = Temporary::new().unwrap();
        let archive = temporary.0.join("approved.tgz");
        fs::write(&archive, b"approved artifact").unwrap();
        assert!(
            pack(&temporary.0, &Value::Null, &archive)
                .unwrap_err()
                .to_string()
                .contains("Refusing to replace")
        );
        assert_eq!(fs::read(&archive).unwrap(), b"approved artifact");
        let link = temporary.0.join("dangling.tgz");
        let target = temporary.0.join("unintended.tgz");
        symlink(&target, &link).unwrap();
        assert!(
            pack(&temporary.0, &Value::Null, &link)
                .unwrap_err()
                .to_string()
                .contains("Refusing to replace")
        );
        assert!(fs::symlink_metadata(&link).unwrap().is_symlink());
        assert!(!target.exists());
    }
}
