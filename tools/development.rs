//! Development-only release support. Never included in the distribution.
use serde_json::Value;
use std::collections::BTreeSet;
use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn run(command: &mut Command) -> Result<String> {
  let output = command.output()?;
  if !output.status.success() {
    return Err(format!("{command:?}: {}", String::from_utf8_lossy(&output.stderr)).into());
  }
  Ok(String::from_utf8(output.stdout)?)
}

fn metadata(root: &Path, target: &str) -> Result<Value> {
  Ok(serde_json::from_str(&run(
    Command::new("cargo").current_dir(root).args([
      "metadata",
      "--locked",
      "--format-version",
      "1",
      "--filter-platform",
      target,
    ]),
  )?)?)
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
      if !dependency["dep_kinds"]
        .as_array()
        .ok_or("Missing dependency kinds")?
        .iter()
        .any(|kind| kind["kind"] != "dev")
      {
        continue;
      }
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
    let texts = license_texts(package)?;
    write!(
      result,
      "## {} {} ({})\n\n{}\n\n",
      package["name"].as_str().unwrap_or_default(),
      package["version"].as_str().unwrap_or_default(),
      package["license"].as_str().unwrap_or_default(),
      texts.join("\n\n")
    )?;
  }
  Ok(result)
}

fn license_texts(package: &Value) -> Result<Vec<String>> {
  let manifest = Path::new(
    package["manifest_path"]
      .as_str()
      .ok_or("Missing manifest")?,
  );
  let mut texts = Vec::new();
  let mut entries = fs::read_dir(manifest.parent().ok_or("Missing parent")?)?
    .collect::<std::io::Result<Vec<_>>>()?;
  entries.sort_by_key(std::fs::DirEntry::file_name);
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
  Ok(texts)
}

/// Write the third-party license notices that ship with the release binary for a target.
fn main() -> Result<()> {
  let arguments: Vec<String> = std::env::args().collect();
  let (Some("notices"), Some(target), Some(output)) = (
    arguments.get(1).map(String::as_str),
    arguments.get(2),
    arguments.get(3),
  ) else {
    return Err("Usage: cargo run --bin hivex-dev -- notices <target> <output>".into());
  };
  let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
  let metadata = metadata(&root, target)?;
  let package = metadata["resolve"]["root"]
    .as_str()
    .ok_or("Missing root package")?;
  fs::write(output, licenses(&metadata, package)?)?;
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::json;

  #[test]
  fn notices_include_shipped_dependencies_and_exclude_analyzers() {
    let temporary = std::env::temp_dir().join(format!("hivex-notices-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&temporary).unwrap();
    for name in ["runtime", "build"] {
      let directory = temporary.join(name);
      fs::create_dir(&directory).unwrap();
      fs::write(directory.join("LICENSE"), format!("{name} license")).unwrap();
    }
    let packages: Vec<_> = ["runtime", "build", "analyzer"]
      .iter()
      .map(|name| {
        json!({
          "id": name, "name": name, "version": "1.0", "license": "MIT", "source": "registry",
          "manifest_path": temporary.join(name).join("Cargo.toml"),
        })
      })
      .collect();
    let metadata = json!({"packages": packages, "resolve": {"nodes": [
      {"id": "hivex", "deps": [
        {"pkg": "runtime", "dep_kinds": [{"kind": null}]},
        {"pkg": "build", "dep_kinds": [{"kind": "build"}]},
        {"pkg": "analyzer", "dep_kinds": [{"kind": "dev"}]}
      ]},
      {"id": "runtime", "deps": []}, {"id": "build", "deps": []}
    ]}});
    let notices = licenses(&metadata, "hivex").unwrap();
    assert!(notices.contains("runtime license"));
    assert!(notices.contains("build license"));
    assert!(!notices.contains("analyzer"));
    fs::remove_dir_all(temporary).unwrap();
  }
}
