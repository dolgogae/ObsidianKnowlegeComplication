use std::ffi::OsString;
use std::fmt::{Formatter, Result as FormatResult};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde::de::{DeserializeOwned, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use vaultc::{Result, VaultcError};
use vaultc_protocol::{
    AugmentationRequest, AugmentationResponse, Envelope, MessageType, PROTOCOL_VERSION,
    ProtocolError, ProviderCapabilities, ProviderOperation,
};

const HARD_MAX_OUTPUT_BYTES: usize = 64 * 1024 * 1024;
const HARD_MAX_LINE_BYTES: usize = 32 * 1024 * 1024;
const HARD_MAX_MESSAGES: usize = 64;
const HARD_MAX_TIMEOUT: Duration = Duration::from_hours(1);
const STDERR_CAPTURE_BYTES: usize = 64 * 1024;
const PROTOCOL_CLOSE_GRACE: Duration = Duration::from_millis(250);
const PROVIDER_POLL_INTERVAL: Duration = Duration::from_millis(50);
const THREAD_JOIN_GRACE: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, Default)]
pub(crate) struct ProviderCancellation(Arc<AtomicBool>);

impl ProviderCancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CommandProviderConfig {
    pub program: PathBuf,
    pub arguments: Vec<OsString>,
    pub working_directory: Option<PathBuf>,
    pub timeout: Duration,
    pub max_line_bytes: usize,
    pub max_output_bytes: usize,
    pub max_messages: usize,
    pub cancellation: ProviderCancellation,
}

#[derive(Debug)]
pub(crate) struct CommandProvider {
    config: CommandProviderConfig,
}

#[derive(Debug)]
pub(crate) struct ProviderRun<A> {
    pub authorization: A,
    pub response: AugmentationResponse,
}

struct UniqueJsonValue(Value);

impl<'de> Deserialize<'de> for UniqueJsonValue {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(UniqueJsonVisitor)
    }
}

struct UniqueJsonVisitor;

impl<'de> Visitor<'de> for UniqueJsonVisitor {
    type Value = UniqueJsonValue;

    fn expecting(&self, formatter: &mut Formatter<'_>) -> FormatResult {
        formatter.write_str("a JSON value without duplicate object keys")
    }

    fn visit_bool<E>(self, value: bool) -> std::result::Result<Self::Value, E> {
        Ok(UniqueJsonValue(Value::Bool(value)))
    }

    fn visit_i64<E>(self, value: i64) -> std::result::Result<Self::Value, E> {
        Ok(UniqueJsonValue(Value::Number(value.into())))
    }

    fn visit_u64<E>(self, value: u64) -> std::result::Result<Self::Value, E> {
        Ok(UniqueJsonValue(Value::Number(value.into())))
    }

    fn visit_f64<E>(self, value: f64) -> std::result::Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        let number = serde_json::Number::from_f64(value)
            .ok_or_else(|| E::custom("JSON number must be finite"))?;
        Ok(UniqueJsonValue(Value::Number(number)))
    }

    fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E> {
        Ok(UniqueJsonValue(Value::String(value.to_owned())))
    }

    fn visit_string<E>(self, value: String) -> std::result::Result<Self::Value, E> {
        Ok(UniqueJsonValue(Value::String(value)))
    }

    fn visit_none<E>(self) -> std::result::Result<Self::Value, E> {
        Ok(UniqueJsonValue(Value::Null))
    }

    fn visit_unit<E>(self) -> std::result::Result<Self::Value, E> {
        Ok(UniqueJsonValue(Value::Null))
    }

    fn visit_seq<A>(self, mut sequence: A) -> std::result::Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut values = Vec::new();
        while let Some(UniqueJsonValue(value)) = sequence.next_element()? {
            values.push(value);
        }
        Ok(UniqueJsonValue(Value::Array(values)))
    }

    fn visit_map<A>(self, mut object: A) -> std::result::Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut values = serde_json::Map::new();
        while let Some(key) = object.next_key::<String>()? {
            if values.contains_key(&key) {
                return Err(serde::de::Error::custom(format!(
                    "duplicate JSON object key `{key}`"
                )));
            }
            let UniqueJsonValue(value) = object.next_value()?;
            values.insert(key, value);
        }
        Ok(UniqueJsonValue(Value::Object(values)))
    }
}

