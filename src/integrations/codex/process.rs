use super::*;

pub(super) struct SignalGuard {
    pub(super) previous_interrupt: libc::sighandler_t,
    pub(super) previous_terminate: libc::sighandler_t,
}

extern "C" fn handle_signal(_: i32) {
    CANCELLED.store(true, AtomicOrdering::SeqCst);
}

impl SignalGuard {
    pub(super) fn install() -> Result<Self> {
        CANCELLED.store(false, AtomicOrdering::SeqCst);
        let interrupt = unsafe {
            libc::signal(
                libc::SIGINT,
                handle_signal as *const () as libc::sighandler_t,
            )
        };
        if interrupt == libc::SIG_ERR {
            return Err(HivexError::new(
                "READ_FAILED",
                "Could not install SIGINT handler",
            ));
        }
        let terminate = unsafe {
            libc::signal(
                libc::SIGTERM,
                handle_signal as *const () as libc::sighandler_t,
            )
        };
        if terminate == libc::SIG_ERR {
            unsafe {
                libc::signal(libc::SIGINT, interrupt);
            }
            return Err(HivexError::new(
                "READ_FAILED",
                "Could not install SIGTERM handler",
            ));
        }
        Ok(Self {
            previous_interrupt: interrupt,
            previous_terminate: terminate,
        })
    }
}

impl Drop for SignalGuard {
    fn drop(&mut self) {
        unsafe {
            libc::signal(libc::SIGINT, self.previous_interrupt);
            libc::signal(libc::SIGTERM, self.previous_terminate);
        }
        CANCELLED.store(false, AtomicOrdering::SeqCst);
    }
}

pub(super) fn run_version(
    binary: &str,
    environment: &[(OsString, OsString)],
) -> std::result::Result<String, NativeError> {
    let mut command = Command::new(binary);
    command
        .arg("--version")
        .env_clear()
        .envs(environment.iter().map(|(key, value)| (key, value)))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| NativeError::Admission(error.to_string()))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| NativeError::Admission("Codex version output was unavailable".to_owned()))?;
    let stderr = child.stderr.take().ok_or_else(|| {
        NativeError::Admission("Codex version error output was unavailable".to_owned())
    })?;
    let output_thread = thread::spawn(move || read_capped(stdout, MAX_VERSION_BYTES));
    let error_thread = thread::spawn(move || read_capped(stderr, MAX_VERSION_BYTES));
    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = unsafe { libc::kill(child.id() as libc::pid_t, libc::SIGKILL) };
            let _ = child.wait();
            return Err(NativeError::Admission(
                "Codex CLI version could not be read".to_owned(),
            ));
        }
        thread::sleep(Duration::from_millis(10));
    };
    let stdout = output_thread
        .join()
        .map_err(|_| NativeError::Admission("Codex version reader failed".to_owned()))??;
    let _stderr = error_thread
        .join()
        .map_err(|_| NativeError::Admission("Codex version error reader failed".to_owned()))??;
    if !status.success() || stdout.is_empty() {
        return Err(NativeError::Admission(
            "Codex CLI version could not be read".to_owned(),
        ));
    }
    let version = String::from_utf8(stdout)
        .map_err(|_| NativeError::Admission("Codex CLI version was not UTF-8".to_owned()))?
        .trim()
        .to_owned();
    if version.is_empty() {
        return Err(NativeError::Admission(
            "Codex CLI version could not be read".to_owned(),
        ));
    }
    Ok(version)
}

pub(super) fn read_capped<R: Read>(mut reader: R, limit: usize) -> std::io::Result<Vec<u8>> {
    let mut output = Vec::new();
    let mut chunk = [0_u8; 8192];
    loop {
        let read = reader.read(&mut chunk)?;
        if read == 0 {
            return Ok(output);
        }
        if output.len().saturating_add(read) > limit {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "output exceeds limit",
            ));
        }
        output.extend_from_slice(&chunk[..read]);
    }
}

pub(super) struct NativeServer {
    pub(super) child: Child,
    pub(super) pid: u32,
    pub(super) rpc: Rpc,
    pub(super) workspace: PathBuf,
    pub(super) active_servers: Vec<String>,
    pub(super) admission: Value,
    pub(super) stderr: Option<JoinHandle<()>>,
}

