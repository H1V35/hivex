use crate::arguments;
use crate::documents::load_project;
use crate::error::{HivexError, Result};
use crate::knowledge_model::{empty_graph, graph_value};
use crate::knowledge_snapshot::{
    read_knowledge_snapshot, shared_knowledge, snapshot_report, write_knowledge_snapshot,
};
use crate::source_relocation::relocate_source;
use crate::store::{Store, StoreOptions};
use serde_json::{Value, json};
use std::path::Path;

fn invalid_argument(message: impl Into<String>) -> HivexError {
    HivexError::new("INVALID_ARGUMENT", message)
}

fn store_graph(store: &Store, root: &Path) -> Result<Value> {
    match store.graph() {
        Ok(graph) => Ok(graph),
        Err(error) if error.code == "SNAPSHOT_REQUIRED" => {
            if store.unfinished()? {
                Ok(graph_value(&empty_graph(), false))
            } else {
                shared_knowledge(root)
            }
        }
        Err(error) => Err(error),
    }
}

fn relocation_report(
    project: &crate::documents::Project,
    relocation: &crate::source_relocation::SourceRelocation,
) -> Result<Value> {
    let mut report = snapshot_report(project, &relocation.graph, "relocate")?;
    let object = report
        .as_object_mut()
        .ok_or_else(|| HivexError::new("READ_FAILED", "Snapshot report is not an object"))?;
    object.insert(
        "from".to_owned(),
        json!({
            "document": relocation.from,
            "versions": relocation.from_versions,
        }),
    );
    object.insert("reused".to_owned(), Value::Bool(relocation.reused));
    object.insert(
        "to".to_owned(),
        json!({
            "document": relocation.to,
            "version": relocation.destination_version,
        }),
    );
    Ok(Value::Object(std::mem::take(object)))
}

pub fn command(args: &[String]) -> Result<Value> {
    let parsed = arguments::parse(args, &["root"], &[]).map_err(|mut error| {
        error.code = "INVALID_ARGUMENT".to_owned();
        error
    })?;
    let positionals = &parsed.positionals;
    let operation = positionals.get(1).map(String::as_str);
    let relocating = operation == Some("relocate");
    let expected = if relocating { 4 } else { 2 };
    if positionals.first().map(String::as_str) != Some("snapshot")
        || !matches!(operation, Some("export" | "import" | "relocate"))
        || positionals.len() != expected
    {
        return Err(invalid_argument(
            "Use snapshot export | import | relocate <from> <to> [--root <project>]",
        ));
    }
    let root = parsed.values.get("root").cloned().unwrap_or_else(|| {
        std::env::current_dir().map_or_else(
            |_| ".".to_owned(),
            |path| path.to_string_lossy().into_owned(),
        )
    });
    let project = load_project(&root)?;
    if relocating {
        let mut store = Store::open(
            &project.root,
            StoreOptions {
                readonly: false,
                update: true,
            },
        )?;
        let graph = store_graph(&store, &project.root)?;
        let relocation = relocate_source(
            &graph,
            &project,
            positionals.get(2).map(String::as_str).unwrap_or_default(),
            positionals.get(3).map(String::as_str).unwrap_or_default(),
        )?;
        store.import_graph(&relocation.graph)?;
        return relocation_report(&project, &relocation);
    }
    if operation == Some("import") {
        let incoming = read_knowledge_snapshot(&project.root)?.ok_or_else(|| {
            HivexError::new(
                "SNAPSHOT_NOT_FOUND",
                "No .hivex/graph.json snapshot is available.",
            )
        })?;
        let mut store = Store::open(
            &project.root,
            StoreOptions {
                readonly: false,
                update: true,
            },
        )?;
        store.import_graph(&incoming)?;
        snapshot_report(&project, &incoming, "import")
    } else {
        let store = Store::open(
            &project.root,
            StoreOptions {
                readonly: false,
                update: true,
            },
        )?;
        let graph = store_graph(&store, &project.root)?;
        write_knowledge_snapshot(&project.root, &graph)?;
        snapshot_report(&project, &graph, "export")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use uuid::Uuid;

    fn project_root() -> PathBuf {
        let root = std::env::temp_dir().join(format!("hivex-snapshot-{}", Uuid::new_v4()));
        fs::create_dir(&root).expect("temporary root");
        fs::write(root.join("notes.md"), "# Notes\n\nKeep evidence.\n").expect("source");
        root
    }

    #[test]
    fn export_and_import_roundtrip_empty_v1_graph() {
        let root = project_root();
        let exported = command(&[
            "snapshot".to_owned(),
            "export".to_owned(),
            "--root".to_owned(),
            root.to_string_lossy().into_owned(),
        ])
        .expect("export");
        assert_eq!(exported["operation"], "export");
        assert_eq!(exported["status"], "partial");
        let snapshot = fs::read_to_string(root.join(".hivex/graph.json")).expect("snapshot");
        assert!(snapshot.contains("\"version\": 1"));
        let imported = command(&[
            "snapshot".to_owned(),
            "import".to_owned(),
            "--root".to_owned(),
            root.to_string_lossy().into_owned(),
        ])
        .expect("import");
        assert_eq!(imported["operation"], "import");
        let _ = fs::remove_dir_all(root);
    }
}