fn parse_unique_json(bytes: &[u8]) -> Result<Value> {
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let UniqueJsonValue(value) =
        UniqueJsonValue::deserialize(&mut deserializer).map_err(|error| {
            VaultcError::Provider(format!("provider emitted malformed NDJSON: {error}"))
        })?;
    deserializer.end().map_err(|error| {
        VaultcError::Provider(format!("provider emitted malformed NDJSON: {error}"))
    })?;
    Ok(value)
}

fn decode_exact_value<T>(value: &Value, context: &str) -> Result<T>
where
    T: DeserializeOwned + Serialize,
{
    let decoded: T = serde_json::from_value(value.clone())
        .map_err(|error| VaultcError::Provider(format!("{context} schema is invalid: {error}")))?;
    let encoded = serde_json::to_value(&decoded).map_err(|error| {
        VaultcError::Internal(format!("failed to re-encode typed provider data: {error}"))
    })?;
    if encoded != *value {
        return Err(VaultcError::Provider(format!(
            "{context} contains unknown or non-schema fields"
        )));
    }
    Ok(decoded)
}

impl CommandProvider {
    pub fn new(config: CommandProviderConfig) -> Result<Self> {
        if config.timeout.is_zero() || config.timeout > HARD_MAX_TIMEOUT {
            return Err(VaultcError::InvalidConfig(format!(
                "provider timeout must be between 1 ms and {} seconds",
                HARD_MAX_TIMEOUT.as_secs()
            )));
        }
        if config.max_line_bytes == 0 || config.max_line_bytes > HARD_MAX_LINE_BYTES {
            return Err(VaultcError::InvalidConfig(format!(
                "provider line limit must be within 1..={HARD_MAX_LINE_BYTES} bytes"
            )));
        }
        if config.max_output_bytes == 0 || config.max_output_bytes > HARD_MAX_OUTPUT_BYTES {
            return Err(VaultcError::InvalidConfig(format!(
                "provider output limit must be within 1..={HARD_MAX_OUTPUT_BYTES} bytes"
            )));
        }
        if config.max_line_bytes > config.max_output_bytes {
            return Err(VaultcError::InvalidConfig(
                "provider line limit cannot exceed the total output limit".into(),
            ));
        }
        if config.max_messages < 2 || config.max_messages > HARD_MAX_MESSAGES {
            return Err(VaultcError::InvalidConfig(format!(
                "provider message limit must be within 2..={HARD_MAX_MESSAGES}"
            )));
        }
        if config.program.as_os_str().is_empty() {
            return Err(VaultcError::InvalidConfig(
                "provider executable cannot be empty".into(),
            ));
        }
        if let Some(directory) = &config.working_directory {
            let metadata = std::fs::metadata(directory).map_err(|error| VaultcError::Io {
                path: directory.clone(),
                source: error,
            })?;
            if !metadata.is_dir() {
                return Err(VaultcError::InvalidConfig(format!(
                    "provider working directory `{}` is not a directory",
                    directory.display()
                )));
            }
        }
        Ok(Self { config })
    }

