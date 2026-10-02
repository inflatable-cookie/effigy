use crate::secure_fs::{Authority, HostRunRoot, TrustError};
use crate::token::{ParentToken, TokenError, TokenKeys};
use crate::{canonical_start_identity, MAX_FRAME_BYTES, MAX_OUTPUT_CHUNK_BYTES};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use chrono::{DateTime, Utc};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

const ATTACH_RECONNECT_WINDOW: Duration = Duration::from_secs(5);
const ATTACH_RECONNECT_INITIAL_BACKOFF: Duration = Duration::from_millis(50);
const ATTACH_RECONNECT_MAX_BACKOFF: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunClass {
    Heavy,
    Light,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClassSource {
    Manifest,
    Default,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    Milestone,
    Validation,
    Interactive,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BudgetFallback {
    pub cpu: u32,
    pub memory_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct SubmitRequest {
    pub client_request_id: String,
    pub caller: String,
    pub repository: String,
    pub cwd: std::path::PathBuf,
    pub selector: String,
    pub argv: Vec<String>,
    pub class: RunClass,
    pub class_source: ClassSource,
    pub priority: Priority,
    pub budget_fallback: BudgetFallback,
    pub capacity_deadline_ms: u64,
    pub run_timeout_ms: u64,
    pub env: std::collections::BTreeMap<String, String>,
    pub cancel_on_disconnect: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatusQuery {
    Run {
        run_id: String,
    },
    CallerRequest {
        caller: String,
        client_request_id: String,
    },
    Host,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmitResult {
    pub run_id: String,
    pub state: String,
    pub position: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettlementOutcome {
    Passed,
    Failed,
    TimedOut,
    Cancelled,
    Lost,
    CapacityTimeout,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostRunFact {
    Nested {
        parent_run_id: String,
        selector: String,
    },
    Override {
        reason: String,
        selector: String,
    },
    ContainerStarted {
        run_id: String,
        epoch: u64,
        runtime: String,
        container_id: String,
    },
    ContainerRemoved {
        run_id: String,
        epoch: u64,
        runtime: String,
        container_id: String,
        removed: Option<bool>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Settlement {
    pub run_id: String,
    pub outcome: SettlementOutcome,
    pub launched: bool,
    pub settled_at: String,
    /// `None` means the server explicitly supplied no result (for example lost).
    pub result: Option<Value>,
    pub containers: Vec<Value>,
    pub raw: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AttachEvent {
    State {
        state: String,
        position: Option<u64>,
    },
    Output {
        stream: OutputStream,
        offset: u64,
        data: Vec<u8>,
    },
    OutputExpired {
        stream: OutputStream,
        available_from: u64,
    },
    Settled(Settlement),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WireError {
    pub code: String,
    pub message: String,
}

#[derive(Debug)]
pub enum ClientError {
    Trust(TrustError),
    Io(io::Error),
    Frame(&'static str),
    Decode(serde_json::Error),
    Wire(WireError),
    InvalidSettlement(&'static str),
    InvalidAttach(&'static str),
    InvalidSubmission(&'static str),
    AmbiguousSubmit,
    InvalidParentToken(TokenError),
    SchedulerUnreachable,
    ReportConflict(String),
    ReportUnacked(Vec<String>),
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Trust(e) => e.fmt(f),
            Self::Io(e) => e.fmt(f),
            Self::Frame(s) => write!(f, "invalid host-run frame: {s}"),
            Self::Decode(e) => write!(f, "invalid host-run response: {e}"),
            Self::Wire(e) => write!(f, "scheduler {}: {}", e.code, e.message),
            Self::InvalidSettlement(s) => write!(f, "invalid host-run settlement: {s}"),
            Self::InvalidAttach(s) => write!(f, "invalid attach stream: {s}"),
            Self::InvalidSubmission(s) => write!(f, "invalid submission: {s}"),
            Self::AmbiguousSubmit => {
                f.write_str("scheduler_unreachable: submit outcome remains unknown")
            }
            Self::InvalidParentToken(e) => e.fmt(f),
            Self::SchedulerUnreachable => f.write_str("scheduler_unreachable"),
            Self::ReportConflict(id) => {
                write!(f, "scheduler fact conflict for {id}; fact retained locally")
            }
            Self::ReportUnacked(ids) => write!(
                f,
                "scheduler did not acknowledge facts {ids:?}; they remain pending"
            ),
        }
    }
}
impl std::error::Error for ClientError {}
impl From<TrustError> for ClientError {
    fn from(value: TrustError) -> Self {
        Self::Trust(value)
    }
}
impl From<io::Error> for ClientError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}
impl From<serde_json::Error> for ClientError {
    fn from(value: serde_json::Error) -> Self {
        Self::Decode(value)
    }
}

fn is_retryable_endpoint_error(error: &ClientError) -> bool {
    match error {
        ClientError::Io(error) => matches!(
            error.kind(),
            io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
        ),
        ClientError::Trust(TrustError::Io(error)) => error.kind() == io::ErrorKind::NotFound,
        _ => false,
    }
}

fn is_transport_closure(error: &ClientError) -> bool {
    matches!(
        error,
        ClientError::Io(error)
            if matches!(
                error.kind(),
                io::ErrorKind::UnexpectedEof
                    | io::ErrorKind::ConnectionReset
                    | io::ErrorKind::BrokenPipe
                    | io::ErrorKind::ConnectionAborted
                    | io::ErrorKind::NotConnected
            )
    )
}

fn classify_socket_trust_error(error: TrustError) -> RecoveryError {
    let retry = matches!(&error, TrustError::Io(error) if error.kind() == io::ErrorKind::NotFound);
    let error = ClientError::Trust(error);
    if retry {
        RecoveryError::Retry
    } else {
        RecoveryError::Fail(error)
    }
}

fn classify_recovery_connect_error(error: ClientError) -> RecoveryError {
    if is_retryable_endpoint_error(&error) || is_transport_closure(&error) {
        RecoveryError::Retry
    } else {
        RecoveryError::Fail(error)
    }
}

fn classify_recovery_io(error: ClientError) -> RecoveryError {
    if is_transport_closure(&error) {
        RecoveryError::Retry
    } else {
        RecoveryError::Fail(error)
    }
}

fn parent_token_status_error(error: ClientError) -> TokenError {
    match error {
        ClientError::SchedulerUnreachable
        | ClientError::Io(_)
        | ClientError::Trust(_)
        | ClientError::InvalidParentToken(TokenError::SchedulerUnreachable) => {
            TokenError::SchedulerUnreachable
        }
        ClientError::Wire(WireError { code, .. })
            if matches!(code.as_str(), "unsupported_version" | "stale_epoch") =>
        {
            TokenError::SchedulerUnreachable
        }
        _ => TokenError::Invalid("takeover lookup failed"),
    }
}

fn read_recovery_keys(root: &HostRunRoot, epoch: u64) -> Result<TokenKeys, ClientError> {
    let keys = TokenKeys::from_root(root).map_err(|_| {
        ClientError::Trust(TrustError::Invalid(
            "token keys could not be trusted during run recovery",
        ))
    })?;
    if !keys.matches_authority_epoch(epoch) {
        return Err(ClientError::Trust(TrustError::Invalid(
            "token key epochs do not match the current authority",
        )));
    }
    Ok(keys)
}

impl ClientError {
    /// Protocol-compatible process exit classification for callers.
    pub fn exit_code(&self) -> Option<u8> {
        match self {
            Self::SchedulerUnreachable | Self::AmbiguousSubmit | Self::Io(_) | Self::Trust(_) => {
                Some(75)
            }
            Self::InvalidParentToken(TokenError::Invalid(_)) => Some(77),
            Self::InvalidParentToken(TokenError::SchedulerUnreachable) => Some(75),
            _ => None,
        }
    }
}

pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}
#[derive(Debug, Default)]
pub struct SystemClock;
impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

pub trait IdentityProvider: Send + Sync {
    fn start_identity(&self, pid: u32) -> Option<String>;
}
#[derive(Debug, Default)]
pub struct SystemIdentityProvider;
impl IdentityProvider for SystemIdentityProvider {
    fn start_identity(&self, pid: u32) -> Option<String> {
        canonical_start_identity(pid)
    }
}

pub(crate) trait ReconnectTimer: Send + Sync {
    fn now(&self) -> Duration;
    fn sleep(&self, duration: Duration);
}

struct SystemReconnectTimer {
    start: Instant,
}

impl Default for SystemReconnectTimer {
    fn default() -> Self {
        Self {
            start: Instant::now(),
        }
    }
}

impl ReconnectTimer for SystemReconnectTimer {
    fn now(&self) -> Duration {
        self.start.elapsed()
    }

    fn sleep(&self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

enum RecoveryError {
    Retry,
    Fail(ClientError),
}

pub struct HostRunClient {
    root: HostRunRoot,
    authority: Authority,
    clock: Arc<dyn Clock>,
    identity: Arc<dyn IdentityProvider>,
    reconnect_timer: Arc<dyn ReconnectTimer>,
}

impl HostRunClient {
    pub fn open(root: HostRunRoot, authority: Authority) -> Self {
        Self {
            root,
            authority,
            clock: Arc::new(SystemClock),
            identity: Arc::new(SystemIdentityProvider),
            reconnect_timer: Arc::new(SystemReconnectTimer::default()),
        }
    }

    pub fn with_clock(mut self, clock: Arc<dyn Clock>) -> Self {
        self.clock = clock;
        self
    }
    pub fn with_identity_provider(mut self, identity: Arc<dyn IdentityProvider>) -> Self {
        self.identity = identity;
        self
    }
    #[cfg(test)]
    pub(crate) fn with_reconnect_timer(mut self, timer: Arc<dyn ReconnectTimer>) -> Self {
        self.reconnect_timer = timer;
        self
    }
    pub fn authority(&self) -> &Authority {
        &self.authority
    }

    pub fn status_query(&mut self, query: StatusQuery) -> Result<Value, ClientError> {
        let body = match query {
            StatusQuery::Run { run_id } => json!({"runId":run_id}),
            StatusQuery::CallerRequest {
                caller,
                client_request_id,
            } => json!({"caller":caller,"clientRequestId":client_request_id}),
            StatusQuery::Host => json!({}),
        };
        self.status(&body)
    }

    pub fn submit_request(&mut self, request: &SubmitRequest) -> Result<SubmitResult, ClientError> {
        let body = request_body(request)?;
        let result = self.submit_validated(&body)?;
        SubmitResult::parse(&result)
    }

    pub fn report_typed_facts(&mut self, facts: &[HostRunFact]) -> Result<Vec<Value>, ClientError> {
        let values = facts
            .iter()
            .map(|fact| fact.to_value(self.clock.as_ref()))
            .collect::<Result<Vec<_>, _>>()?;
        self.report_facts(&values)
    }

    /// A present token is always validated; failure never falls back to submit.
    pub fn validate_parent_token(
        &mut self,
        token: Option<&str>,
        cwd: &Path,
    ) -> Result<Option<ParentToken>, ClientError> {
        let Some(token) = token else {
            return Ok(None);
        };
        let keys = TokenKeys::from_root(&self.root).map_err(ClientError::InvalidParentToken)?;
        let authority = self.authority.clone();
        let result = keys.validate(token, cwd, &authority, self.clock.now(), |run_id| {
            self.parent_token_status(run_id, &keys, authority.epoch)
        });
        match result {
            Ok(token) => Ok(Some(token)),
            Err(TokenError::SchedulerUnreachable) => Err(ClientError::SchedulerUnreachable),
            Err(error) => Err(ClientError::InvalidParentToken(error)),
        }
    }

    fn parent_token_status(
        &mut self,
        run_id: &str,
        key_anchor: &TokenKeys,
        expected_epoch: u64,
    ) -> Result<Value, TokenError> {
        let body = json!({"runId":run_id});
        let started_at = self.reconnect_timer.now();
        let mut backoff = ATTACH_RECONNECT_INITIAL_BACKOFF;
        loop {
            match self.exchange_recovery("status", &body, expected_epoch, key_anchor, started_at) {
                Ok(status) => {
                    let status = validate_status(status)
                        .map_err(|_| TokenError::Invalid("takeover lookup failed"))?;
                    if status.get("runId").and_then(Value::as_str) != Some(run_id)
                        || status.get("epoch").and_then(Value::as_u64) != Some(expected_epoch)
                    {
                        return Err(TokenError::Invalid("takeover lookup failed"));
                    }
                    return Ok(status);
                }
                Err(RecoveryError::Retry) => {
                    let elapsed = self.reconnect_timer.now().saturating_sub(started_at);
                    if elapsed >= ATTACH_RECONNECT_WINDOW {
                        return Err(TokenError::SchedulerUnreachable);
                    }
                    self.reconnect_timer
                        .sleep(backoff.min(ATTACH_RECONNECT_WINDOW.saturating_sub(elapsed)));
                    if self.reconnect_timer.now().saturating_sub(started_at)
                        >= ATTACH_RECONNECT_WINDOW
                    {
                        return Err(TokenError::SchedulerUnreachable);
                    }
                    backoff = backoff.saturating_mul(2).min(ATTACH_RECONNECT_MAX_BACKOFF);
                }
                Err(RecoveryError::Fail(error)) => {
                    return Err(parent_token_status_error(error));
                }
            }
        }
    }

    pub fn status(&mut self, body: &Value) -> Result<Value, ClientError> {
        self.flush_pending_facts()?;
        match self.exchange("status", body).and_then(validate_status) {
            Err(ClientError::Wire(WireError { code, .. })) if code == "stale_epoch" => {
                self.refresh()?;
                self.exchange("status", body).and_then(validate_status)
            }
            result => result,
        }
    }

    /// Submit ambiguity always resolves by status before any resubmission.
    pub fn submit(&mut self, body: &Value) -> Result<Value, ClientError> {
        let request: SubmitRequest =
            serde_json::from_value(body.clone()).map_err(ClientError::Decode)?;
        let body = request_body(&request)?;
        self.submit_validated(&body)
    }

    fn submit_validated(&mut self, body: &Value) -> Result<Value, ClientError> {
        self.flush_pending_facts()?;
        let caller = body
            .get("caller")
            .and_then(Value::as_str)
            .ok_or(ClientError::InvalidSubmission("caller is required"))?;
        let request_id = body.get("clientRequestId").and_then(Value::as_str).ok_or(
            ClientError::InvalidSubmission("clientRequestId is required"),
        )?;
        if !is_uuid(request_id) {
            return Err(ClientError::InvalidSubmission(
                "clientRequestId must be a UUID",
            ));
        }
        let lookup_body = json!({"caller":caller,"clientRequestId":request_id});
        match self.exchange("submit", body) {
            Ok(value) => Ok(value),
            Err(ClientError::Wire(WireError { code, .. })) if code == "conflict" => {
                Err(ClientError::Wire(WireError {
                    code,
                    message: "request id conflicts with a different body".into(),
                }))
            }
            Err(ClientError::Wire(WireError { code, .. })) if code == "stale_epoch" => {
                self.refresh()?;
                match self.status(&lookup_body) {
                    Ok(value) if value.get("runId").is_some() => Ok(value),
                    Err(ClientError::Wire(WireError { code, .. })) if code == "unknown_run" => {
                        self.exchange("submit", body)
                    }
                    Err(error) => Err(error),
                    _ => Err(ClientError::AmbiguousSubmit),
                }
            }
            Err(ClientError::Io(_))
            | Err(ClientError::SchedulerUnreachable)
            | Err(ClientError::Frame(_))
            | Err(ClientError::Decode(_)) => match self.status(&lookup_body) {
                Ok(value) if value.get("runId").is_some() => Ok(value),
                Err(ClientError::Wire(WireError { code, .. })) if code == "unknown_run" => {
                    self.exchange("submit", body)
                }
                Err(error) => Err(error),
                _ => Err(ClientError::AmbiguousSubmit),
            },
            Err(error) => Err(error),
        }
    }

    pub fn cancel(&mut self, run_id: &str, reason: &str) -> Result<Value, ClientError> {
        let recovery_epoch = self.authority.epoch;
        let mut recovery_keys = read_recovery_keys(&self.root, recovery_epoch)?;
        self.refresh_attach_authority(recovery_epoch, &recovery_keys)?;
        if let Err(error) = self.flush_pending_facts() {
            if !is_retryable_endpoint_error(&error) && !is_transport_closure(&error) {
                return Err(error);
            }
        }
        self.refresh_attach_authority(recovery_epoch, &recovery_keys)?;
        let body = json!({"runId":run_id,"reason":reason});
        let mut result = self.exchange("cancel", &body);
        if matches!(&result, Err(ClientError::Wire(WireError { code, .. })) if code == "stale_epoch")
        {
            self.refresh()?;
            if self.authority.epoch != recovery_epoch {
                return Err(ClientError::Trust(TrustError::Invalid(
                    "authority epoch changed while cancelling the run",
                )));
            }
            let refreshed_keys = read_recovery_keys(&self.root, recovery_epoch)?;
            if !refreshed_keys.same_keyset(&recovery_keys) {
                return Err(ClientError::Trust(TrustError::Invalid(
                    "token keys changed while cancelling the run",
                )));
            }
            recovery_keys = refreshed_keys;
            result = self.exchange("cancel", &body);
        }
        match result {
            Ok(value) => return Ok(value),
            Err(error) if is_retryable_endpoint_error(&error) || is_transport_closure(&error) => {}
            Err(error) => return Err(error),
        }
        let started_at = self.reconnect_timer.now();
        let mut backoff = ATTACH_RECONNECT_INITIAL_BACKOFF;
        loop {
            let elapsed = self.reconnect_timer.now().saturating_sub(started_at);
            if elapsed >= ATTACH_RECONNECT_WINDOW {
                return Err(ClientError::SchedulerUnreachable);
            }
            self.reconnect_timer
                .sleep(backoff.min(ATTACH_RECONNECT_WINDOW.saturating_sub(elapsed)));
            if self.reconnect_timer.now().saturating_sub(started_at) >= ATTACH_RECONNECT_WINDOW {
                return Err(ClientError::SchedulerUnreachable);
            }

            match self.exchange_recovery(
                "cancel",
                &body,
                recovery_epoch,
                &recovery_keys,
                started_at,
            ) {
                Ok(value) => return Ok(value),
                Err(RecoveryError::Retry) => {
                    backoff = backoff.saturating_mul(2).min(ATTACH_RECONNECT_MAX_BACKOFF);
                    continue;
                }
                Err(RecoveryError::Fail(error)) => return Err(error),
            }
        }
    }

    /// Attach from byte offsets, dropping already-seen prefixes and refusing gaps.
    pub fn attach(
        &mut self,
        run_id: &str,
        from_stdout: u64,
        from_stderr: u64,
    ) -> Result<Vec<AttachEvent>, ClientError> {
        let mut events = Vec::new();
        self.attach_stream(run_id, from_stdout, from_stderr, |event| events.push(event))?;
        Ok(events)
    }

    /// Stream attach events as they arrive and return the validated settlement.
    /// Reconnects resume from the exact byte offsets already delivered.
    pub fn attach_stream<F>(
        &mut self,
        run_id: &str,
        from_stdout: u64,
        from_stderr: u64,
        mut on_event: F,
    ) -> Result<Settlement, ClientError>
    where
        F: FnMut(AttachEvent),
    {
        let mut expected = [from_stdout, from_stderr];
        let expected_epoch = self.authority.epoch;
        let recovery_keys = read_recovery_keys(&self.root, expected_epoch)?;
        let authority_changed = self.refresh_attach_authority(expected_epoch, &recovery_keys)?;
        let mut recovery = if authority_changed {
            Some((self.reconnect_timer.now(), ATTACH_RECONNECT_INITIAL_BACKOFF))
        } else {
            None
        };
        if recovery.is_none() {
            if let Err(error) = self.flush_pending_facts() {
                if is_retryable_endpoint_error(&error) || is_transport_closure(&error) {
                    recovery = Some((self.reconnect_timer.now(), ATTACH_RECONNECT_INITIAL_BACKOFF));
                } else {
                    return Err(error);
                }
            }
        }
        loop {
            let mut stream = if let Some((started_at, backoff)) = recovery {
                let elapsed = self.reconnect_timer.now().saturating_sub(started_at);
                if elapsed >= ATTACH_RECONNECT_WINDOW {
                    return Err(ClientError::SchedulerUnreachable);
                }
                self.reconnect_timer
                    .sleep(backoff.min(ATTACH_RECONNECT_WINDOW.saturating_sub(elapsed)));
                if self.reconnect_timer.now().saturating_sub(started_at) >= ATTACH_RECONNECT_WINDOW
                {
                    return Err(ClientError::SchedulerUnreachable);
                }
                match self.recovery_attach_connection(
                    run_id,
                    expected_epoch,
                    &recovery_keys,
                    started_at,
                ) {
                    Ok(stream) => stream,
                    Err(RecoveryError::Retry) => {
                        let elapsed = self.reconnect_timer.now().saturating_sub(started_at);
                        if elapsed >= ATTACH_RECONNECT_WINDOW {
                            return Err(ClientError::SchedulerUnreachable);
                        }
                        recovery = Some((
                            started_at,
                            backoff.saturating_mul(2).min(ATTACH_RECONNECT_MAX_BACKOFF),
                        ));
                        continue;
                    }
                    Err(RecoveryError::Fail(error)) => return Err(error),
                }
            } else {
                match self.connect() {
                    Ok(stream) => stream,
                    Err(error)
                        if is_retryable_endpoint_error(&error) || is_transport_closure(&error) =>
                    {
                        recovery =
                            Some((self.reconnect_timer.now(), ATTACH_RECONNECT_INITIAL_BACKOFF));
                        continue;
                    }
                    Err(error) => return Err(error),
                }
            };
            let body =
                json!({"runId":run_id,"fromOffset":{"stdout":expected[0],"stderr":expected[1]}});
            let request = request("attach", self.authority.epoch, &body)?;
            let request_id = request["id"].clone();
            if let Err(error) = write_request(&mut stream, &request) {
                if is_transport_closure(&error) {
                    recovery = Some((self.reconnect_timer.now(), ATTACH_RECONNECT_INITIAL_BACKOFF));
                    continue;
                }
                return Err(error);
            }
            let mut reader = BufReader::new(stream);
            loop {
                let frame = match read_frame(&mut reader) {
                    Ok(frame) => frame,
                    Err(error) if is_transport_closure(&error) => {
                        recovery =
                            Some((self.reconnect_timer.now(), ATTACH_RECONNECT_INITIAL_BACKOFF));
                        break;
                    }
                    Err(error) => return Err(error),
                };
                let value: Value = serde_json::from_slice(&frame)?;
                if value.get("v").is_some() {
                    if value.get("id") != Some(&request_id) {
                        return Err(ClientError::Frame("response id does not match request"));
                    }
                    if value.get("ok").and_then(Value::as_bool) == Some(false) {
                        let error = parse_response(value).unwrap_err();
                        return Err(error);
                    }
                    if value.get("ok").and_then(Value::as_bool) == Some(true)
                        && value.get("body").and_then(Value::as_object).is_none()
                    {
                        return Err(ClientError::InvalidAttach(
                            "attach response body is not an event",
                        ));
                    }
                }
                let event = value.get("body").unwrap_or(&value);
                match event.get("event").and_then(Value::as_str) {
                    Some("state") => on_event(AttachEvent::State {
                        state: event
                            .get("state")
                            .and_then(Value::as_str)
                            .ok_or(ClientError::InvalidAttach("state event missing state"))?
                            .to_owned(),
                        position: event.get("position").and_then(Value::as_u64),
                    }),
                    Some("output_expired") => {
                        let index =
                            stream_index(event.get("stream").and_then(Value::as_str).ok_or(
                                ClientError::InvalidAttach("expired event missing stream"),
                            )?)?;
                        let available = event
                            .get("availableFrom")
                            .and_then(Value::as_u64)
                            .ok_or(ClientError::InvalidAttach("expired event missing offset"))?;
                        if available < expected[index] {
                            return Err(ClientError::InvalidAttach(
                                "output_expired moved the offset backwards",
                            ));
                        }
                        expected[index] = available;
                        on_event(AttachEvent::OutputExpired {
                            stream: index_stream(index),
                            available_from: available,
                        });
                    }
                    Some("output") => {
                        let index =
                            stream_index(event.get("stream").and_then(Value::as_str).ok_or(
                                ClientError::InvalidAttach("output event missing stream"),
                            )?)?;
                        let offset = event
                            .get("offset")
                            .and_then(Value::as_u64)
                            .ok_or(ClientError::InvalidAttach("output event missing offset"))?;
                        let encoded = event
                            .get("dataB64")
                            .and_then(Value::as_str)
                            .ok_or(ClientError::InvalidAttach("output event missing data"))?;
                        let data = STANDARD.decode(encoded).map_err(|_| {
                            ClientError::InvalidAttach("output chunk has invalid base64")
                        })?;
                        if data.len() > MAX_OUTPUT_CHUNK_BYTES {
                            return Err(ClientError::InvalidAttach("output chunk exceeds 64 KiB"));
                        }
                        let end = offset
                            .checked_add(data.len() as u64)
                            .ok_or(ClientError::InvalidAttach("output offset overflow"))?;
                        if offset > expected[index] {
                            return Err(ClientError::InvalidAttach("output stream contains a gap"));
                        }
                        if end > expected[index] {
                            let skip = (expected[index] - offset) as usize;
                            let new_offset = expected[index];
                            let retained = data[skip..].to_vec();
                            expected[index] = end;
                            on_event(AttachEvent::Output {
                                stream: index_stream(index),
                                offset: new_offset,
                                data: retained,
                            });
                        }
                    }
                    Some("settled") => {
                        let settlement = parse_settlement(event.get("settlement").ok_or(
                            ClientError::InvalidAttach("settled event missing envelope"),
                        )?)?;
                        if settlement.run_id != run_id {
                            return Err(ClientError::InvalidSettlement(
                                "run id does not match attach request",
                            ));
                        }
                        on_event(AttachEvent::Settled(settlement.clone()));
                        return Ok(settlement);
                    }
                    _ => return Err(ClientError::InvalidAttach("unknown event")),
                }
            }
        }
    }

    fn recovery_attach_connection(
        &mut self,
        run_id: &str,
        expected_epoch: u64,
        key_anchor: &TokenKeys,
        started_at: Duration,
    ) -> Result<UnixStream, RecoveryError> {
        let body = json!({"runId":run_id});
        let status_response =
            self.exchange_recovery("status", &body, expected_epoch, key_anchor, started_at)?;
        let status = validate_status(status_response).map_err(RecoveryError::Fail)?;
        if status.get("runId").and_then(Value::as_str) != Some(run_id)
            || status.get("epoch").and_then(Value::as_u64) != Some(expected_epoch)
        {
            return Err(RecoveryError::Fail(ClientError::InvalidAttach(
                "recovery status did not prove the requested run at the current epoch",
            )));
        }

        let status_authority = self.authority.clone();
        self.refresh_recovery_authority(expected_epoch, key_anchor)?;
        if self.authority.pid != status_authority.pid
            || self.authority.start_identity != status_authority.start_identity
        {
            return Err(RecoveryError::Fail(ClientError::Trust(
                TrustError::Invalid("scheduler peer changed during attach recovery"),
            )));
        }
        let timeout = self.recovery_time_left(started_at)?;
        let stream = self
            .connect_with_timeout(timeout)
            .map_err(classify_recovery_connect_error)?;
        self.recovery_time_left(started_at)?;
        stream
            .set_read_timeout(Some(Duration::from_secs(60)))
            .map_err(|error| RecoveryError::Fail(ClientError::Io(error)))?;
        stream
            .set_write_timeout(Some(Duration::from_secs(60)))
            .map_err(|error| RecoveryError::Fail(ClientError::Io(error)))?;
        Ok(stream)
    }

    fn refresh_attach_authority(
        &mut self,
        expected_epoch: u64,
        key_anchor: &TokenKeys,
    ) -> Result<bool, ClientError> {
        let previous = self.authority.clone();
        let authority = self.root.read_authority()?;
        if authority.epoch != expected_epoch {
            return Err(ClientError::Trust(TrustError::Invalid(
                "authority epoch changed before attach",
            )));
        }
        let keys = read_recovery_keys(&self.root, authority.epoch)?;
        if !keys.same_keyset(key_anchor) {
            return Err(ClientError::Trust(TrustError::Invalid(
                "token keys changed before attach",
            )));
        }
        let changed =
            previous.pid != authority.pid || previous.start_identity != authority.start_identity;
        self.authority = authority;
        Ok(changed)
    }

    fn exchange_recovery(
        &mut self,
        method: &str,
        body: &Value,
        expected_epoch: u64,
        key_anchor: &TokenKeys,
        started_at: Duration,
    ) -> Result<Value, RecoveryError> {
        self.refresh_recovery_authority(expected_epoch, key_anchor)?;
        let timeout = self.recovery_time_left(started_at)?;
        let mut stream = self
            .connect_with_timeout(timeout)
            .map_err(classify_recovery_connect_error)?;
        self.recovery_time_left(started_at)?;
        let request = request(method, self.authority.epoch, body).map_err(RecoveryError::Fail)?;
        let request_id = request["id"].clone();
        write_request(&mut stream, &request).map_err(classify_recovery_io)?;
        let frame = read_frame(&mut BufReader::new(stream)).map_err(classify_recovery_io)?;
        let response: Value = serde_json::from_slice(&frame)
            .map_err(ClientError::Decode)
            .map_err(RecoveryError::Fail)?;
        if response.get("id") != Some(&request_id) {
            return Err(RecoveryError::Fail(ClientError::Frame(
                "response id does not match request",
            )));
        }
        parse_response(response).map_err(RecoveryError::Fail)
    }

    fn refresh_recovery_authority(
        &mut self,
        expected_epoch: u64,
        key_anchor: &TokenKeys,
    ) -> Result<(), RecoveryError> {
        let authority = self
            .root
            .read_authority()
            .map_err(|error| RecoveryError::Fail(ClientError::Trust(error)))?;
        if authority.epoch != expected_epoch {
            return Err(RecoveryError::Fail(ClientError::Trust(
                TrustError::Invalid("authority epoch changed during attach recovery"),
            )));
        }
        self.root
            .verify_socket(&authority)
            .map_err(classify_socket_trust_error)?;
        let keys = TokenKeys::from_root(&self.root).map_err(|_| {
            RecoveryError::Fail(ClientError::Trust(TrustError::Invalid(
                "token keys could not be trusted during attach recovery",
            )))
        })?;
        if !keys.matches_authority_epoch(authority.epoch) || !keys.same_keyset(key_anchor) {
            return Err(RecoveryError::Fail(ClientError::Trust(
                TrustError::Invalid("token keys changed during attach recovery"),
            )));
        }
        self.authority = authority;
        Ok(())
    }

    fn recovery_time_left(&self, started_at: Duration) -> Result<Duration, RecoveryError> {
        let elapsed = self.reconnect_timer.now().saturating_sub(started_at);
        let remaining = ATTACH_RECONNECT_WINDOW.saturating_sub(elapsed);
        if remaining.is_zero() {
            Err(RecoveryError::Fail(ClientError::SchedulerUnreachable))
        } else {
            Ok(remaining)
        }
    }

    /// Append facts before sending, then remove only facts with exact stored-copy acks.
    pub fn report_facts(&mut self, facts: &[Value]) -> Result<Vec<Value>, ClientError> {
        let lock = self.root.open_pending_lock()?;
        lock.lock_exclusive()?;
        let result = (|| {
            let mut file = self.root.open_pending_facts()?;
            let mut pending = read_pending(&mut file)?;
            for fact in facts {
                validate_fact(fact)?;
                let id = fact["factId"].as_str().expect("validated fact id");
                if let Some(existing) = pending.iter().find(|value| value["factId"] == id) {
                    if existing != fact {
                        return Err(ClientError::ReportConflict(id.to_owned()));
                    }
                } else {
                    pending.push(fact.clone());
                }
            }
            write_pending(&self.root, &pending)?;
            let body = json!({"facts":pending});
            let response = match self.exchange("report", &body) {
                Err(ClientError::Wire(WireError { code, .. })) if code == "stale_epoch" => {
                    self.refresh()?;
                    self.exchange("report", &body)?
                }
                result => result?,
            };
            let acks = read_acks(&response)?;
            verify_acks(&pending, &acks)?;
            let expected_ids = pending
                .iter()
                .filter_map(|fact| fact["factId"].as_str().map(str::to_owned))
                .collect::<std::collections::HashSet<_>>();
            let acked_ids = acks
                .iter()
                .map(|(id, _)| id.clone())
                .collect::<std::collections::HashSet<_>>();
            let missing = expected_ids
                .difference(&acked_ids)
                .cloned()
                .collect::<Vec<_>>();
            pending.retain(|fact| {
                !fact["factId"]
                    .as_str()
                    .is_some_and(|id| acked_ids.contains(id))
            });
            write_pending(&self.root, &pending)?;
            if !missing.is_empty() {
                return Err(ClientError::ReportUnacked(missing));
            }
            Ok(acks.into_iter().map(|(_, fact)| fact).collect())
        })();
        let unlock_result = FileExt::unlock(&lock);
        if let Err(error) = unlock_result {
            return Err(ClientError::Io(error));
        }
        result
    }

    /// Replay durable facts retained from an earlier unreachable call.
    pub fn flush_pending_facts(&mut self) -> Result<Vec<Value>, ClientError> {
        let lock = self.root.open_pending_lock()?;
        lock.lock_exclusive()?;
        let result = (|| {
            let mut file = self.root.open_pending_facts()?;
            let mut pending = read_pending(&mut file)?;
            if pending.is_empty() {
                return Ok(Vec::new());
            }
            let body = json!({"facts":pending});
            let response = match self.exchange("report", &body) {
                Err(ClientError::Wire(WireError { code, .. })) if code == "stale_epoch" => {
                    self.refresh()?;
                    self.exchange("report", &body)?
                }
                result => result?,
            };
            let acks = read_acks(&response)?;
            verify_acks(&pending, &acks)?;
            let expected_ids = pending
                .iter()
                .filter_map(|fact| fact["factId"].as_str().map(str::to_owned))
                .collect::<std::collections::HashSet<_>>();
            let acked_ids = acks
                .iter()
                .map(|(id, _)| id.clone())
                .collect::<std::collections::HashSet<_>>();
            let missing = expected_ids
                .difference(&acked_ids)
                .cloned()
                .collect::<Vec<_>>();
            pending.retain(|fact| {
                !fact["factId"]
                    .as_str()
                    .is_some_and(|id| acked_ids.contains(id))
            });
            write_pending(&self.root, &pending)?;
            if !missing.is_empty() {
                return Err(ClientError::ReportUnacked(missing));
            }
            Ok(acks.into_iter().map(|(_, fact)| fact).collect())
        })();
        let unlock_result = FileExt::unlock(&lock);
        if let Err(error) = unlock_result {
            return Err(ClientError::Io(error));
        }
        result
    }

    fn exchange(&mut self, method: &str, body: &Value) -> Result<Value, ClientError> {
        let mut stream = self.connect()?;
        let request = request(method, self.authority.epoch, body)?;
        let request_id = request["id"].clone();
        write_request(&mut stream, &request)?;
        let frame = read_frame(&mut BufReader::new(stream))?;
        let response: Value = serde_json::from_slice(&frame)?;
        if response.get("id") != Some(&request_id) {
            return Err(ClientError::Frame("response id does not match request"));
        }
        parse_response(response)
    }

    fn refresh(&mut self) -> Result<(), ClientError> {
        let authority = self.root.read_authority()?;
        self.root.verify_socket(&authority)?;
        self.authority = authority;
        Ok(())
    }

    fn connect(&self) -> Result<UnixStream, ClientError> {
        self.connect_with_timeout(Duration::from_secs(60))
    }

    fn connect_with_timeout(&self, timeout: Duration) -> Result<UnixStream, ClientError> {
        self.root.verify_socket(&self.authority)?;
        let stream = UnixStream::connect(&self.authority.endpoint)?;
        stream.set_read_timeout(Some(timeout))?;
        stream.set_write_timeout(Some(timeout))?;
        verify_peer(
            stream.as_raw_fd(),
            self.root.uid(),
            &self.authority,
            self.identity.as_ref(),
        )?;
        Ok(stream)
    }
}

impl SubmitResult {
    pub fn parse(value: &Value) -> Result<Self, ClientError> {
        let run_id = value
            .get("runId")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .ok_or(ClientError::InvalidSubmission(
                "submit response missing runId",
            ))?;
        let state =
            value
                .get("state")
                .and_then(Value::as_str)
                .ok_or(ClientError::InvalidSubmission(
                    "submit response missing state",
                ))?;
        if !matches!(state, "queued" | "admitted") {
            return Err(ClientError::InvalidSubmission(
                "submit response has invalid state",
            ));
        }
        let position = value.get("position").and_then(Value::as_u64);
        Ok(Self {
            run_id: run_id.to_owned(),
            state: state.to_owned(),
            position,
        })
    }
}

impl HostRunFact {
    fn to_value(&self, clock: &dyn Clock) -> Result<Value, ClientError> {
        match self {
            Self::Nested {
                parent_run_id,
                selector,
            } => nested_fact(parent_run_id, selector, clock),
            Self::Override { reason, selector } => override_fact(reason, selector, clock),
            Self::ContainerStarted {
                run_id,
                epoch,
                runtime,
                container_id,
            } => container_started_fact(run_id, *epoch, runtime, container_id, clock),
            Self::ContainerRemoved {
                run_id,
                epoch,
                runtime,
                container_id,
                removed,
            } => container_removed_fact(run_id, *epoch, runtime, container_id, *removed, clock),
        }
    }
}

fn request_body(request: &SubmitRequest) -> Result<Value, ClientError> {
    if !is_uuid(&request.client_request_id)
        || request.caller.is_empty()
        || request.repository.is_empty()
        || request.selector.is_empty()
        || request.argv.is_empty()
        || request.capacity_deadline_ms == 0
        || request.run_timeout_ms == 0
        || !request.cwd.is_absolute()
    {
        return Err(ClientError::InvalidSubmission(
            "required submit fields are empty or invalid",
        ));
    }
    if request.env.contains_key("HOST_RUN_TOKEN") || request.env.contains_key("HOST_RUN_ID") {
        return Err(ClientError::InvalidSubmission(
            "scheduler-owned host-run environment keys cannot be supplied",
        ));
    }
    if request
        .env
        .keys()
        .any(|key| key.is_empty() || key.contains('=') || key.contains('\0'))
    {
        return Err(ClientError::InvalidSubmission(
            "environment contains an invalid name",
        ));
    }
    let mut body = serde_json::to_value(request).map_err(ClientError::Decode)?;
    let cwd = std::fs::canonicalize(&request.cwd).map_err(ClientError::Io)?;
    if !cwd.is_dir() {
        return Err(ClientError::InvalidSubmission("cwd is not a directory"));
    }
    body["cwd"] = Value::String(cwd.to_string_lossy().into_owned());
    Ok(body)
}

fn verify_peer(
    fd: RawFd,
    uid: u32,
    authority: &Authority,
    identity: &dyn IdentityProvider,
) -> Result<(), ClientError> {
    #[cfg(target_os = "linux")]
    {
        let mut credentials = std::mem::MaybeUninit::<libc::ucred>::zeroed();
        let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
        let result = unsafe {
            libc::getsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_PEERCRED,
                credentials.as_mut_ptr().cast(),
                &mut len,
            )
        };
        if result != 0 {
            return Err(ClientError::Io(io::Error::last_os_error()));
        }
        let credentials = unsafe { credentials.assume_init() };
        if len as usize != std::mem::size_of::<libc::ucred>()
            || credentials.uid != uid
            || credentials.pid <= 0
            || credentials.pid as u32 != authority.pid
        {
            return Err(ClientError::SchedulerUnreachable);
        }
    }
    #[cfg(target_os = "macos")]
    {
        let mut peer_pid: libc::pid_t = 0;
        let mut len = std::mem::size_of::<libc::pid_t>() as libc::socklen_t;
        let result = unsafe {
            libc::getsockopt(
                fd,
                libc::SOL_LOCAL,
                libc::LOCAL_PEERPID,
                (&mut peer_pid as *mut libc::pid_t).cast(),
                &mut len,
            )
        };
        if result != 0 {
            return Err(ClientError::Io(io::Error::last_os_error()));
        }
        let mut peer_uid: libc::uid_t = 0;
        let mut peer_gid: libc::gid_t = 0;
        let result = unsafe { libc::getpeereid(fd, &mut peer_uid, &mut peer_gid) };
        if result != 0 {
            return Err(ClientError::Io(io::Error::last_os_error()));
        }
        let _ = peer_gid;
        if len as usize != std::mem::size_of::<libc::pid_t>()
            || peer_uid != uid
            || peer_pid <= 0
            || peer_pid as u32 != authority.pid
        {
            return Err(ClientError::SchedulerUnreachable);
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = (fd, uid, authority, identity);
        return Err(ClientError::Trust(TrustError::Unsupported));
    }
    if !identity
        .start_identity(authority.pid)
        .is_some_and(|actual| actual == authority.start_identity)
    {
        return Err(ClientError::SchedulerUnreachable);
    }
    Ok(())
}

fn request(method: &str, epoch: u64, body: &Value) -> Result<Value, ClientError> {
    Ok(json!({"v":1,"id":request_id()? ,"method":method,"epoch":epoch,"body":body}))
}

fn request_id() -> Result<String, ClientError> {
    fact_id()
}

fn write_request(writer: &mut impl Write, value: &Value) -> Result<(), ClientError> {
    let mut bytes = serde_json::to_vec(value)?;
    bytes.push(b'\n');
    if bytes.len() > MAX_FRAME_BYTES {
        return Err(ClientError::Frame("request exceeds 1 MiB"));
    }
    writer.write_all(&bytes)?;
    writer.flush()?;
    Ok(())
}

fn read_frame(reader: &mut impl BufRead) -> Result<Vec<u8>, ClientError> {
    let mut frame = Vec::new();
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return Err(ClientError::Io(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "scheduler closed before newline",
            )));
        }
        let take = available
            .iter()
            .position(|byte| *byte == b'\n')
            .map(|i| i + 1)
            .unwrap_or(available.len());
        if frame.len() + take > MAX_FRAME_BYTES {
            return Err(ClientError::Frame("response exceeds 1 MiB"));
        }
        let done = available.get(take - 1) == Some(&b'\n');
        frame.extend_from_slice(&available[..take]);
        reader.consume(take);
        if done {
            frame.pop();
            return Ok(frame);
        }
    }
}

fn parse_response(response: Value) -> Result<Value, ClientError> {
    if response.get("v").and_then(Value::as_u64) != Some(1) {
        return Err(ClientError::SchedulerUnreachable);
    }
    if response.get("ok").and_then(Value::as_bool) == Some(true) {
        return response
            .get("body")
            .cloned()
            .ok_or(ClientError::InvalidAttach("response has no body"));
    }
    if response.get("ok").and_then(Value::as_bool) == Some(false) {
        let error = response
            .get("error")
            .ok_or(ClientError::SchedulerUnreachable)?;
        let wire = WireError {
            code: error
                .get("code")
                .and_then(Value::as_str)
                .ok_or(ClientError::SchedulerUnreachable)?
                .to_owned(),
            message: error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("scheduler rejected request")
                .to_owned(),
        };
        if wire.code == "unsupported_version" {
            return Err(ClientError::SchedulerUnreachable);
        }
        return Err(ClientError::Wire(wire));
    }
    Err(ClientError::SchedulerUnreachable)
}

fn stream_index(value: &str) -> Result<usize, ClientError> {
    match value {
        "stdout" => Ok(0),
        "stderr" => Ok(1),
        _ => Err(ClientError::InvalidAttach("unknown output stream")),
    }
}
fn index_stream(index: usize) -> OutputStream {
    if index == 0 {
        OutputStream::Stdout
    } else {
        OutputStream::Stderr
    }
}

pub fn parse_settlement(value: &Value) -> Result<Settlement, ClientError> {
    if value.get("format").and_then(Value::as_str) != Some("host.run.settlement")
        || value.get("version").and_then(Value::as_u64) != Some(1)
    {
        return Err(ClientError::InvalidSettlement(
            "unsupported format or version",
        ));
    }
    let run_id = value
        .get("runId")
        .and_then(Value::as_str)
        .ok_or(ClientError::InvalidSettlement("missing runId"))?
        .to_owned();
    let outcome = match value.get("outcome").and_then(Value::as_str) {
        Some("passed") => SettlementOutcome::Passed,
        Some("failed") => SettlementOutcome::Failed,
        Some("timed_out") => SettlementOutcome::TimedOut,
        Some("cancelled") => SettlementOutcome::Cancelled,
        Some("lost") => SettlementOutcome::Lost,
        Some("capacity_timeout") => SettlementOutcome::CapacityTimeout,
        _ => return Err(ClientError::InvalidSettlement("unknown outcome")),
    };
    let launched = value
        .get("launched")
        .and_then(Value::as_bool)
        .ok_or(ClientError::InvalidSettlement("missing launched"))?;
    let settled_at = value
        .get("settledAt")
        .and_then(Value::as_str)
        .ok_or(ClientError::InvalidSettlement("missing settledAt"))?
        .to_owned();
    if chrono::DateTime::parse_from_rfc3339(&settled_at).is_err() {
        return Err(ClientError::InvalidSettlement("settledAt is not RFC3339"));
    }
    let result = match value.get("result") {
        None | Some(Value::Null) => None,
        Some(other) => Some(other.clone()),
    };
    let containers = value
        .get("containers")
        .and_then(Value::as_array)
        .ok_or(ClientError::InvalidSettlement("missing containers"))?
        .clone();
    for container in &containers {
        let valid_removal = match container.get("removed") {
            Some(Value::Bool(_)) => true,
            Some(Value::String(value)) => value == "unknown",
            _ => false,
        };
        if container.get("runtime").and_then(Value::as_str).is_none()
            || container.get("id").and_then(Value::as_str).is_none()
            || !valid_removal
        {
            return Err(ClientError::InvalidSettlement(
                "container closure entry is malformed",
            ));
        }
    }
    if !launched {
        if !matches!(
            outcome,
            SettlementOutcome::Cancelled | SettlementOutcome::CapacityTimeout
        ) || result.is_some()
            || value.get("pgid").is_some_and(|v| !v.is_null())
            || value.get("startIdentity").is_some_and(|v| !v.is_null())
        {
            return Err(ClientError::InvalidSettlement(
                "unlaunched settlement has an impossible outcome or process result",
            ));
        }
    } else if outcome == SettlementOutcome::Lost {
        if result.is_some() {
            return Err(ClientError::InvalidSettlement(
                "lost settlement must not invent a result",
            ));
        }
    } else {
        let result = result.as_ref().ok_or(ClientError::InvalidSettlement(
            "launched settlement is missing its result",
        ))?;
        if result.get("format").and_then(Value::as_str) != Some("host.run.result")
            || result.get("version").and_then(Value::as_u64) != Some(1)
            || result.get("runId").and_then(Value::as_str) != Some(run_id.as_str())
        {
            return Err(ClientError::InvalidSettlement(
                "result envelope identity is invalid",
            ));
        }
        let has_exit = result.get("exitCode").is_some_and(|v| !v.is_null());
        let has_signal = result.get("signal").is_some_and(|v| !v.is_null());
        if has_exit == has_signal {
            return Err(ClientError::InvalidSettlement(
                "result must carry exactly one of exitCode and signal",
            ));
        }
        for field in [
            "epoch",
            "pgid",
            "startIdentity",
            "startedAt",
            "endedAt",
            "wallMs",
            "cpuMs",
            "escapedDescendants",
        ] {
            if result.get(field).is_none() {
                return Err(ClientError::InvalidSettlement(
                    "result is missing required evidence",
                ));
            }
        }
        if result.get("epoch").and_then(Value::as_u64).unwrap_or(0) == 0
            || result.get("pgid").and_then(Value::as_u64).unwrap_or(0) == 0
            || result
                .get("startIdentity")
                .and_then(Value::as_str)
                .is_none_or(str::is_empty)
            || !valid_time_value(result.get("startedAt"))
            || !valid_time_value(result.get("endedAt"))
            || !valid_telemetry_value(result.get("wallMs"))
            || !valid_telemetry_value(result.get("cpuMs"))
            || !result
                .get("escapedDescendants")
                .is_some_and(Value::is_array)
            || has_exit && result.get("exitCode").and_then(Value::as_i64).is_none()
            || has_signal && !valid_signal_value(result.get("signal"))
        {
            return Err(ClientError::InvalidSettlement(
                "result evidence has invalid field types",
            ));
        }
    }
    Ok(Settlement {
        run_id,
        outcome,
        launched,
        settled_at,
        result,
        containers,
        raw: value.clone(),
    })
}

fn valid_time_value(value: Option<&Value>) -> bool {
    value
        .and_then(Value::as_str)
        .is_some_and(|text| chrono::DateTime::parse_from_rfc3339(text).is_ok())
}

fn valid_telemetry_value(value: Option<&Value>) -> bool {
    matches!(value, Some(Value::Null)) || value.is_some_and(Value::is_number)
}

fn valid_signal_value(value: Option<&Value>) -> bool {
    value.is_some_and(|value| {
        value.as_u64().is_some_and(|number| number > 0)
            || value.as_str().is_some_and(|name| !name.is_empty())
    })
}

fn validate_fact(fact: &Value) -> Result<(), ClientError> {
    if !fact.is_object()
        || fact
            .get("factId")
            .and_then(Value::as_str)
            .is_none_or(|id| !is_uuid(id))
        || fact.get("observedAt").and_then(Value::as_str).is_none()
    {
        return Err(ClientError::InvalidSubmission(
            "fact requires factId and observedAt",
        ));
    }
    match fact.get("kind").and_then(Value::as_str) {
        Some("nested")
            if has_only_keys(
                fact,
                &["kind", "factId", "observedAt", "parentRunId", "selector"],
            ) && fact.get("parentRunId").and_then(Value::as_str).is_some()
                && fact.get("selector").and_then(Value::as_str).is_some() =>
        {
            Ok(())
        }
        Some("override")
            if has_only_keys(
                fact,
                &["kind", "factId", "observedAt", "reason", "selector"],
            ) && fact.get("reason").and_then(Value::as_str).is_some()
                && fact.get("selector").and_then(Value::as_str).is_some() =>
        {
            Ok(())
        }
        Some("container") => {
            let event = fact.get("event").and_then(Value::as_str);
            let valid_removal = match fact.get("removed") {
                Some(Value::Bool(_)) => true,
                Some(Value::String(value)) => value == "unknown",
                _ => false,
            };
            if has_only_keys(
                fact,
                &[
                    "kind",
                    "factId",
                    "observedAt",
                    "runId",
                    "epoch",
                    "runtime",
                    "containerId",
                    "event",
                    "removed",
                ],
            ) && fact.get("runId").and_then(Value::as_str).is_some()
                && fact.get("epoch").and_then(Value::as_u64).is_some()
                && fact.get("runtime").and_then(Value::as_str).is_some()
                && fact.get("containerId").and_then(Value::as_str).is_some()
                && ((event == Some("started") && fact.get("removed").is_none())
                    || event == Some("removed") && valid_removal)
            {
                Ok(())
            } else {
                Err(ClientError::InvalidSubmission(
                    "fact has an invalid protocol shape",
                ))
            }
        }
        _ => Err(ClientError::InvalidSubmission(
            "fact has an invalid protocol shape",
        )),
    }
}

fn has_only_keys(value: &Value, allowed: &[&str]) -> bool {
    value
        .as_object()
        .is_some_and(|object| object.keys().all(|key| allowed.contains(&key.as_str())))
}

fn validate_status(value: Value) -> Result<Value, ClientError> {
    if let Some(settlement) = value
        .get("settlement")
        .filter(|settlement| !settlement.is_null())
    {
        parse_settlement(settlement)?;
    }
    if value.get("state").and_then(Value::as_str) == Some("settled")
        && value.get("settlement").is_none()
    {
        return Err(ClientError::InvalidSettlement(
            "settled status is missing its envelope",
        ));
    }
    Ok(value)
}

/// Append facts to the durable pending journal without contacting the
/// scheduler. The next client call replays them until acknowledged. A known
/// `factId` with a different body is a conflict; an identical copy is a no-op.
pub fn journal_facts_offline(root: &HostRunRoot, facts: &[Value]) -> Result<(), ClientError> {
    let lock = root.open_pending_lock()?;
    lock.lock_exclusive()?;
    let result = (|| {
        let mut file = root.open_pending_facts()?;
        let mut pending = read_pending(&mut file)?;
        for fact in facts {
            validate_fact(fact)?;
            let id = fact["factId"].as_str().expect("validated fact id");
            match pending.iter().find(|value| value["factId"] == id) {
                Some(existing) if existing != fact => {
                    return Err(ClientError::ReportConflict(id.to_owned()))
                }
                Some(_) => {}
                None => pending.push(fact.clone()),
            }
        }
        write_pending(root, &pending)
    })();
    let unlock = FileExt::unlock(&lock);
    result?;
    unlock?;
    Ok(())
}

fn read_pending(file: &mut File) -> Result<Vec<Value>, ClientError> {
    file.seek(SeekFrom::Start(0))?;
    let mut bytes = Vec::new();
    file.take(8 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err(ClientError::Frame("pending-facts file exceeds size limit"));
    }
    let mut values = Vec::new();
    for line in bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
    {
        let value: Value = serde_json::from_slice(line)?;
        validate_fact(&value)?;
        values.push(value);
    }
    Ok(values)
}

fn write_pending(root: &HostRunRoot, pending: &[Value]) -> Result<(), ClientError> {
    let mut bytes = Vec::new();
    for fact in pending {
        serde_json::to_writer(&mut bytes, fact)?;
        bytes.push(b'\n');
    }
    if bytes.len() > 8 * 1024 * 1024 {
        return Err(ClientError::Frame(
            "pending-facts journal exceeds size limit",
        ));
    }
    root.replace_pending_facts(&bytes)?;
    Ok(())
}

fn read_acks(body: &Value) -> Result<Vec<(String, Value)>, ClientError> {
    let values = body
        .get("acks")
        .or_else(|| body.get("facts"))
        .and_then(Value::as_array)
        .ok_or(ClientError::InvalidAttach(
            "report response has no stored-copy acknowledgements",
        ))?;
    let mut acks = Vec::new();
    for value in values {
        let stored = value.get("stored").unwrap_or(value);
        let id = value
            .get("factId")
            .and_then(Value::as_str)
            .or_else(|| stored.get("factId").and_then(Value::as_str))
            .ok_or(ClientError::InvalidAttach(
                "fact acknowledgement has no factId",
            ))?;
        acks.push((id.to_owned(), stored.clone()));
    }
    Ok(acks)
}

fn verify_acks(pending: &[Value], acks: &[(String, Value)]) -> Result<(), ClientError> {
    for (id, stored) in acks {
        let sent =
            pending
                .iter()
                .find(|fact| fact["factId"] == *id)
                .ok_or(ClientError::InvalidAttach(
                    "report acknowledged an unsent fact",
                ))?;
        if sent != stored {
            return Err(ClientError::ReportConflict(id.clone()));
        }
    }
    Ok(())
}

/// Build a protocol fact with a random UUID-like client identity and UTC time.
pub fn new_fact(kind: &str, fields: Value, clock: &dyn Clock) -> Result<Value, ClientError> {
    let mut value = fields
        .as_object()
        .cloned()
        .ok_or(ClientError::InvalidSubmission(
            "fact fields must be a JSON object",
        ))?;
    value.insert("kind".into(), Value::String(kind.to_owned()));
    value.insert("factId".into(), Value::String(fact_id()?));
    value.insert(
        "observedAt".into(),
        Value::String(
            clock
                .now()
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        ),
    );
    let fact = Value::Object(value);
    validate_fact(&fact)?;
    Ok(fact)
}

pub fn new_client_request_id() -> Result<String, ClientError> {
    fact_id()
}

pub fn nested_fact(
    parent_run_id: &str,
    selector: &str,
    clock: &dyn Clock,
) -> Result<Value, ClientError> {
    new_fact(
        "nested",
        json!({"parentRunId":parent_run_id,"selector":selector}),
        clock,
    )
}

pub fn override_fact(
    reason: &str,
    selector: &str,
    clock: &dyn Clock,
) -> Result<Value, ClientError> {
    new_fact(
        "override",
        json!({"reason":reason,"selector":selector}),
        clock,
    )
}

pub fn container_started_fact(
    run_id: &str,
    epoch: u64,
    runtime: &str,
    container_id: &str,
    clock: &dyn Clock,
) -> Result<Value, ClientError> {
    new_fact(
        "container",
        json!({"runId":run_id,"epoch":epoch,"runtime":runtime,"containerId":container_id,"event":"started"}),
        clock,
    )
}

pub fn container_removed_fact(
    run_id: &str,
    epoch: u64,
    runtime: &str,
    container_id: &str,
    removed: Option<bool>,
    clock: &dyn Clock,
) -> Result<Value, ClientError> {
    let removal = removed
        .map(Value::Bool)
        .unwrap_or_else(|| Value::String("unknown".into()));
    new_fact(
        "container",
        json!({"runId":run_id,"epoch":epoch,"runtime":runtime,"containerId":container_id,"event":"removed","removed":removal}),
        clock,
    )
}

fn fact_id() -> Result<String, ClientError> {
    let mut bytes = [0u8; 16];
    File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    let hex = bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    Ok(format!(
        "{}-{}-4{}-a{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[13..16],
        &hex[17..20],
        &hex[20..32]
    ))
}

fn is_uuid(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 36
        && [8, 13, 18, 23].iter().all(|index| bytes[*index] == b'-')
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| [8, 13, 18, 23].contains(&index) || byte.is_ascii_hexdigit())
        && matches!(bytes[14], b'1'..=b'8')
        && matches!(bytes[19].to_ascii_lowercase(), b'8' | b'9' | b'a' | b'b')
}

#[cfg(test)]
pub(crate) fn parse_test_frame(reader: &mut impl BufRead) -> Result<Vec<u8>, ClientError> {
    read_frame(reader)
}
