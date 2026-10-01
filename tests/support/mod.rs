use serde_json::Value;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
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
      .map_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_hivex")), PathBuf::from);
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
    assert_eq!(result.stdout.len(), 0);
    serde_json::from_slice(&result.stderr).unwrap()
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
}
impl Drop for Project {
  fn drop(&mut self) {
    let _ = fs::remove_dir_all(&self.root);
  }
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