    pub fn augment<A, F>(
        &self,
        request: &AugmentationRequest,
        authorize: F,
    ) -> Result<ProviderRun<A>>
    where
        F: FnOnce(&ProviderCapabilities) -> Result<A>,
    {
        let deadline = Instant::now()
            .checked_add(self.config.timeout)
            .ok_or_else(|| VaultcError::InvalidConfig("provider timeout overflow".into()))?;
        let mut session = ChildSession::spawn(&self.config)?;

        let capabilities_request = Envelope::new(
            "capabilities-1",
            MessageType::CapabilitiesRequest,
            Value::Object(serde_json::Map::new()),
        );
        session.send(&capabilities_request, deadline)?;
        let capabilities_wire = session.receive::<ProviderCapabilities>(
            deadline,
            "capabilities-1",
            MessageType::CapabilitiesResponse,
        )?;
        let capabilities = capabilities_wire.payload.clone();
        if !capabilities.structured_output {
            return Err(VaultcError::Provider(
                "knowledge augmentation requires structured provider output".into(),
            ));
        }
        if !capabilities.supports(ProviderOperation::KnowledgeAugmentation) {
            return Err(VaultcError::Provider(
                "provider did not negotiate knowledge augmentation".into(),
            ));
        }
        let authorization = authorize(&capabilities)?;

        let request_payload = serde_json::to_vec(request)?;
        if request_payload.len() as u64 > capabilities.max_input_bytes {
            return Err(VaultcError::ResourceLimit(format!(
                "augmentation projection is {} bytes, exceeding provider input limit {}",
                request_payload.len(),
                capabilities.max_input_bytes
            )));
        }
        let augmentation_request = Envelope::new(
            "augmentation-1",
            MessageType::AugmentationRequest,
            request.clone(),
        );
        session.send(&augmentation_request, deadline)?;
        let response_wire = session.receive::<AugmentationResponse>(
            deadline,
            "augmentation-1",
            MessageType::AugmentationResponse,
        )?;
        let response_bytes = serde_json::to_vec(&response_wire.payload)?;
        if response_bytes.len() as u64 > capabilities.max_output_bytes {
            return Err(VaultcError::ResourceLimit(format!(
                "augmentation response is {} bytes, exceeding declared provider output limit {}",
                response_bytes.len(),
                capabilities.max_output_bytes
            )));
        }
        for proposal in &response_wire.payload.proposals {
            if proposal.provider != capabilities.provider {
                return Err(VaultcError::Provider(
                    "a proposal identity does not match the negotiated provider identity".into(),
                ));
            }
        }

        session.finish(deadline)?;
        Ok(ProviderRun {
            authorization,
            response: response_wire.payload,
        })
    }
}

#[derive(Debug)]
enum ReaderEvent {
    Line(Vec<u8>),
    End,
    Failed(String),
}

struct WriterCommand {
    bytes: Vec<u8>,
    completion: Sender<std::result::Result<(), String>>,
}

#[derive(Debug, Default)]
struct StderrSummary {
    bytes_seen: usize,
    truncated: bool,
}

struct ChildSession {
    child: Child,
    writer: Option<Sender<WriterCommand>>,
    receiver: Receiver<ReaderEvent>,
    cancellation: ProviderCancellation,
    stdin_thread: Option<JoinHandle<()>>,
    stdout_thread: Option<JoinHandle<()>>,
    stderr_thread: Option<JoinHandle<StderrSummary>>,
    reaped: bool,
}

