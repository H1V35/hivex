use rusqlite::{Connection, params};
use serde_json::{Value, json};
use std::fs;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

pub struct Project {
    pub root: PathBuf,
}
impl Project {
    pub fn new() -> Self {
        let root = std::env::temp_dir().join(format!("hivex-contract-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        Self { root }
    }
    pub fn policy() -> Self {
        let p = Self::new();
        p.write(
            "cache.md",
            "# Cache\n\nCached data expires after seven days.\n",
        );
        p.write(
            "privacy.md",
            "# Access\n\nRevoking access immediately removes cached private data.\n",
        );
        p.model("");
        p
    }
    pub fn path(&self, file: &str) -> PathBuf {
        self.root.join(file)
    }
    pub fn write(&self, file: &str, text: impl AsRef<[u8]>) {
        let path = self.path(file);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    pub fn json(&self, file: &str, value: &Value) {
        self.write(file, value.to_string());
    }
    pub fn read_json(&self, file: &str) -> Value {
        serde_json::from_slice(&fs::read(self.path(file)).unwrap()).unwrap()
    }
    pub fn command(&self, args: &[&str]) -> Command {
        let binary = std::env::var_os("HIVEX_TEST_BINARY")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_hivex")));
        let mut command = Command::new(binary);
        command.args(args);
        if !args.is_empty() {
            command.arg("--root").arg(&self.root);
        }
        command
    }
    pub fn raw(&self, args: &[&str]) -> Output {
        bounded(self.command(args))
    }
    pub fn cli(&self, args: &[&str]) -> Value {
        let result = self.raw(args);
        assert!(
            !result.stdout.is_empty(),
            "{args:?}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        serde_json::from_slice(&result.stdout).unwrap()
    }
    pub fn ok(&self, args: &[&str]) -> Value {
        let result = self.raw(args);
        assert!(
            result.status.success(),
            "{args:?}: {} {}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        serde_json::from_slice(&result.stdout).unwrap()
    }
    pub fn error(&self, args: &[&str]) -> Value {
        let result = self.raw(args);
        assert_eq!(result.status.code(), Some(1), "{args:?}");
        assert!(result.stdout.is_empty());
        serde_json::from_slice(&result.stderr).unwrap()
    }
    pub fn model(&self, scenario: &str) {
        let program = env!("CARGO_BIN_EXE_hivex-test-codex");
        let wrapper = format!(
            "#!/bin/sh\nexport HIVEX_TEST_SCENARIO={}\nexport HIVEX_TEST_RESPONSES={}\nexport HIVEX_TEST_CALLS_FILE={}\nexport HIVEX_TEST_PID_PATH={}\nexec {} \"$@\"\n",
            quote(scenario),
            quote(self.path("responses.json").to_str().unwrap()),
            quote(self.path("calls.log").to_str().unwrap()),
            quote(self.path("pids.json").to_str().unwrap()),
            quote(program)
        );
        self.write("codex", wrapper);
        fs::set_permissions(self.path("codex"), fs::Permissions::from_mode(0o700)).unwrap();
        if !self.path("responses.json").exists() {
            self.json("responses.json", &responses());
        }
    }
    pub fn model_cli(&self, args: &[&str]) -> Value {
        let model = self.path("codex");
        let mut args = args.to_vec();
        args.extend(["--codex", model.to_str().unwrap()]);
        self.cli(&args)
    }
    pub fn calls(&self) -> usize {
        fs::read_to_string(self.path("calls.log"))
            .unwrap_or_default()
            .lines()
            .count()
    }
    pub fn db(&self) -> Connection {
        fs::create_dir_all(self.path(".hivex")).unwrap();
        let db = Connection::open(self.path(".hivex/knowledge.sqlite")).unwrap();
        db.execute_batch("CREATE TABLE IF NOT EXISTS graph (id INTEGER PRIMARY KEY CHECK(id=1), data TEXT NOT NULL); CREATE TABLE IF NOT EXISTS work (id TEXT PRIMARY KEY, kind TEXT NOT NULL,key TEXT NOT NULL,data TEXT NOT NULL); CREATE TABLE IF NOT EXISTS model_cache (key TEXT PRIMARY KEY,value TEXT NOT NULL);").unwrap();
        db
    }
    pub fn graph(&self) -> Value {
        let text: String = self
            .db()
            .query_row("SELECT data FROM graph WHERE id=1", [], |row| row.get(0))
            .unwrap();
        serde_json::from_str(&text).unwrap()
    }
    pub fn set_graph(&self, value: &Value) {
        self.db()
            .execute(
                "INSERT OR REPLACE INTO graph VALUES(1,?)",
                [value.to_string()],
            )
            .unwrap();
    }
    pub fn work(&self, id: &str) -> Value {
        let text: String = self
            .db()
            .query_row("SELECT data FROM work WHERE id=?", [id], |row| row.get(0))
            .unwrap();
        serde_json::from_str(&text).unwrap()
    }
    pub fn set_work(&self, value: &Value) {
        self.db()
            .execute(
                "INSERT OR REPLACE INTO work VALUES(?,?,?,?)",
                params![
                    value["id"].as_str().unwrap(),
                    value["kind"].as_str().unwrap(),
                    value["key"].as_str().unwrap(),
                    value.to_string()
                ],
            )
            .unwrap();
    }
    pub fn sql_fixture(&self, name: &str) {
        fs::create_dir_all(self.path(".hivex")).unwrap();
        Connection::open(self.path(".hivex/knowledge.sqlite"))
            .unwrap()
            .execute_batch(
                &fs::read_to_string(
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("tests/fixtures")
                        .join(name),
                )
                .unwrap(),
            )
            .unwrap();
    }
    pub fn git(&self, args: &[&str]) -> String {
        let output = Command::new("git")
            .current_dir(&self.root)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }
    pub fn git_init(&self) {
        self.git(&["init", "-q"]);
        self.git(&["config", "user.name", "Fixture"]);
        self.git(&["config", "user.email", "fixture@example.invalid"]);
        self.git(&["add", "*.md"]);
        self.git(&["commit", "-qm", "fixture"]);
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
pub fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\"'\"'"))
}
pub fn bounded(mut command: Command) -> Output {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn().unwrap();
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let out = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).unwrap();
        bytes
    });
    let err = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stderr.read_to_end(&mut bytes).unwrap();
        bytes
    });
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if started.elapsed() > Duration::from_secs(20) {
            let _ = child.kill();
            let _ = child.wait();
            panic!("CLI exceeded contract deadline: {command:?}");
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    Output {
        status,
        stdout: out.join().unwrap(),
        stderr: err.join().unwrap(),
    }
}
pub fn subset(actual: &Value, expected: &Value) {
    match expected {
        Value::Object(fields) => {
            for (key, value) in fields {
                assert!(actual.get(key).is_some(), "missing {key}: {actual}");
                subset(&actual[key], value);
            }
        }
        Value::Array(values) => {
            assert_eq!(
                actual.as_array().map(Vec::len),
                Some(values.len()),
                "{actual}"
            );
            for (a, b) in actual.as_array().unwrap().iter().zip(values) {
                subset(a, b);
            }
        }
        _ => assert_eq!(actual, expected),
    }
}
pub fn list<'a>(value: &'a Value, key: &str) -> &'a Vec<Value> {
    value[key].as_array().unwrap()
}
pub fn empty_graph() -> Value {
    json!({"version":1,"documents":{},"decisions":[],"relationships":[],"units":{},"warnings":[]})
}
pub fn work(id: &str) -> Value {
    json!({"id":id,"key":id,"kind":"ask","snapshot":"fixture","maxCalls":3,"maxInputBytes":131072,"remaining":[],"plannedUnits":[],"pending":null,"phase":"ask","materializedChecks":true,"status":"pending","calls":0,"inputBytes":0,"totalTokens":0,"cacheHits":0,"attempts":[]})
}
pub fn decision(document: &str, id: &str, line: u64, text: &str) -> Value {
    json!({"document":document,"id":id,"lineStart":line,"lineEnd":line,"text":text,"conditions":[],"exceptions":[],"reason":"Preserve the documented rule.","kind":"constraint","status":"current"})
}
pub fn responses() -> Value {
    json!({"capturePackets":true,"ask":{"answer":"Revoking access removes private cached data immediately; seven days is the ordinary lifetime.","evidence":[{"document":"privacy.md","lineStart":3,"lineEnd":3}],"uncertainties":[]},"check":{"findings":[]},"extract":{"decisions":[decision("cache.md","c1",3,"Cached data expires after seven days."),decision("privacy.md","c2",3,"Access revocation immediately purges private cache.")],"relationships":[{"id":"r1","from":"c2","to":"c1","type":"exception-to","reason":"Revocation overrides ordinary retention.","evidence":[{"document":"cache.md","lineStart":3,"lineEnd":3},{"document":"privacy.md","lineStart":3,"lineEnd":3}]}],"uncertainties":[]}})
}
