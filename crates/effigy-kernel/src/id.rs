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
        write!(f, "invalid kernel identity: {self:?}")
    }
}
impl std::error::Error for IdentityError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum CheckoutKind {
    Primary = 0,
    LinkedWorktree = 1,
    EphemeralClone = 2,
    Machine = 3,
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
    pub fn resource(&self) -> ResourceId {
        ResourceId::new(
            ResourceKind::Checkout,
            Scope::Machine,
            vec![Some(hex(&self.token))],
        )
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
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
    pub fn environment(checkout: CheckoutId, name: &str) -> Result<Self, IdentityError> {
        Ok(Self::new(
            ResourceKind::Env,
            Scope::Checkout(checkout),
            vec![Some(text(name)?)],
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Digest(pub [u8; 32]);
impl Digest {
    pub fn of(bytes: &[u8]) -> Self {
        Self(Sha256::digest(bytes).into())
    }
}
#[derive(Debug, Clone, Serialize)]
pub enum Route {
    Task,
    Builtin(String),
    ExecAlias(String),
    Defer(String),
}
#[derive(Debug, Clone, Serialize)]
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
#[derive(Debug, Clone, Serialize)]
pub struct EnvBinding {
    pub provenance: String,
    pub secret_reference: Option<String>,
}
#[derive(Debug, Clone, Serialize)]
pub enum AttachmentMode {
    Borrow,
    Exclusive { stop_on_release: bool },
}
#[derive(Debug, Clone, Serialize)]
pub struct AttachmentRequest {
    pub environment: ResourceId,
    pub mode: AttachmentMode,
}
#[derive(Debug, Clone, Serialize)]
pub struct InvocationSpec {
    pub task: TaskId,
    pub route: Route,
    #[serde(serialize_with = "serialize_argv")]
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
            ResourceKind::Endpoint => 2,
            _ => 1,
        };
        if self.key.len() != count {
            return Err(IdentityError::InvalidInvocation);
        }
        match (self.kind, self.scope) {
            (ResourceKind::Env, Scope::Checkout(c)) => Self::environment(c, field(0)?),
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
            (ResourceKind::Checkout, Scope::Machine)
                if field(0)?.len() == 32
                    && field(0)?
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) =>
            {
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