impl NativeServer {
    pub(super) fn stop(mut self) -> std::result::Result<(), NativeError> {
        self.rpc.close_input();
        let mut cleanup_error = None;
        let _ = wait_for_exit(&mut self.child, Duration::from_secs(1))?;
        if group_alive(self.pid)? {
            match signal_group(self.pid, libc::SIGTERM) {
                Ok(true) | Ok(false) => {}
                Err(error) => cleanup_error = Some(error),
            }
            if cleanup_error.is_none()
                && group_alive(self.pid)?
                && !wait_for_group_exit(self.pid, Duration::from_secs(2))?
            {
                let _ = signal_group(self.pid, libc::SIGKILL);
                if !wait_for_group_exit(self.pid, Duration::from_secs(2))? {
                    cleanup_error = Some(NativeError::Io(
                        "Owned Codex process group survived cleanup".to_owned(),
                    ));
                }
            }
        }
        if cleanup_error.is_none() && self.child.try_wait()?.is_none() {
            cleanup_error = Some(NativeError::Io(
                "Native Codex process was not reaped".to_owned(),
            ));
        }
        if self.child.try_wait()?.is_some() {
            let _ = self.child.wait();
            if let Some(reader) = self.rpc.reader.take() {
                let _ = reader.join();
            }
            if let Some(stderr) = self.stderr.take() {
                let _ = stderr.join();
            }
        }
        cleanup_error.map_or(Ok(()), Err)
    }
}

pub(super) fn wait_for_exit(
    child: &mut Child,
    timeout: Duration,
) -> std::result::Result<bool, NativeError> {
    let deadline = Instant::now() + timeout;
    loop {
        if child.try_wait()?.is_some() {
            return Ok(true);
        }
        if Instant::now() >= deadline {
            return Ok(false);
        }
        thread::sleep(Duration::from_millis(10));
    }
}

pub(super) fn wait_for_group_exit(
    pid: u32,
    timeout: Duration,
) -> std::result::Result<bool, NativeError> {
    let deadline = Instant::now() + timeout;
    loop {
        if !group_alive(pid)? {
            return Ok(true);
        }
        if Instant::now() >= deadline {
            return Ok(false);
        }
        thread::sleep(Duration::from_millis(25));
    }
}

pub(super) fn group_alive(pid: u32) -> std::result::Result<bool, NativeError> {
    let result = unsafe { libc::kill(-(pid as libc::pid_t), 0) };
    if result == 0 {
        return Ok(true);
    }
    match std::io::Error::last_os_error().raw_os_error() {
        Some(libc::ESRCH) => Ok(false),
        Some(libc::EPERM) => Ok(true),
        _ => Err(NativeError::Io(std::io::Error::last_os_error().to_string())),
    }
}

pub(super) fn signal_group(pid: u32, signal: i32) -> std::result::Result<bool, NativeError> {
    let result = unsafe { libc::kill(-(pid as libc::pid_t), signal) };
    if result == 0 {
        return Ok(true);
    }
    match std::io::Error::last_os_error().raw_os_error() {
        Some(libc::ESRCH) => Ok(false),
        Some(libc::EPERM) if signal == 0 => Ok(true),
        _ => Err(NativeError::Io(std::io::Error::last_os_error().to_string())),
    }
}

pub(super) fn native_environment() -> Vec<(OsString, OsString)> {
    env::vars_os()
        .filter(|(key, _)| {
            let key = key.to_string_lossy();
            ENVIRONMENT_KEYS.iter().any(|allowed| *allowed == key)
                || (key.starts_with("LC_")
                    && key[3..].chars().all(|c| c.is_ascii_uppercase() || c == '_'))
        })
        .collect()
}

