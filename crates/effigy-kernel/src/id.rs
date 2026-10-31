//! Encoding v1. Identity inputs are resolved coordinates, never CLI grammar.
use crate::ctx::{EffectClass, Placement};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fmt;
use std::os::unix::ffi::OsStrExt;
use unicode_normalization::UnicodeNormalization;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityError {
    Empty,
    InvalidPath,
    TooLong,
    InvalidEndpoint,
    Collision {
        existing: Vec<u8>,
        incoming: Vec<u8>,
    },
    InvalidInvocation,
}
impl fmt::Display for IdentityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("a kernel identity was empty"),
            Self::InvalidPath => f.write_str("a path is not a valid kernel identity"),
            Self::TooLong => f.write_str("a kernel identity is longer than the kernel allows"),
            Self::InvalidEndpoint => f.write_str("an endpoint is not a valid kernel identity"),
            Self::Collision { existing, incoming } => write!(
                f,
                "two different objects claim the same name ({} vs {})",
                hex_prefix(existing),
                hex_prefix(incoming)
            ),
            Self::InvalidInvocation => f.write_str("an invocation is not a valid kernel identity"),
        }
    }
}

/// The first bytes of an identity digest, enough to tell two apart.
fn hex_prefix(bytes: &[u8]) -> String {
    bytes
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
impl std::error::Error for IdentityError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum CheckoutKind {
    Primary = 0,
    LinkedWorktree = 1,
    EphemeralClone = 2,
    Machine = 3,
    /// Memory-only filesystem gate identity, never a persisted checkout token.
    Filesystem = 4,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckoutId {
    kind: CheckoutKind,
    token: [u8; 16],
}
impl CheckoutId {
    /// Consumes an existing token; token persistence/minting belongs to Prepare.
    pub fn new(kind: CheckoutKind, token: [u8; 16]) -> Self {
        Self { kind, token }
    }
    pub fn token(&self) -> &[u8; 16] {
        &self.token
    }
    pub fn kind(&self) -> CheckoutKind {
        self.kind
    }
    /// Stable text form for evidence bodies: the kind tag and the token in
    /// hex. Never parsed back as authority.
    pub fn text(&self) -> String {
        format!("{}:{}", self.kind as u8, hex(&self.token))
    }
    pub fn resource(&self) -> ResourceId {
        ResourceId::new(
            ResourceKind::Checkout,
            Scope::Machine,
            vec![Some(if self.kind == CheckoutKind::Filesystem {
                format!("filesystem:{}", hex(&self.token))
            } else {
                hex(&self.token)
            })],
        )
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Generation(u64);
impl Generation {
    pub fn new(value: u64) -> Self {
        Self(value)
    }
    pub fn get(self) -> u64 {
        self.0
    }
    pub fn next(self) -> Option<Self> {
        self.0.checked_add(1).map(Self)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct RunId([u8; 16]);
impl RunId {
    /// Caller supplies cryptographic entropy. Time is only an ordering hint.
    /// A journal must still reject reuse; this value grants no authority.
    pub fn new(milliseconds: u64, random: [u8; 10]) -> Result<Self, IdentityError> {
        if milliseconds >= 1 << 48 {
            return Err(IdentityError::TooLong);
        }
        let mut bytes = [0; 16];
        bytes[..6].copy_from_slice(&milliseconds.to_be_bytes()[2..]);
        bytes[6..].copy_from_slice(&random);
        Ok(Self(bytes))
    }
    pub fn bytes(&self) -> &[u8; 16] {
        &self.0
    }
    pub fn resource(&self) -> ResourceId {
        ResourceId::new(
            ResourceKind::Run,
            Scope::Machine,
            vec![Some(self.to_string())],
        )
    }
}
impl fmt::Display for RunId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&base32(&self.0))
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Surface {
    Published,
    Draft,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskId {
    checkout: CheckoutId,
    catalog: String,
    surface: Surface,
    task: String,
    profile: Option<String>,
}
impl TaskId {
    /// Selector resolution belongs to the engine. Catalog coordinates are
    /// normalized here, so equivalent paths and Unicode have one identity.
    pub fn new(
        checkout: CheckoutId,
        catalog: &str,
        surface: Surface,
        task: &str,
        profile: Option<&str>,
    ) -> Result<Self, IdentityError> {
        Ok(Self {
            checkout,
            catalog: canonical_path(catalog)?,
            surface,
            task: text(task)?,
            profile: profile.map(text).transpose()?,
        })
    }
    pub fn checkout(&self) -> CheckoutId {
        self.checkout
    }
    /// Root-relative, '/'-separated; "" for the root catalog.
    pub fn catalog(&self) -> &str {
        &self.catalog
    }
    pub fn surface(&self) -> Surface {
        self.surface
    }
    pub fn task(&self) -> &str {
        &self.task
    }
    pub fn profile(&self) -> Option<&str> {
        self.profile.as_deref()
    }
    pub fn resource(&self) -> ResourceId {
        ResourceId::new(
            ResourceKind::Task,
            Scope::Checkout(self.checkout),
            vec![
                Some(self.catalog.clone()),
                Some(
                    match self.surface {
                        Surface::Published => "published",
                        Surface::Draft => "draft",
                    }
                    .into(),
                ),
                Some(self.task.clone()),
                self.profile.clone(),
            ],
        )
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum ResourceKind {
    Checkout = 0,
    Task = 1,
    Run = 2,
    Env = 3,
    Data = 4,
    Endpoint = 5,
    Store = 6,
    Gateway = 7,
    /// One host process owned by an environment (052 §10.2).
    HostProcess = 8,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Scope {
    Machine,
    Checkout(CheckoutId),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointKind {
    Domain,
    Address,
    Port,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResourceId {
    kind: ResourceKind,
    scope: Scope,
    key: Vec<Option<String>>,
}
impl ResourceId {
    fn new(kind: ResourceKind, scope: Scope, key: Vec<Option<String>>) -> Self {
        Self { kind, scope, key }
    }
    /// Equal coordinates under a persistent token and its filesystem-derived
    /// checkout alias are the same logical owner, not auxiliary resources.
    pub(crate) fn with_checkout(&self, checkout: CheckoutId) -> Result<Self, IdentityError> {
        if !matches!(self.scope, Scope::Checkout(_)) {
            return Err(IdentityError::InvalidInvocation);
        }
        Self::new(self.kind, Scope::Checkout(checkout), self.key.clone()).validate_record()
    }
    pub(crate) fn checkout_alias_of(&self, other: &Self) -> bool {
        self.kind == other.kind
            && self.key == other.key
            && matches!(self.scope, Scope::Checkout(_))
            && matches!(other.scope, Scope::Checkout(_))
    }
    pub fn environment(checkout: CheckoutId, name: &str) -> Result<Self, IdentityError> {
        Ok(Self::new(
            ResourceKind::Env,
            Scope::Checkout(checkout),
            vec![Some(text(name)?)],
        ))
    }
    /// A host process of `environment` in `profile`: its own gate, in the
    /// environment's checkout scope, so the environment can settle it
    /// without holding `Gate(Env)`.
    pub fn host_process(
        checkout: CheckoutId,
        environment: &str,
        profile: &str,
        process: &str,
    ) -> Result<Self, IdentityError> {
        Ok(Self::new(
            ResourceKind::HostProcess,
            Scope::Checkout(checkout),
            vec![
                Some(text(environment)?),
                Some(text(profile)?),
                Some(text(process)?),
            ],
        ))
    }
    pub fn data(engine: &str, context: &str, volume: &str) -> Result<Self, IdentityError> {
        Ok(Self::new(
            ResourceKind::Data,
            Scope::Machine,
            vec![
                Some(text(engine)?),
                Some(text(context)?),
                Some(text(volume)?),
            ],
        ))
    }
    pub fn endpoint(kind: EndpointKind, value: &str) -> Result<Self, IdentityError> {
        let (kind, value) = match kind {
            EndpointKind::Domain => {
                let name = value.trim_end_matches('.').to_ascii_lowercase();
                if name.is_empty()
                    || name.len() > 253
                    || name.split('.').any(|part| {
                        part.is_empty()
                            || part.len() > 63
                            || part.starts_with('-')
                            || part.ends_with('-')
                            || !part.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
                    })
                {
                    return Err(IdentityError::InvalidEndpoint);
                }
                ("domain", name)
            }
            EndpointKind::Address => (
                "address",
                value
                    .parse::<std::net::IpAddr>()
                    .map_err(|_| IdentityError::InvalidEndpoint)?
                    .to_string(),
            ),
            EndpointKind::Port => (
                "port",
                value
                    .parse::<u16>()
                    .map_err(|_| IdentityError::InvalidEndpoint)?
                    .to_string(),
            ),
        };
        Ok(Self::new(
            ResourceKind::Endpoint,
            Scope::Machine,
            vec![Some(kind.into()), Some(value)],
        ))
    }
    pub fn store(scope: Scope, name: &str) -> Result<Self, IdentityError> {
        Ok(Self::new(
            ResourceKind::Store,
            scope,
            vec![Some(text(name)?)],
        ))
    }
    pub fn gateway(instance: &str) -> Result<Self, IdentityError> {
        Ok(Self::new(
            ResourceKind::Gateway,
            Scope::Machine,
            vec![Some(text(instance)?)],
        ))
    }
    pub fn kind(&self) -> ResourceKind {
        self.kind
    }
    pub fn scope(&self) -> Scope {
        self.scope
    }
    /// The key fields in encoding order, for engines that render or match
    /// their own resources. Identity is still the canonical encoding.
    pub fn key(&self) -> &[Option<String>] {
        &self.key
    }
    /// The closed shared-fact model. Checkout journals may also contain these
    /// canonical resources during import, but the machine store admits only these.
    pub(crate) fn is_machine_fact(&self) -> bool {
        self.scope == Scope::Machine
            && (matches!(self.kind, ResourceKind::Endpoint | ResourceKind::Gateway)
                || (self.kind == ResourceKind::Store
                    && self
                        .key
                        .first()
                        .and_then(Option::as_deref)
                        .is_some_and(|key| {
                            matches!(
                                key,
                                "gateway-endpoints" | "gateway-setup" | "ports" | "admission"
                            )
                        })))
    }
    pub fn canonical(&self) -> Vec<u8> {
        let mut out = b"effigy.rid\0\x01".to_vec();
        out.push(self.kind as u8);
        match self.scope {
            Scope::Machine => out.push(0),
            Scope::Checkout(checkout) => {
                out.extend([1, checkout.kind as u8]);
                out.extend(checkout.token);
            }
        }
        for field in &self.key {
            let bytes = field.as_deref().unwrap_or("").as_bytes();
            out.push(u8::from(field.is_some()));
            // Constructors check the u32 encoding boundary before storing fields.
            out.extend((bytes.len() as u32).to_be_bytes());
            out.extend(bytes);
        }
        out
    }
    /// Decode the stored encoding v1 without trusting its hashed filename or
    /// descriptive columns. Migration and gate discovery re-encode the result
    /// and require byte equality before using it as a resource identity.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, IdentityError> {
        fn take<'a>(input: &mut &'a [u8], n: usize) -> Result<&'a [u8], IdentityError> {
            let head = input.get(..n).ok_or(IdentityError::InvalidInvocation)?;
            *input = input.get(n..).ok_or(IdentityError::InvalidInvocation)?;
            Ok(head)
        }
        fn byte(input: &mut &[u8]) -> Result<u8, IdentityError> {
            let (head, tail) = input
                .split_first()
                .ok_or(IdentityError::InvalidInvocation)?;
            *input = tail;
            Ok(*head)
        }
        let mut input = bytes;
        if take(&mut input, 12)? != b"effigy.rid\0\x01" {
            return Err(IdentityError::InvalidInvocation);
        }
        let kind = match byte(&mut input)? {
            0 => ResourceKind::Checkout,
            1 => ResourceKind::Task,
            2 => ResourceKind::Run,
            3 => ResourceKind::Env,
            4 => ResourceKind::Data,
            5 => ResourceKind::Endpoint,
            6 => ResourceKind::Store,
            7 => ResourceKind::Gateway,
            8 => ResourceKind::HostProcess,
            _ => return Err(IdentityError::InvalidInvocation),
        };
        let scope = match byte(&mut input)? {
            0 => Scope::Machine,
            1 => {
                let kind = match byte(&mut input)? {
                    0 => CheckoutKind::Primary,
                    1 => CheckoutKind::LinkedWorktree,
                    2 => CheckoutKind::EphemeralClone,
                    3 => CheckoutKind::Machine,
                    4 => CheckoutKind::Filesystem,
                    _ => return Err(IdentityError::InvalidInvocation),
                };
                let token = take(&mut input, 16)?
                    .try_into()
                    .map_err(|_| IdentityError::InvalidInvocation)?;
                Scope::Checkout(CheckoutId::new(kind, token))
            }
            _ => return Err(IdentityError::InvalidInvocation),
        };
        let count = match kind {
            ResourceKind::Task => 4,
            ResourceKind::Data => 3,
            ResourceKind::HostProcess => 3,
            ResourceKind::Endpoint => 2,
            _ => 1,
        };
        let mut key = Vec::with_capacity(count);
        for _ in 0..count {
            let present = byte(&mut input)?;
            let length = u32::from_be_bytes(
                take(&mut input, 4)?
                    .try_into()
                    .map_err(|_| IdentityError::InvalidInvocation)?,
            ) as usize;
            let field = take(&mut input, length)?;
            key.push(match present {
                0 if field.is_empty() => None,
                1 => Some(
                    std::str::from_utf8(field)
                        .map_err(|_| IdentityError::InvalidInvocation)?
                        .to_owned(),
                ),
                _ => return Err(IdentityError::InvalidInvocation),
            });
        }
        if !input.is_empty() {
            return Err(IdentityError::InvalidInvocation);
        }
        let id = Self::new(kind, scope, key);
        let normalized = id.validate_record()?;
        if id != normalized || id.canonical() != bytes {
            return Err(IdentityError::InvalidInvocation);
        }
        Ok(id)
    }

    pub fn name(&self) -> GateName {
        GateName(base32(&Sha256::digest(self.canonical())[..20]))
    }
    pub fn label(&self) -> String {
        format!("{:?}:{:?}", self.kind, self.key)
    }
    /// Store insertion calls this before accepting an existing digest name.
    /// Even a cryptographic collision is refused with both canonical forms.
    pub fn check_existing(&self, existing: &[u8]) -> Result<(), IdentityError> {
        let incoming = self.canonical();
        if existing == incoming {
            Ok(())
        } else {
            Err(IdentityError::Collision {
                existing: existing.to_vec(),
                incoming,
            })
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GateName(String);
impl fmt::Display for GateName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Digest(pub [u8; 32]);
impl Digest {
    pub fn of(bytes: &[u8]) -> Self {
        Self(Sha256::digest(bytes).into())
    }
    /// Lowercase hex, exactly as a stored digest, a guest object name or a pin
    /// is written. Never a credential: a digest is public evidence.
    pub fn hex(&self) -> String {
        hex(&self.0)
    }
    /// Accept only the exact lowercase 64-hex form `hex` emits. Any other
    /// spelling is refused rather than normalized, so one digest has one name.
    pub fn parse_hex(text: &str) -> Option<Self> {
        let text = text.strip_prefix("sha256:").unwrap_or(text);
        if text.len() != 64 {
            return None;
        }
        let mut bytes = [0u8; 32];
        for (index, pair) in text.as_bytes().chunks_exact(2).enumerate() {
            let digit = |byte: u8| match byte {
                b'0'..=b'9' => Some(byte - b'0'),
                b'a'..=b'f' => Some(byte - b'a' + 10),
                _ => None,
            };
            bytes[index] = (digit(pair[0])? << 4) | digit(pair[1])?;
        }
        Some(Self(bytes))
    }
}
impl fmt::Display for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.hex())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Route {
    Task,
    Builtin(String),
    ExecAlias(String),
    Defer(String),
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExecutionTarget {
    Host,
    Container {
        env: ResourceId,
        service: String,
        user: String,
        workdir: String,
    },
}
/// Names and references only. An environment value has no slot in this type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvBinding {
    pub provenance: String,
    pub secret_reference: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AttachmentMode {
    Borrow,
    Exclusive { stop_on_release: bool },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentRequest {
    pub environment: ResourceId,
    pub mode: AttachmentMode,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvocationSpec {
    pub task: TaskId,
    pub route: Route,
    #[serde(
        serialize_with = "serialize_argv",
        deserialize_with = "deserialize_argv"
    )]
    pub argv: Vec<OsString>,
    /// Checkout-relative coordinates, normalized at resolution.
    pub cwd: String,
    pub target: ExecutionTarget,
    pub env: BTreeMap<String, EnvBinding>,
    pub owner: ResourceId,
    pub gates: Vec<ResourceId>,
    pub attachments: Vec<AttachmentRequest>,
    pub class: EffectClass,
    pub placement: Placement,
    /// Bundle pins, catalog and manifest digests. Sorted by provenance name.
    pub provenance: BTreeMap<String, Digest>,
}
#[derive(Debug, Clone, Serialize)]
pub struct ResolvedInvocation {
    schema: u16,
    spec: InvocationSpec,
    digest: Digest,
}
impl ResolvedInvocation {
    pub fn new(mut spec: InvocationSpec) -> Result<Self, IdentityError> {
        spec.cwd = canonical_path(&spec.cwd)?;
        spec.gates.sort_by_key(ResourceId::canonical);
        spec.gates.dedup();
        let mut canonical = b"effigy.invocation\0\x01".to_vec();
        canonical.extend(serde_json::to_vec(&spec).map_err(|_| IdentityError::InvalidInvocation)?);
        Ok(Self {
            schema: 1,
            spec,
            digest: Digest::of(&canonical),
        })
    }
    /// Maximum canonical invocation frame accepted from a helper pipe.
    pub const MAX_WIRE_BYTES: usize = 1 << 20;

    /// Decodes exactly the canonical wire representation emitted by Serialize.
    /// Rebuilds through the existing constructor and checks schema, ordering,
    /// normalization and digest together. This never resolves a selector again.
    pub fn decode(bytes: &[u8]) -> Result<Self, IdentityError> {
        if bytes.len() > Self::MAX_WIRE_BYTES {
            return Err(IdentityError::TooLong);
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            schema: u16,
            spec: InvocationSpec,
            digest: Digest,
        }
        let wire: Wire =
            serde_json::from_slice(bytes).map_err(|_| IdentityError::InvalidInvocation)?;
        if wire.schema != 1 {
            return Err(IdentityError::InvalidInvocation);
        }
        let invocation = Self::new(wire.spec)?;
        if invocation.digest != wire.digest
            || serde_json::to_vec(&invocation).map_err(|_| IdentityError::InvalidInvocation)?
                != bytes
        {
            return Err(IdentityError::InvalidInvocation);
        }
        Ok(invocation)
    }
    pub fn digest(&self) -> &Digest {
        &self.digest
    }
    pub fn spec(&self) -> &InvocationSpec {
        &self.spec
    }
}
fn text(value: &str) -> Result<String, IdentityError> {
    let normalized: String = value.nfc().collect();
    if normalized.is_empty() {
        return Err(IdentityError::Empty);
    }
    if normalized.len() > u32::MAX as usize {
        return Err(IdentityError::TooLong);
    }
    Ok(normalized)
}
fn canonical_path(value: &str) -> Result<String, IdentityError> {
    if value.starts_with('/') || value.contains(['\\', '\0']) {
        return Err(IdentityError::InvalidPath);
    }
    let normalized: String = value.nfc().collect();
    let mut parts = Vec::new();
    for part in normalized.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return Err(IdentityError::InvalidPath);
                }
            }
            _ => parts.push(part),
        }
    }
    let path = parts.join("/");
    if path.len() > u32::MAX as usize {
        return Err(IdentityError::TooLong);
    }
    Ok(path)
}
fn base32(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";
    let mut out = String::new();
    let mut bits = 0u32;
    let mut count = 0;
    for byte in bytes {
        bits = (bits << 8) | u32::from(*byte);
        count += 8;
        while count >= 5 {
            count -= 5;
            out.push(ALPHABET[((bits >> count) & 31) as usize] as char);
        }
    }
    if count != 0 {
        out.push(ALPHABET[((bits << (5 - count)) & 31) as usize] as char);
    }
    out
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

// Preserve arbitrary Unix argv bytes without lossy Unicode conversion.
fn serialize_argv<S: serde::Serializer>(
    argv: &[OsString],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let bytes: Vec<&[u8]> = argv.iter().map(|arg| arg.as_bytes()).collect();
    bytes.serialize(serializer)
}

// Recovery accepts only the exact normalized representation constructors emit.
impl<'de> Deserialize<'de> for ResourceId {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Stored {
            kind: ResourceKind,
            scope: Scope,
            key: Vec<Option<String>>,
        }
        let stored = Stored::deserialize(d)?;
        let candidate = Self::new(stored.kind, stored.scope, stored.key);
        let valid = candidate
            .validate_record()
            .map_err(serde::de::Error::custom)?;
        if candidate != valid {
            return Err(serde::de::Error::custom("noncanonical resource"));
        }
        Ok(valid)
    }
}
impl ResourceId {
    fn validate_record(&self) -> Result<Self, IdentityError> {
        let field = |n: usize| {
            self.key
                .get(n)
                .and_then(Option::as_deref)
                .ok_or(IdentityError::InvalidInvocation)
        };
        let count = match self.kind {
            ResourceKind::Task => 4,
            ResourceKind::Data => 3,
            ResourceKind::HostProcess => 3,
            ResourceKind::Endpoint => 2,
            _ => 1,
        };
        if self.key.len() != count {
            return Err(IdentityError::InvalidInvocation);
        }
        match (self.kind, self.scope) {
            (ResourceKind::Env, Scope::Checkout(c)) => Self::environment(c, field(0)?),
            (ResourceKind::HostProcess, Scope::Checkout(c)) => {
                Self::host_process(c, field(0)?, field(1)?, field(2)?)
            }
            (ResourceKind::Store, scope) => Self::store(scope, field(0)?),
            (ResourceKind::Gateway, Scope::Machine) => Self::gateway(field(0)?),
            (ResourceKind::Data, Scope::Machine) => Self::data(field(0)?, field(1)?, field(2)?),
            (ResourceKind::Endpoint, Scope::Machine) => Self::endpoint(
                match field(0)? {
                    "domain" => EndpointKind::Domain,
                    "address" => EndpointKind::Address,
                    "port" => EndpointKind::Port,
                    _ => return Err(IdentityError::InvalidEndpoint),
                },
                field(1)?,
            ),
            (ResourceKind::Task, Scope::Checkout(c)) => Ok(TaskId::new(
                c,
                field(0)?,
                match field(1)? {
                    "published" => Surface::Published,
                    "draft" => Surface::Draft,
                    _ => return Err(IdentityError::InvalidInvocation),
                },
                field(2)?,
                self.key[3].as_deref(),
            )?
            .resource()),
            (ResourceKind::Run, Scope::Machine) => Ok(field(0)?.parse::<RunId>()?.resource()),
            (ResourceKind::Checkout, Scope::Machine) => {
                let token = field(0)?.strip_prefix("filesystem:").unwrap_or(field(0)?);
                if token.len() != 32
                    || !token
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                {
                    return Err(IdentityError::InvalidInvocation);
                }
                Ok(self.clone())
            }
            _ => Err(IdentityError::InvalidInvocation),
        }
    }
}

impl std::str::FromStr for RunId {
    type Err = IdentityError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.len() != 26 {
            return Err(IdentityError::InvalidInvocation);
        }
        let mut bytes = [0_u8; 16];
        let mut accumulator = 0_u16;
        let mut bits = 0;
        let mut written = 0;
        for byte in value.bytes() {
            let digit = match byte {
                b'a'..=b'z' => byte - b'a',
                b'2'..=b'7' => byte - b'2' + 26,
                _ => return Err(IdentityError::InvalidInvocation),
            };
            accumulator = (accumulator << 5) | u16::from(digit);
            bits += 5;
            if bits >= 8 {
                bits -= 8;
                bytes[written] = (accumulator >> bits) as u8;
                written += 1;
            }
        }
        let id = Self(bytes);
        if id.to_string() != value {
            return Err(IdentityError::InvalidInvocation);
        }
        Ok(id)
    }
}

fn deserialize_argv<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<OsString>, D::Error> {
    use std::os::unix::ffi::OsStringExt;
    Vec::<Vec<u8>>::deserialize(d).map(|args| args.into_iter().map(OsString::from_vec).collect())
}

impl<'de> Deserialize<'de> for TaskId {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            checkout: CheckoutId,
            catalog: String,
            surface: Surface,
            task: String,
            profile: Option<String>,
        }
        let w = Wire::deserialize(d)?;
        let id = Self::new(
            w.checkout,
            &w.catalog,
            w.surface,
            &w.task,
            w.profile.as_deref(),
        )
        .map_err(serde::de::Error::custom)?;
        if id.catalog != w.catalog || id.task != w.task || id.profile != w.profile {
            return Err(serde::de::Error::custom("noncanonical task identity"));
        }
        Ok(id)
    }
}