impl ChildSession {
    fn spawn(config: &CommandProviderConfig) -> Result<Self> {
        let mut command = Command::new(&config.program);
        command
            .args(&config.arguments)
            .env_clear()
            .env(
                "VAULTC_PROVIDER_PROTOCOL_VERSION",
                PROTOCOL_VERSION.to_string(),
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        copy_required_environment(&mut command);
        if let Some(directory) = &config.working_directory {
            command.current_dir(directory);
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt as _;
            command.process_group(0);
        }

        let mut child = command.spawn().map_err(|error| {
            VaultcError::Provider(format!(
                "failed to start provider executable `{}`: {error}",
                config.program.display()
            ))
        })?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| VaultcError::Provider("provider stdin was not piped".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| VaultcError::Provider("provider stdout was not piped".into()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| VaultcError::Provider("provider stderr was not piped".into()))?;

        let (writer, writer_receiver) = mpsc::channel();
        let stdin_thread = thread::spawn(move || write_stdin(stdin, &writer_receiver));
        let (sender, receiver) = mpsc::channel();
        let line_limit = config.max_line_bytes;
        let output_limit = config.max_output_bytes;
        let message_limit = config.max_messages;
        let stdout_thread = thread::spawn(move || {
            read_ndjson(stdout, line_limit, output_limit, message_limit, &sender);
        });
        let stderr_thread = thread::spawn(move || drain_stderr(stderr, STDERR_CAPTURE_BYTES));
        Ok(Self {
            child,
            writer: Some(writer),
            receiver,
            cancellation: config.cancellation.clone(),
            stdin_thread: Some(stdin_thread),
            stdout_thread: Some(stdout_thread),
            stderr_thread: Some(stderr_thread),
            reaped: false,
        })
    }

    fn send<T: Serialize>(&mut self, envelope: &Envelope<T>, deadline: Instant) -> Result<()> {
        let mut encoded = serde_json::to_vec(envelope)?;
        if encoded.len() > HARD_MAX_OUTPUT_BYTES {
            return Err(VaultcError::ResourceLimit(format!(
                "provider request envelope exceeds {HARD_MAX_OUTPUT_BYTES} bytes"
            )));
        }
        encoded.push(b'\n');
        let writer = self
            .writer
            .as_ref()
            .ok_or_else(|| VaultcError::Provider("provider stdin is closed".into()))?;
        let (completion, result) = mpsc::channel();
        writer
            .send(WriterCommand {
                bytes: encoded,
                completion,
            })
            .map_err(|_| VaultcError::Provider("provider input writer stopped".into()))?;
        loop {
            if self.cancellation.is_cancelled() {
                self.terminate();
                return Err(VaultcError::Provider(
                    "provider operation was cancelled".into(),
                ));
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                self.terminate();
                return Err(VaultcError::Provider(
                    "provider input deadline exceeded".into(),
                ));
            }
            match result.recv_timeout(remaining.min(PROVIDER_POLL_INTERVAL)) {
                Ok(Ok(())) => return Ok(()),
                Ok(Err(reason)) => return Err(VaultcError::Provider(reason)),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(VaultcError::Provider(
                        "provider input writer stopped unexpectedly".into(),
                    ));
                }
            }
        }
    }

    fn receive<T: DeserializeOwned + Serialize>(
        &mut self,
        deadline: Instant,
        request_id: &str,
        expected_type: MessageType,
    ) -> Result<Envelope<T>> {
        let line = loop {
            if self.cancellation.is_cancelled() {
                self.terminate();
                return Err(VaultcError::Provider(
                    "provider operation was cancelled".into(),
                ));
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                self.terminate();
                return Err(VaultcError::Provider("provider deadline exceeded".into()));
            }
            match self
                .receiver
                .recv_timeout(remaining.min(PROVIDER_POLL_INTERVAL))
            {
                Ok(ReaderEvent::Line(line)) => break line,
                Ok(ReaderEvent::End) => {
                    let status = self.child.try_wait().ok().flatten();
                    return Err(VaultcError::Provider(format!(
                        "provider closed protocol output{}",
                        status_suffix(status)
                    )));
                }
                Ok(ReaderEvent::Failed(reason)) => {
                    return Err(VaultcError::Provider(reason));
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(VaultcError::Provider(
                        "provider protocol reader stopped unexpectedly".into(),
                    ));
                }
            }
        };
        let raw = parse_unique_json(&line)?;
        let wire: Envelope<Value> = decode_exact_value(&raw, "provider response envelope")?;
        if wire.protocol_version != PROTOCOL_VERSION {
            return Err(VaultcError::Provider(format!(
                "provider responded with unsupported protocol version {}",
                wire.protocol_version
            )));
        }
        if wire.request_id != request_id {
            return Err(VaultcError::Provider(
                "provider response request ID does not match the active request".into(),
            ));
        }
        if wire.message_type == MessageType::Error {
            let protocol_error: ProtocolError =
                decode_exact_value(&wire.payload, "provider error payload")?;
            return Err(VaultcError::Provider(format!(
                "provider returned error code `{}`; untrusted error text was withheld",
                safe_protocol_label(&protocol_error.code)
            )));
        }
        if wire.message_type != expected_type {
            return Err(VaultcError::Provider(format!(
                "provider returned {:?}; expected {:?}",
                wire.message_type, expected_type
            )));
        }
        let payload = decode_exact_value(&wire.payload, "provider response payload")?;
        Ok(Envelope {
            protocol_version: wire.protocol_version,
            request_id: wire.request_id,
            message_type: wire.message_type,
            payload,
        })
    }

    fn finish(&mut self, deadline: Instant) -> Result<()> {
        self.writer.take();
        loop {
            if self.cancellation.is_cancelled() {
                self.terminate();
                return Err(VaultcError::Provider(
                    "provider operation was cancelled".into(),
                ));
            }
            match self.child.try_wait() {
                Ok(Some(status)) => {
                    self.reaped = true;
                    let protocol_result = validate_protocol_tail(&self.receiver);
                    if protocol_result.is_err() {
                        // A descendant may still own an inherited stdout pipe
                        // after the direct provider process exits.
                        terminate_process_tree(&mut self.child);
                    }
                    let workers_stopped = self.join_worker_threads();
                    if !status.success() {
                        return Err(VaultcError::Provider(format!(
                            "provider exited unsuccessfully{}",
                            status_suffix(Some(status))
                        )));
                    }
                    if !workers_stopped && protocol_result.is_ok() {
                        return Err(VaultcError::Provider(
                            "provider worker threads did not stop after process exit".into(),
                        ));
                    }
                    return protocol_result;
                }
                Ok(None) if Instant::now() < deadline => {
                    thread::sleep(
                        deadline
                            .saturating_duration_since(Instant::now())
                            .min(PROVIDER_POLL_INTERVAL),
                    );
                }
                Ok(None) => {
                    self.terminate();
                    return Err(VaultcError::Provider(
                        "provider did not exit before its deadline".into(),
                    ));
                }
                Err(error) => {
                    self.terminate();
                    return Err(VaultcError::Provider(format!(
                        "failed to query provider status: {error}"
                    )));
                }
            }
        }
    }

    fn terminate(&mut self) {
        if self.reaped {
            return;
        }
        self.writer.take();
        terminate_process_tree(&mut self.child);
        let _ = self.child.wait();
        self.reaped = true;
        self.join_worker_threads();
    }

    fn join_worker_threads(&mut self) -> bool {
        let deadline = Instant::now()
            .checked_add(THREAD_JOIN_GRACE)
            .unwrap_or_else(Instant::now);
        while Instant::now() < deadline
            && (self
                .stdin_thread
                .as_ref()
                .is_some_and(|thread| !thread.is_finished())
                || self
                    .stdout_thread
                    .as_ref()
                    .is_some_and(|thread| !thread.is_finished())
                || self
                    .stderr_thread
                    .as_ref()
                    .is_some_and(|thread| !thread.is_finished()))
        {
            thread::sleep(Duration::from_millis(10));
        }

        let mut stopped = true;
        if let Some(thread) = self.stdin_thread.take() {
            stopped &= thread.is_finished() && thread.join().is_ok();
        }
        if let Some(thread) = self.stdout_thread.take() {
            stopped &= thread.is_finished() && thread.join().is_ok();
        }
        if let Some(thread) = self.stderr_thread.take() {
            if thread.is_finished() {
                if let Ok(summary) = thread.join() {
                    let _ = (summary.bytes_seen, summary.truncated);
                } else {
                    stopped = false;
                }
            } else {
                stopped = false;
            }
        }
        stopped
    }
}

impl Drop for ChildSession {
    fn drop(&mut self) {
        self.terminate();
    }
}

fn validate_protocol_tail(receiver: &Receiver<ReaderEvent>) -> Result<()> {
    match receiver.recv_timeout(PROTOCOL_CLOSE_GRACE) {
        Ok(ReaderEvent::Line(_)) => Err(VaultcError::Provider(
            "provider emitted an unexpected protocol message after the response".into(),
        )),
        Ok(ReaderEvent::Failed(reason)) => Err(VaultcError::Provider(reason)),
        Ok(ReaderEvent::End) => Ok(()),
        Err(RecvTimeoutError::Timeout) => Err(VaultcError::Provider(
            "provider protocol output did not close after the response".into(),
        )),
        Err(RecvTimeoutError::Disconnected) => Err(VaultcError::Provider(
            "provider protocol reader stopped without a completion marker".into(),
        )),
    }
}

fn write_stdin<W: Write>(mut stdin: W, receiver: &Receiver<WriterCommand>) {
    while let Ok(command) = receiver.recv() {
        let result = stdin
            .write_all(&command.bytes)
            .and_then(|()| stdin.flush())
            .map_err(|error| format!("provider input failed: {error}"));
        let failed = result.is_err();
        let _ = command.completion.send(result);
        if failed {
            break;
        }
    }
}

fn read_ndjson<R: Read>(
    mut reader: R,
    max_line_bytes: usize,
    max_output_bytes: usize,
    max_messages: usize,
    sender: &Sender<ReaderEvent>,
) {
    let mut buffer = [0_u8; 8192];
    let mut line = Vec::new();
    let mut total = 0_usize;
    let mut messages = 0_usize;
    loop {
        let read = match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) => {
                let _ = sender.send(ReaderEvent::Failed(format!(
                    "provider output read failed: {error}"
                )));
                return;
            }
        };
        total = total.saturating_add(read);
        if total > max_output_bytes {
            let _ = sender.send(ReaderEvent::Failed(format!(
                "provider output exceeded {max_output_bytes} bytes"
            )));
            return;
        }
        for byte in &buffer[..read] {
            if *byte == b'\n' {
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
                if line.is_empty() {
                    let _ = sender.send(ReaderEvent::Failed(
                        "provider emitted a blank protocol line".into(),
                    ));
                    return;
                }
                messages += 1;
                if messages > max_messages {
                    let _ = sender.send(ReaderEvent::Failed(format!(
                        "provider emitted more than {max_messages} protocol messages"
                    )));
                    return;
                }
                if sender
                    .send(ReaderEvent::Line(std::mem::take(&mut line)))
                    .is_err()
                {
                    return;
                }
            } else {
                line.push(*byte);
                if line.len() > max_line_bytes {
                    let _ = sender.send(ReaderEvent::Failed(format!(
                        "provider protocol line exceeded {max_line_bytes} bytes"
                    )));
                    return;
                }
            }
        }
    }
    if !line.is_empty() {
        messages += 1;
        if messages > max_messages {
            let _ = sender.send(ReaderEvent::Failed(format!(
                "provider emitted more than {max_messages} protocol messages"
            )));
            return;
        }
        if sender.send(ReaderEvent::Line(line)).is_err() {
            return;
        }
    }
    let _ = sender.send(ReaderEvent::End);
}