pub(super) fn launch_server(
    binary: &str,
    workspace: &Path,
    disabled_servers: &[String],
    profile: &ExecutionProfile,
) -> std::result::Result<NativeServer, NativeError> {
    let environment = native_environment();
    let native_version = run_version(binary, &environment)
        .map_err(|error| NativeError::PreSpawn(Box::new(error)))?;
    let mut command = Command::new(binary);
    command
        .args(launch_arguments(disabled_servers, profile))
        .current_dir(workspace)
        .env_clear()
        .envs(environment.iter().map(|(key, value)| (key, value)))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    let mut child = command
        .spawn()
        .map_err(|error| NativeError::Admission(error.to_string()))?;
    let pid = child.id();
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| NativeError::Admission("Native Codex stdin was unavailable".to_owned()))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| NativeError::Admission("Native Codex stdout was unavailable".to_owned()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| NativeError::Admission("Native Codex stderr was unavailable".to_owned()))?;
    let stderr = Some(thread::spawn(move || {
        let mut stderr = stderr;
        let mut buffer = [0_u8; 8192];
        loop {
            match stderr.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
        }
    }));
    let mut server = NativeServer {
        child,
        pid,
        rpc: Rpc::new(stdin, stdout),
        workspace: workspace.to_owned(),
        active_servers: Vec::new(),
        admission: Value::Null,
        stderr,
    };
    let mut transcript = Transcript::new();
    let result = (|| {
        server.rpc.request(
            "initialize",
            Some(json!({"clientInfo": {"name": "hivex", "version": APP_CLIENT_VERSION}})),
            INITIALIZE_TIMEOUT,
            &mut transcript,
            true,
        )?;
        server.rpc.notify("initialized", None)?;
        let profile = admit_profile(
            &mut server.rpc,
            &mut transcript,
            &native_version,
            disabled_servers,
            profile,
        )?;
        server.active_servers = profile.active_servers;
        server.admission = profile.admission;
        Ok::<(), NativeError>(())
    })();
    if let Err(error) = result {
        let process_id = server.pid;
        let admission = server.admission.clone();
        let cleanup = server.stop();
        return match cleanup {
            Ok(()) => Err(NativeError::AdmissionProcess {
                cause: Box::new(error),
                process_id,
                admission,
                cleanup_confirmed: true,
            }),
            Err(cleanup_error) => Err(NativeError::AdmissionProcess {
                cause: Box::new(NativeError::Admission(format!(
                    "{}; cleanup failed: {}",
                    error.message(),
                    cleanup_error.message()
                ))),
                process_id,
                admission,
                cleanup_confirmed: false,
            }),
        };
    }
    Ok(server)
}

pub(super) fn start_server(
    binary: &str,
    workspace: &Path,
    profile: &ExecutionProfile,
) -> std::result::Result<NativeServer, NativeError> {
    let initial = launch_server(binary, workspace, &[], profile)?;
    if initial.active_servers.is_empty() {
        return Ok(initial);
    }
    let disabled = initial.active_servers.clone();
    let initial_pid = initial.pid;
    let initial_admission = initial.admission.clone();
    if let Err(error) = initial.stop() {
        return Err(NativeError::AdmissionProcess {
            cause: Box::new(NativeError::Admission(format!(
                "MCP isolation cleanup failed: {}",
                error.message()
            ))),
            process_id: initial_pid,
            admission: initial_admission,
            cleanup_confirmed: false,
        });
    }
    if CANCELLED.load(AtomicOrdering::SeqCst) {
        return Err(NativeError::Cancelled);
    }
    let isolated = launch_server(binary, workspace, &disabled, profile)?;
    if isolated.active_servers.is_empty() {
        return Ok(isolated);
    }
    let process_id = isolated.pid;
    let admission = isolated.admission.clone();
    let (cleanup_confirmed, cause) = match isolated.stop() {
        Ok(()) => (
            true,
            NativeError::Admission(
                "MCP configuration changed or did not honor process-local overrides".to_owned(),
            ),
        ),
        Err(error) => (
            false,
            NativeError::Admission(format!(
                "MCP configuration changed or did not honor process-local overrides; cleanup failed: {}",
                error.message()
            )),
        ),
    };
    Err(NativeError::AdmissionProcess {
        cause: Box::new(cause),
        process_id,
        admission,
        cleanup_confirmed,
    })
}

pub(super) fn stop_after_error(server: NativeServer, error: NativeError) -> (NativeError, bool) {
    match server.stop() {
        Ok(()) => (error, true),
        Err(cleanup_error) => (
            NativeError::Admission(format!(
                "{}; cleanup failed: {}",
                error.message(),
                cleanup_error.message()
            )),
            false,
        ),
    }
}

pub(super) fn unique_workspace() -> Result<PathBuf> {
    let base = env::temp_dir();
    for _ in 0..16 {
        let candidate = base.join(format!("hivex-model-{}", uuid::Uuid::new_v4()));
        match fs::create_dir(&candidate) {
            Ok(()) => {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let mut permissions = fs::metadata(&candidate)?.permissions();
                    permissions.set_mode(0o700);
                    fs::set_permissions(&candidate, permissions)?;
                }
                return fs::canonicalize(&candidate).map_err(|error| {
                    HivexError::new("READ_FAILED", "Could not resolve native workspace")
                        .with_details(json!({"reason": error.to_string()}))
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => {
                return Err(
                    HivexError::new("READ_FAILED", "Could not create native workspace")
                        .with_details(json!({"reason": error.to_string()})),
                );
            }
        }
    }
    Err(HivexError::new(
        "READ_FAILED",
        "Could not create native workspace",
    ))
}