fn drain_stderr<R: Read>(mut reader: R, capture_limit: usize) -> StderrSummary {
    let mut buffer = [0_u8; 8192];
    let mut summary = StderrSummary::default();
    loop {
        match reader.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(read) => {
                summary.bytes_seen = summary.bytes_seen.saturating_add(read);
                if summary.bytes_seen > capture_limit {
                    summary.truncated = true;
                }
            }
        }
    }
    summary
}

fn copy_required_environment(command: &mut Command) {
    const SAFE_KEYS: &[&str] = if cfg!(windows) {
        &["PATH", "PATHEXT", "SYSTEMROOT", "WINDIR", "TEMP", "TMP"]
    } else {
        &["PATH", "TMPDIR"]
    };
    for key in SAFE_KEYS {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
}

#[cfg(unix)]
fn terminate_process_tree(child: &mut Child) {
    let process_group = format!("-{}", child.id());
    let _ = Command::new("/bin/kill")
        .args(["-KILL", "--", process_group.as_str()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let _ = child.kill();
}

#[cfg(not(unix))]
fn terminate_process_tree(child: &mut Child) {
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill.exe")
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
}

fn status_suffix(status: Option<ExitStatus>) -> String {
    status.map_or_else(String::new, |status| format!(" with status {status}"))
}

fn safe_protocol_label(value: &str) -> String {
    let sanitized: String = value
        .chars()
        .filter(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
        .take(64)
        .collect();
    if sanitized.is_empty() {
        "untrusted".into()
    } else {
        sanitized
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect(input: &[u8], line: usize, total: usize, count: usize) -> Vec<ReaderEvent> {
        let (sender, receiver) = mpsc::channel();
        read_ndjson(input, line, total, count, &sender);
        drop(sender);
        receiver.into_iter().collect()
    }

    #[test]
    fn reader_accepts_crlf_and_unterminated_last_line() {
        let events = collect(b"{\"a\":1}\r\n{\"b\":2}", 32, 64, 2);
        assert!(matches!(&events[0], ReaderEvent::Line(line) if line == br#"{"a":1}"#));
        assert!(matches!(&events[1], ReaderEvent::Line(line) if line == br#"{"b":2}"#));
        assert!(matches!(&events[2], ReaderEvent::End));
    }

    #[test]
    fn reader_rejects_overlong_line_before_unbounded_growth() {
        let events = collect(b"123456", 5, 64, 2);
        assert!(
            matches!(&events[0], ReaderEvent::Failed(message) if message.contains("line exceeded"))
        );
    }

    #[test]
    fn reader_rejects_total_output_and_message_floods() {
        let bytes = collect(b"{}\n{}\n", 8, 3, 4);
        assert!(
            matches!(&bytes[0], ReaderEvent::Failed(message) if message.contains("output exceeded"))
        );

        let messages = collect(b"{}\n{}\n{}\n", 8, 32, 2);
        assert!(matches!(&messages[0], ReaderEvent::Line(_)));
        assert!(matches!(&messages[1], ReaderEvent::Line(_)));
        assert!(
            matches!(&messages[2], ReaderEvent::Failed(message) if message.contains("protocol messages"))
        );
    }

    #[test]
    fn provider_json_rejects_recursive_duplicates_and_non_schema_fields() {
        assert!(
            parse_unique_json(br#"{"payload":{"proposals":[],"proposals":[]}}"#).is_err(),
            "nested duplicate object keys must fail before Value materialization"
        );

        let unknown_payload = parse_unique_json(br#"{"proposals":[],"unexpected":true}"#)
            .expect("unique JSON with an unknown field");
        assert!(
            decode_exact_value::<AugmentationResponse>(
                &unknown_payload,
                "provider response payload"
            )
            .is_err(),
            "typed payload roundtrip must reject unknown fields"
        );

        let unknown_envelope = parse_unique_json(
            br#"{"message_type":"augmentation_response","payload":{"proposals":[]},"protocol_version":1,"request_id":"augmentation-1","unexpected":true}"#,
        )
        .expect("unique envelope with an unknown field");
        assert!(
            decode_exact_value::<Envelope<Value>>(&unknown_envelope, "provider response envelope")
                .is_err(),
            "typed envelope roundtrip must reject unknown fields"
        );
    }

    #[test]
    fn protocol_tail_requires_only_the_end_marker() {
        let (sender, receiver) = mpsc::channel();
        sender.send(ReaderEvent::End).expect("queue end marker");
        drop(sender);
        assert!(validate_protocol_tail(&receiver).is_ok());

        let (sender, receiver) = mpsc::channel();
        sender
            .send(ReaderEvent::Line(br#"{"unexpected":true}"#.to_vec()))
            .expect("queue unexpected line");
        sender.send(ReaderEvent::End).expect("queue end marker");
        drop(sender);
        assert!(validate_protocol_tail(&receiver).is_err());

        let (sender, receiver) = mpsc::channel();
        sender
            .send(ReaderEvent::Failed("bounded reader failed".into()))
            .expect("queue reader failure");
        drop(sender);
        assert!(validate_protocol_tail(&receiver).is_err());
    }
}
