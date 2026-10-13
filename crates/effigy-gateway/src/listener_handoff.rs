//! Launchd loopback listener handoff for the operator-owned gateway.
//!
//! Source implementation candidate for the `loopback-privileged-listener-handoff`
//! capability returned by task 155 (architecture 034). An
//! administrator-installed launchd definition owns two exact loopback sockets
//! — TCP `127.0.0.1:80` and `127.0.0.1:443` — and hands them, through
//! launchd's documented socket-activation ABI, to the gateway daemon running
//! as the authenticated non-root operator. The daemon validates every handed
//! descriptor before adopting it, refuses malformed or foreign input
//! fail-closed, and keeps DNS (UDP `127.0.0.1:15353`) and the TCP alias
//! services on their ordinary unprivileged startup paths.
//!
//! SOURCE ONLY: this module implements and privately qualifies the adoption
//! boundary and the generated setup definition. It never registers a launchd
//! job, never binds the privileged endpoints, never elevates, and never
//! migrates a live gateway. Administrator installation, operator-run
//! activation and cross-boot restore belong to Tom alone (architecture 034,
//! the guide's listener-handoff section).
//!
//! # ABI pin
//!
//! `launch_activate_socket(const char *name, int **fds, size_t *cnt)` from
//! `<launch.h>`, available since macOS 10.10, verified against
//! `MacOSX.sdk/usr/include/launch.h` in the Xcode SDK used to build this tree.
//! The call returns zero on success and a POSIX errno otherwise (`ENOENT`: no
//! socket of that name is owned by the caller; `ESRCH`: the caller is not a
//! process managed by launchd; `EALREADY`: the caller already activated the
//! socket). On success the caller owns `cnt` descriptors delivered in one
//! `malloc(3)` allocation and must `free(3)` that array; every descriptor is
//! owned by the caller and must be closed exactly once when it is no longer
//! used. launchd leaves the exec-side state of the descriptors unspecified for
//! this contract, so this module sets `FD_CLOEXEC` on every descriptor it
//! accepts and closes every descriptor it refuses.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::os::fd::FromRawFd;
use std::os::unix::io::RawFd;
use std::path::PathBuf;

/// launchd Sockets-dictionary key for the plain HTTP listener.
pub const HANDOFF_HTTP_SOCKET_NAME: &str = "http";
/// launchd Sockets-dictionary key for the TLS-terminating HTTPS listener.
pub const HANDOFF_HTTPS_SOCKET_NAME: &str = "https";
/// Label of the generated administrator-installed launchd definition.
pub const LAUNCHD_JOB_LABEL: &str = "dev.effigy.gateway.user";
/// Flag the generated definition passes so the daemon opts into adoption.
pub const HANDOFF_RUN_FLAG: &str = "--listener-handoff";
/// Internal gateway subcommand the definition launches.
pub const HANDOFF_RUN_SUBCOMMAND: &str = "__gateway-run";

/// Exact loopback endpoints the handoff supports, with their launchd names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HandedSocketSpec {
    /// launchd socket name from the definition's Sockets dictionary.
    pub name: &'static str,
    /// The one loopback endpoint the descriptor must present.
    pub endpoint: SocketAddr,
}

/// The production handoff contract: exactly `127.0.0.1:80` and `127.0.0.1:443`.
pub fn production_specs() -> [HandedSocketSpec; 2] {
    [
        HandedSocketSpec {
            name: HANDOFF_HTTP_SOCKET_NAME,
            endpoint: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 80),
        },
        HandedSocketSpec {
            name: HANDOFF_HTTPS_SOCKET_NAME,
            endpoint: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 443),
        },
    ]
}

/// Typed identity view for handoff admission.
///
/// The daemon must run as the authenticated non-root operator. Membership is
/// never accepted from the environment: a sudo elevation environment is a
/// refusal, not an authorizer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HandoffIdentity {
    /// Effective UID the launchd job runs under.
    pub effective_uid: u32,
    /// True when any `SUDO_*` environment key is present.
    pub sudo_environment_present: bool,
}

impl HandoffIdentity {
    /// Read the current process identity.
    pub fn current() -> Self {
        Self {
            effective_uid: nix::unistd::Uid::effective().as_raw(),
            sudo_environment_present: ["SUDO_UID", "SUDO_GID", "SUDO_USER"]
                .iter()
                .any(|key| std::env::var_os(key).is_some()),
        }
    }
}

/// Bounded label for a launchd activation failure mapped from its errno.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivationFailure {
    /// `ENOENT`: no socket of that name is owned by the caller.
    MissingSocket,
    /// `ESRCH`: the caller is not a launchd-managed process.
    NotLaunchedByLaunchd,
    /// `EALREADY`: the caller activated this socket before.
    AlreadyActivated,
    /// Any other errno from the activation call.
    Other,
}

impl ActivationFailure {
    fn from_errno(errno: i32) -> Self {
        match errno {
            libc::ENOENT => Self::MissingSocket,
            libc::ESRCH => Self::NotLaunchedByLaunchd,
            libc::EALREADY => Self::AlreadyActivated,
            _ => Self::Other,
        }
    }
}

/// Why one handed descriptor failed validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DescriptorRefusal {
    /// The descriptor is closed or otherwise unusable.
    InvalidDescriptor,
    /// The same descriptor number appears more than once in the handoff.
    DuplicateDescriptor,
    /// The same underlying socket object was handed more than once.
    DuplicateSocketObject,
    /// The descriptor is not a stream socket.
    WrongSocketType,
    /// The descriptor is a stream socket but is not listening.
    NotListening,
    /// The socket is not IPv4.
    WrongFamily,
    /// The socket presents a different loopback endpoint than required.
    WrongEndpoint {
        /// Endpoint the definition and daemon expect.
        expected: SocketAddr,
        /// Endpoint the descriptor actually presents.
        actual: SocketAddr,
    },
    /// A kernel probe on the descriptor failed.
    ProbeDenied,
    /// The descriptor could not be marked close-on-exec.
    CloexecDenied,
}

/// Why adopting the launchd handoff failed. Every refusal is fail-closed: no
/// descriptor is adopted, handed descriptors are closed, and records are kept.
#[derive(Debug, thiserror::Error)]
pub enum ListenerHandoffError {
    /// launchd socket activation does not exist on this platform.
    #[error("listener handoff requires launchd socket activation, which is unsupported on this platform")]
    UnsupportedPlatform,
    /// The daemon would run as root instead of the operator.
    #[error("listener handoff refuses a root identity; launchd must run the daemon as the authenticated non-root operator")]
    RootIdentity,
    /// The invocation carries a sudo elevation environment.
    #[error("listener handoff refuses an elevated invocation; a sudo environment is not an operator launchd session")]
    ElevatedInvocation,
    /// launchd activation refused or could not serve the named socket.
    #[error("launchd activation failed for socket {socket:?}: {failure:?} (errno {errno})")]
    ActivationFailed {
        /// launchd socket name requested.
        socket: &'static str,
        /// Bounded failure label mapped from the errno.
        failure: ActivationFailure,
        /// Raw errno from `launch_activate_socket`.
        errno: i32,
    },
    /// launchd did not hand exactly one descriptor for the socket.
    #[error("launchd handed {count} descriptors for socket {socket:?}; expected exactly one")]
    UnexpectedDescriptorCount {
        /// launchd socket name.
        socket: &'static str,
        /// Number of descriptors handed (zero means the name is missing).
        count: usize,
    },
    /// One handed descriptor failed a validation step.
    #[error("handed descriptor for socket {socket:?} failed validation: {refusal:?}")]
    DescriptorRefused {
        /// launchd socket name.
        socket: &'static str,
        /// Bounded descriptor refusal label.
        refusal: DescriptorRefusal,
    },
    /// The gateway has no HTTPS server for the handed `127.0.0.1:443` socket.
    #[error("listener handoff requires an enabled HTTPS configuration for the handed 127.0.0.1:443 endpoint")]
    HttpsEndpointUnserved,
}

/// The two adopted listeners, in daemon serving order.
///
/// Ownership: each listener owns its descriptor. Dropping a listener closes
/// the descriptor.
#[derive(Debug)]
pub struct HandedListeners {
    /// Validated `127.0.0.1:80` listening socket.
    pub http: std::net::TcpListener,
    /// Validated `127.0.0.1:443` listening socket.
    pub https: std::net::TcpListener,
}

impl HandedListeners {
    /// Register both listeners with the tokio runtime.
    ///
    /// The sockets are placed in non-blocking mode first, as tokio requires.
    /// Ownership of each descriptor stays with its listener throughout.
    pub fn into_tokio(
        self,
    ) -> Result<(tokio::net::TcpListener, tokio::net::TcpListener), std::io::Error> {
        let HandedListeners { http, https } = self;
        http.set_nonblocking(true)?;
        let http = tokio::net::TcpListener::from_std(http)?;
        https.set_nonblocking(true)?;
        let https = tokio::net::TcpListener::from_std(https)?;
        Ok((http, https))
    }
}

/// Adopt the production handoff from the running launchd job.
///
/// Reads the real process identity, then activates each production socket
/// name through the launchd ABI and validates the handed descriptors against
/// the exact production endpoints. Outside launchd (including tests and
/// ordinary shells) this refuses with [`ActivationFailure::NotLaunchedByLaunchd`]
/// or [`ListenerHandoffError::UnsupportedPlatform`]; it never binds, elevates,
/// or retries.
pub fn adopt_handed_listeners() -> Result<HandedListeners, ListenerHandoffError> {
    #[cfg(target_os = "macos")]
    {
        let identity = HandoffIdentity::current();
        // Refuse a disallowed identity before asking launchd for descriptors,
        // so an elevated invocation never takes ownership of the sockets.
        check_identity(&identity)?;
        let specs = production_specs();
        let mut activated = Vec::with_capacity(specs.len());
        for spec in &specs {
            let fds = activate_launchd_socket(spec.name).map_err(|(failure, errno)| {
                ListenerHandoffError::ActivationFailed {
                    socket: spec.name,
                    failure,
                    errno,
                }
            })?;
            activated.push((spec.name, fds));
        }
        validate_and_adopt(&specs, &activated, &identity)
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err(ListenerHandoffError::UnsupportedPlatform)
    }
}

/// Validate handed descriptors and adopt them as gateway listeners.
///
/// This is the production adoption boundary, parameterized so private
/// fixtures can exercise it on ephemeral loopback endpoints.
///
/// Ownership contract: this call takes ownership of every descriptor in
/// `activated`. On success each adopted descriptor's ownership moves into the
/// returned listener. On any refusal every handed descriptor is closed exactly
/// once and nothing is adopted. Descriptors must not be owned anywhere else.
pub fn validate_and_adopt(
    specs: &[HandedSocketSpec],
    activated: &[(&'static str, Vec<RawFd>)],
    identity: &HandoffIdentity,
) -> Result<HandedListeners, ListenerHandoffError> {
    // Every refusal — identity, count, or descriptor validation — closes all
    // handed descriptors exactly once and adopts nothing.
    let adoption = check_identity(identity).and_then(|_| {
        let mut seen_socket_handles: Vec<u64> = Vec::new();
        let mut validated: Vec<RawFd> = Vec::new();
        for spec in specs {
            let fds = activated
                .iter()
                .find(|(name, _)| *name == spec.name)
                .map(|(_, fds)| fds.as_slice())
                .unwrap_or(&[]);
            let [fd] = fds else {
                return Err(ListenerHandoffError::UnexpectedDescriptorCount {
                    socket: spec.name,
                    count: fds.len(),
                });
            };
            validate_handed_fd(*fd, &spec.endpoint, &mut seen_socket_handles).map_err(
                |refusal| ListenerHandoffError::DescriptorRefused {
                    socket: spec.name,
                    refusal,
                },
            )?;
            validated.push(*fd);
        }
        // Conversion happens only after the complete set validated, so a
        // refusal never leaves a half-adopted listener behind.
        let mut sockets = validated.iter();
        let http = listener_from_owned_fd(*sockets.next().expect("http spec validated"));
        let https = listener_from_owned_fd(*sockets.next().expect("https spec validated"));
        Ok(HandedListeners { http, https })
    });
    if adoption.is_err() {
        // Close every handed descriptor exactly once. Duplicate numbers
        // (which a duplicate-descriptor refusal itself produces) close once.
        let mut closed: Vec<RawFd> = Vec::new();
        for (_, fds) in activated {
            for &fd in fds {
                if !closed.contains(&fd) {
                    close_owned_fd(fd);
                    closed.push(fd);
                }
            }
        }
    }
    adoption
}

/// Refuse identities that must never adopt the handoff.
fn check_identity(identity: &HandoffIdentity) -> Result<(), ListenerHandoffError> {
    if identity.effective_uid == 0 {
        return Err(ListenerHandoffError::RootIdentity);
    }
    if identity.sudo_environment_present {
        return Err(ListenerHandoffError::ElevatedInvocation);
    }
    Ok(())
}

/// Validate one handed descriptor against the exact expected endpoint.
///
/// Checks, in order: the descriptor is open, it is not a duplicate number or
/// duplicate socket object of one seen before in this handoff, it is a stream
/// socket, it is listening (libproc `PROC_PIDFDSOCKETINFO` TCP state on
/// macOS, which has no `SO_ACCEPTCONN`; `SO_ACCEPTCONN` on Linux), its local
/// name is the exact IPv4 loopback endpoint, and it accepts a close-on-exec
/// flag. Nothing is mutated before the descriptor has passed every preceding
/// check.
fn validate_handed_fd(
    fd: RawFd,
    expected: &SocketAddr,
    seen: &mut Vec<u64>,
) -> Result<(), DescriptorRefusal> {
    // SAFETY: `fd` is a descriptor handed to this process by launchd (or the
    // fixture boundary) and is not yet closed by this module; `F_GETFD` only
    // reads the descriptor's own flag word and has no other precondition.
    if unsafe { libc::fcntl(fd, libc::F_GETFD) } < 0 {
        return Err(DescriptorRefusal::InvalidDescriptor);
    }
    let socket_handle = fd_socket_identity(fd)?;
    if seen.contains(&socket_handle) {
        return Err(DescriptorRefusal::DuplicateSocketObject);
    }
    let socket_type = socket_int_option(fd, libc::SO_TYPE)?;
    if socket_type != libc::SOCK_STREAM {
        return Err(DescriptorRefusal::WrongSocketType);
    }
    if !fd_is_listening(fd)? {
        return Err(DescriptorRefusal::NotListening);
    }
    let actual = socket_local_addr(fd)?;
    if actual.family != libc::AF_INET {
        return Err(DescriptorRefusal::WrongFamily);
    }
    let actual_endpoint = SocketAddr::new(IpAddr::V4(actual.address), actual.port);
    if actual_endpoint != *expected {
        return Err(DescriptorRefusal::WrongEndpoint {
            expected: *expected,
            actual: actual_endpoint,
        });
    }
    // SAFETY: `fd` is an open, owned descriptor; `F_SETFD` writes only its
    // flag word. Marking it close-on-exec prevents a child spawned later from
    // inheriting the privileged listening socket.
    if unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) } < 0 {
        return Err(DescriptorRefusal::CloexecDenied);
    }
    seen.push(socket_handle);
    Ok(())
}

/// Stable kernel identity of the socket object behind one descriptor.
///
/// macOS reports `dev=0, ino=0` for every socket `fstat`, so identity comes
/// from the documented libproc `soi_so` handle instead; Linux socket inodes
/// are unique per socket object.
#[cfg(target_os = "macos")]
fn fd_socket_identity(fd: RawFd) -> Result<u64, DescriptorRefusal> {
    let pid = i32::try_from(std::process::id()).map_err(|_| DescriptorRefusal::ProbeDenied)?;
    crate::macos_socket::fd_socket_handle(pid, fd).map_err(|_| DescriptorRefusal::ProbeDenied)
}

#[cfg(not(target_os = "macos"))]
fn fd_socket_identity(fd: RawFd) -> Result<u64, DescriptorRefusal> {
    let mut file_stat: libc::stat = unsafe { std::mem::zeroed() };
    // SAFETY: `fd` is an open descriptor (the `F_GETFD` probe in the caller
    // just succeeded) and `file_stat` is writable storage for exactly one
    // `stat` record, which the kernel fills for that descriptor.
    if unsafe { libc::fstat(fd, &mut file_stat) } != 0 {
        return Err(DescriptorRefusal::InvalidDescriptor);
    }
    Ok(file_stat.st_ino as u64)
}

struct SocketLocalName {
    family: i32,
    address: Ipv4Addr,
    port: u16,
}

/// Listening-state probe for one descriptor, split by platform.
///
/// macOS does not implement `SO_ACCEPTCONN` (`getsockopt` answers
/// `ENOPROTOOPT`), so the listening state is read through the documented
/// libproc `PROC_PIDFDSOCKETINFO` TCP state the crate already uses for
/// legacy recovery. Linux implements `SO_ACCEPTCONN` and answers it.
#[cfg(target_os = "macos")]
fn fd_is_listening(fd: RawFd) -> Result<bool, DescriptorRefusal> {
    let pid = i32::try_from(std::process::id()).map_err(|_| DescriptorRefusal::ProbeDenied)?;
    match crate::macos_socket::socket_endpoint(pid, fd) {
        Ok(Some(endpoint)) => Ok(matches!(
            endpoint.transport,
            crate::legacy::GatewayTransport::Tcp
        )),
        Ok(None) => Ok(false),
        Err(_) => Err(DescriptorRefusal::ProbeDenied),
    }
}

#[cfg(not(target_os = "macos"))]
fn fd_is_listening(fd: RawFd) -> Result<bool, DescriptorRefusal> {
    Ok(socket_int_option(fd, libc::SO_ACCEPTCONN)? != 0)
}

fn socket_local_addr(fd: RawFd) -> Result<SocketLocalName, DescriptorRefusal> {
    // All-zero `sockaddr_in` is a valid uninitialized address record for the
    // kernel to overwrite; getsockname writes at most `sizeof(sockaddr_in)`.
    let mut storage: libc::sockaddr_in = unsafe { std::mem::zeroed() };
    let mut length = std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t;
    // SAFETY: `storage` is writable memory of exactly `length` bytes, and
    // `length` is initialized. The kernel fills the record for the open
    // socket `fd` and reports the written length through `length`.
    let result = unsafe {
        libc::getsockname(
            fd,
            (&mut storage as *mut libc::sockaddr_in).cast::<libc::sockaddr>(),
            &mut length,
        )
    };
    if result != 0 {
        return Err(DescriptorRefusal::ProbeDenied);
    }
    Ok(SocketLocalName {
        family: i32::from(storage.sin_family),
        // `s_addr` is in network byte order; one big-endian read yields the
        // host-order value `Ipv4Addr::from` expects.
        address: Ipv4Addr::from(u32::from_be(storage.sin_addr.s_addr)),
        port: u16::from_be(storage.sin_port),
    })
}

fn socket_int_option(fd: RawFd, option: libc::c_int) -> Result<i32, DescriptorRefusal> {
    let mut value: libc::c_int = 0;
    let mut length = std::mem::size_of::<libc::c_int>() as libc::socklen_t;
    // SAFETY: `value` is writable storage of exactly `length` bytes for the
    // integer-valued socket option on the open descriptor `fd`.
    let result = unsafe {
        libc::getsockopt(
            fd,
            libc::SOL_SOCKET,
            option,
            (&mut value as *mut libc::c_int).cast(),
            &mut length,
        )
    };
    if result != 0 {
        return Err(DescriptorRefusal::ProbeDenied);
    }
    Ok(value)
}

/// Convert a validated owned descriptor into a listener.
///
/// Caller contract: `fd` is an open, owned descriptor that
/// [`validate_handed_fd`] just accepted and it is not owned anywhere else.
/// The returned listener then owns the descriptor; dropping it closes the
/// descriptor exactly once.
fn listener_from_owned_fd(fd: RawFd) -> std::net::TcpListener {
    // SAFETY: `fd` is an open, owned stream socket descriptor accepted by
    // validation, ownership is exclusive to this call, and ownership moves
    // into the returned `TcpListener` without duplication.
    unsafe { std::net::TcpListener::from_raw_fd(fd) }
}

/// Close an owned handed descriptor exactly once on a refusal path.
fn close_owned_fd(fd: RawFd) {
    // SAFETY: `fd` is an owned descriptor this module received in
    // `activated` and has not adopted into a listener; the refusal path
    // closes each handed descriptor exactly once and ignores the result
    // because the descriptor is being discarded.
    unsafe { libc::close(fd) };
}

#[cfg(target_os = "macos")]
mod launchd {
    //! The one scoped macOS FFI surface of this module.
    //!
    //! Pin: `launch_activate_socket` in `MacOSX.sdk/usr/include/launch.h`
    //! (macOS 10.10+): `int launch_activate_socket(const char *name, int **fds,
    //! size_t *cnt)`. See the module documentation for the ownership contract
    //! this wrapper implements.

    use super::{ActivationFailure, RawFd};
    use std::ffi::CString;

    #[link(name = "System")]
    extern "C" {
        // SAFETY (declaration): matches the pinned launch.h signature exactly:
        // `name` is a NUL-terminated read-only C string; `fds` and `cnt` are
        // writable out-parameters; the return value is zero or a POSIX errno.
        fn launch_activate_socket(
            name: *const std::ffi::c_char,
            fds: *mut *mut std::ffi::c_int,
            cnt: *mut usize,
        ) -> std::ffi::c_int;
    }

    /// Activate one launchd socket by name and take ownership of its fds.
    ///
    /// On success the caller owns every returned descriptor exactly once and
    /// must close or adopt each one. On failure no descriptor is returned.
    pub(super) fn activate_launchd_socket(
        name: &str,
    ) -> Result<Vec<RawFd>, (ActivationFailure, i32)> {
        let c_name = CString::new(name).map_err(|_| (ActivationFailure::Other, 0))?;
        let mut fds: *mut std::ffi::c_int = std::ptr::null_mut();
        let mut count: usize = 0;
        // SAFETY: `c_name` is a valid NUL-terminated string that stays
        // allocated for the duration of the call; `fds` and `count` are valid
        // writable out-parameters. Per the pinned header, a zero return means
        // `fds` now points to one malloc allocation of `count` initialized
        // `int` descriptors owned by this process, and any nonzero return
        // leaves nothing allocated, so no cleanup is needed on failure.
        let status = unsafe { launch_activate_socket(c_name.as_ptr(), &mut fds, &mut count) };
        if status != 0 {
            return Err((ActivationFailure::from_errno(status), status));
        }
        // Guard the header contract: a zero return with a null array or zero
        // count has no allocation to free and cannot describe a socket.
        if fds.is_null() || count == 0 {
            return Err((ActivationFailure::Other, 0));
        }
        let mut descriptors = Vec::with_capacity(count);
        for index in 0..count {
            // SAFETY: `fds` points to `count` initialized `c_int` elements in
            // one allocation (header contract above); `index` is in range.
            descriptors.push(unsafe { *fds.add(index) });
        }
        // SAFETY: `fds` is the single malloc allocation returned by
        // `launch_activate_socket`, not yet freed, and is freed exactly here.
        unsafe { libc::free(fds.cast()) };
        Ok(descriptors)
    }
}

#[cfg(target_os = "macos")]
use launchd::activate_launchd_socket;

/// Why generating or validating the setup definition failed.
#[derive(Debug, thiserror::Error)]
pub enum SetupDefinitionError {
    /// The executable path is not a canonical regular file.
    #[error("setup definition requires a canonical existing executable, not {path:?}")]
    ProgramPath {
        /// Path that was rejected.
        path: PathBuf,
    },
    /// The operator's passwd/group records could not be resolved.
    #[error("setup definition requires the resolved operator user and group records")]
    OperatorRecord,
    /// The operator home directory is not an absolute path.
    #[error("setup definition requires the operator's absolute home directory")]
    HomeDirectory,
}

/// Deterministic administrator setup definition for the launchd job.
///
/// The definition is generated bytes for Tom to review and install as an
/// administrator. It carries no trust claim: launchd does not verify the
/// executable's contents, and nothing here hashes or pins the binary beyond
/// naming the exact canonical path reviewed at generation time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListenerHandoffSetup {
    /// launchd job label.
    pub label: &'static str,
    /// Canonical path of the exact reviewed executable.
    pub program_path: PathBuf,
    /// Operator user name the job runs as.
    pub user_name: String,
    /// Operator group name the job runs as.
    pub group_name: String,
    /// Operator home directory the daemon's state root is bound to.
    pub home: PathBuf,
}

impl ListenerHandoffSetup {
    /// Build the definition for the user running this command.
    pub fn for_current_operator() -> Result<Self, SetupDefinitionError> {
        let user = nix::unistd::User::from_uid(nix::unistd::Uid::current())
            .map_err(|_| SetupDefinitionError::OperatorRecord)?
            .ok_or(SetupDefinitionError::OperatorRecord)?;
        let group = nix::unistd::Group::from_gid(user.gid)
            .map_err(|_| SetupDefinitionError::OperatorRecord)?
            .ok_or(SetupDefinitionError::OperatorRecord)?;
        let program_path = std::env::current_exe()
            .map_err(|_| SetupDefinitionError::ProgramPath {
                path: PathBuf::from("<current executable unavailable>"),
            })?
            .canonicalize()
            .map_err(|_| SetupDefinitionError::ProgramPath {
                path: std::env::current_exe()
                    .unwrap_or_else(|_| PathBuf::from("<current executable unavailable>")),
            })?;
        let setup = Self {
            label: LAUNCHD_JOB_LABEL,
            program_path,
            user_name: user.name,
            group_name: group.name,
            home: user.dir,
        };
        setup.validate()?;
        Ok(setup)
    }

    /// Check the definition describes an installable job.
    pub fn validate(&self) -> Result<(), SetupDefinitionError> {
        if !self.program_path.is_absolute() || !self.program_path.is_file() {
            return Err(SetupDefinitionError::ProgramPath {
                path: self.program_path.clone(),
            });
        }
        if self.user_name.is_empty() || self.group_name.is_empty() {
            return Err(SetupDefinitionError::OperatorRecord);
        }
        if !self.home.is_absolute() {
            return Err(SetupDefinitionError::HomeDirectory);
        }
        Ok(())
    }

    /// Program arguments the definition launches: the reviewed executable,
    /// the internal daemon subcommand, and the adoption opt-in flag.
    pub fn program_arguments(&self) -> [String; 3] {
        [
            self.program_path.display().to_string(),
            HANDOFF_RUN_SUBCOMMAND.to_owned(),
            HANDOFF_RUN_FLAG.to_owned(),
        ]
    }

    /// Render the deterministic launchd plist for this definition.
    pub fn render_launchd_plist(&self) -> String {
        let [program, subcommand, flag] = self.program_arguments();
        let mut plist = String::new();
        plist.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        plist.push_str("<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n");
        plist.push_str("<plist version=\"1.0\">\n");
        plist.push_str("<dict>\n");
        plist.push_str("  <key>Label</key>\n");
        plist.push_str(&format!("  <string>{}</string>\n", escape_xml(self.label)));
        plist.push_str("  <key>ProgramArguments</key>\n");
        plist.push_str("  <array>\n");
        for argument in [program, subcommand, flag] {
            plist.push_str(&format!("    <string>{}</string>\n", escape_xml(&argument)));
        }
        plist.push_str("  </array>\n");
        plist.push_str("  <key>UserName</key>\n");
        plist.push_str(&format!(
            "  <string>{}</string>\n",
            escape_xml(&self.user_name)
        ));
        plist.push_str("  <key>GroupName</key>\n");
        plist.push_str(&format!(
            "  <string>{}</string>\n",
            escape_xml(&self.group_name)
        ));
        plist.push_str("  <key>EnvironmentVariables</key>\n");
        plist.push_str("  <dict>\n");
        plist.push_str("    <key>HOME</key>\n");
        plist.push_str(&format!(
            "    <string>{}</string>\n",
            escape_xml(&self.home.display().to_string())
        ));
        plist.push_str("  </dict>\n");
        plist.push_str("  <key>RunAtLoad</key>\n  <true/>\n");
        plist.push_str("  <key>KeepAlive</key>\n  <true/>\n");
        plist.push_str("  <key>Sockets</key>\n");
        plist.push_str("  <dict>\n");
        for spec in production_specs() {
            plist.push_str(&format!("    <key>{}</key>\n", spec.name));
            plist.push_str("    <dict>\n");
            plist.push_str("      <key>SockFamily</key>\n      <string>IPv4</string>\n");
            plist.push_str("      <key>SockNodeName</key>\n      <string>127.0.0.1</string>\n");
            plist.push_str(&format!(
                "      <key>SockPort</key>\n      <integer>{}</integer>\n",
                spec.endpoint.port()
            ));
            plist.push_str("      <key>SockType</key>\n      <string>TCP</string>\n");
            plist.push_str("    </dict>\n");
        }
        plist.push_str("  </dict>\n");
        plist.push_str("</dict>\n");
        plist.push_str("</plist>\n");
        plist
    }
}

/// Escape the five XML text characters for interpolation into the plist.
fn escape_xml(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            other => escaped.push(other),
        }
    }
    escaped
}

/// A parsed plist subset: what the generator emits and its parser accepts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlistValue {
    /// `<string>` element.
    String(String),
    /// `<integer>` element.
    Integer(i64),
    /// `<true/>` or `<false/>`.
    Boolean(bool),
    /// `<array>` element.
    Array(Vec<PlistValue>),
    /// `<dict>` element with ordered key/value pairs.
    Dict(Vec<(String, PlistValue)>),
}

impl PlistValue {
    /// Value of a dict key, if this is a dict holding it.
    pub fn get(&self, key: &str) -> Option<&PlistValue> {
        match self {
            PlistValue::Dict(entries) => entries
                .iter()
                .find(|(candidate, _)| candidate == key)
                .map(|(_, value)| value),
            _ => None,
        }
    }

    /// String content, if this is a string.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            PlistValue::String(value) => Some(value),
            _ => None,
        }
    }

    /// Integer content, if this is an integer.
    pub fn as_integer(&self) -> Option<i64> {
        match self {
            PlistValue::Integer(value) => Some(*value),
            _ => None,
        }
    }

    /// Boolean content, if this is a boolean.
    pub fn as_boolean(&self) -> Option<bool> {
        match self {
            PlistValue::Boolean(value) => Some(*value),
            _ => None,
        }
    }
}

/// Why the definition bytes could not be parsed.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DefinitionParseError {
    /// The bytes do not start with the expected XML prolog.
    #[error("definition must start with an XML prolog")]
    MissingProlog,
    /// The plist DOCTYPE declaration is missing.
    #[error("definition must carry the plist DOCTYPE declaration")]
    MissingDoctype,
    /// The parser met a tag outside its supported subset.
    #[error("definition contains an unsupported element <{0}>")]
    UnsupportedElement(String),
    /// A close tag did not match the open tag.
    #[error("definition has a mismatched close tag </{0}>")]
    MismatchedCloseTag(String),
    /// Text appeared where only elements are allowed.
    #[error("definition has text outside a string or integer element")]
    UnexpectedText,
    /// The bytes ended before the document was complete.
    #[error("definition ended before the document was complete")]
    UnexpectedEnd,
    /// An integer element did not contain an integer.
    #[error("definition contains a malformed integer")]
    MalformedInteger,
    /// An entity reference is not in the supported set.
    #[error("definition contains an unsupported entity reference")]
    UnsupportedEntity,
    /// Bytes remained after the complete document.
    #[error("definition has trailing content after the root element")]
    TrailingContent,
}

/// Parse the generator's plist subset strictly.
///
/// Accepts exactly the shape [`ListenerHandoffSetup::render_launchd_plist`]
/// emits — prolog, plist DOCTYPE, one dict of strings, integers, booleans,
/// arrays and dicts — and refuses everything else, including attributes,
/// comments and unknown entities.
pub fn parse_launchd_definition(text: &str) -> Result<PlistValue, DefinitionParseError> {
    let mut rest = text
        .strip_prefix("<?xml")
        .ok_or(DefinitionParseError::MissingProlog)?;
    let prolog_end = rest.find("?>").ok_or(DefinitionParseError::MissingProlog)?;
    rest = &rest[prolog_end + 2..];
    skip_whitespace(&mut rest);
    let Some(after_doctype) = rest.strip_prefix("<!DOCTYPE") else {
        return Err(DefinitionParseError::MissingDoctype);
    };
    let doctype_end = after_doctype
        .find('>')
        .ok_or(DefinitionParseError::UnexpectedEnd)?;
    rest = &after_doctype[doctype_end + 1..];
    skip_whitespace(&mut rest);
    rest = rest.strip_prefix("<plist version=\"1.0\">").ok_or(
        DefinitionParseError::UnsupportedElement("expected plist root element".to_owned()),
    )?;
    let value = parse_value(&mut rest)?;
    skip_whitespace(&mut rest);
    let Some(after_root) = rest.strip_prefix("</plist>") else {
        return Err(DefinitionParseError::MismatchedCloseTag("plist".to_owned()));
    };
    rest = after_root;
    skip_whitespace(&mut rest);
    if !rest.is_empty() {
        return Err(DefinitionParseError::TrailingContent);
    }
    Ok(value)
}

fn skip_whitespace(input: &mut &str) {
    *input = input.trim_start();
}

fn parse_value(input: &mut &str) -> Result<PlistValue, DefinitionParseError> {
    skip_whitespace(input);
    let Some(rest) = input.strip_prefix('<') else {
        return Err(DefinitionParseError::UnexpectedEnd);
    };
    let tag_end = rest.find('>').ok_or(DefinitionParseError::UnexpectedEnd)?;
    let tag = &rest[..tag_end];
    *input = &rest[tag_end + 1..];
    match tag {
        "dict" => parse_dict(input),
        "array" => parse_array(input),
        "true/" => Ok(PlistValue::Boolean(true)),
        "false/" => Ok(PlistValue::Boolean(false)),
        "string" => Ok(PlistValue::String(parse_text(input, "string")?)),
        "integer" => {
            let text = parse_text(input, "integer")?;
            let value = text
                .trim()
                .parse::<i64>()
                .map_err(|_| DefinitionParseError::MalformedInteger)?;
            Ok(PlistValue::Integer(value))
        }
        other => Err(DefinitionParseError::UnsupportedElement(other.to_owned())),
    }
}

fn parse_dict(input: &mut &str) -> Result<PlistValue, DefinitionParseError> {
    let mut entries = Vec::new();
    loop {
        skip_whitespace(input);
        if input.starts_with("</dict>") {
            *input = &input["</dict>".len()..];
            return Ok(PlistValue::Dict(entries));
        }
        let Some(rest) = input.strip_prefix("<key>") else {
            return Err(expected_close_or_key(input));
        };
        *input = rest;
        let key = parse_text(input, "key")?;
        skip_whitespace(input);
        let value = parse_value(input)?;
        entries.push((key, value));
    }
}

fn expected_close_or_key(input: &str) -> DefinitionParseError {
    match input.strip_prefix("</") {
        Some(rest) => {
            let tag = rest.split(['>', ' ']).next().unwrap_or_default();
            DefinitionParseError::MismatchedCloseTag(tag.to_owned())
        }
        None if input.is_empty() => DefinitionParseError::UnexpectedEnd,
        None => DefinitionParseError::UnexpectedText,
    }
}

fn parse_array(input: &mut &str) -> Result<PlistValue, DefinitionParseError> {
    let mut values = Vec::new();
    loop {
        skip_whitespace(input);
        if input.starts_with("</array>") {
            *input = &input["</array>".len()..];
            return Ok(PlistValue::Array(values));
        }
        values.push(parse_value(input)?);
    }
}

fn parse_text(input: &mut &str, element: &str) -> Result<String, DefinitionParseError> {
    let close = format!("</{element}>");
    let end = input
        .find(&close)
        .ok_or(DefinitionParseError::UnexpectedEnd)?;
    let raw = &input[..end];
    *input = &input[end + close.len()..];
    unescape_entities(raw)
}

fn unescape_entities(raw: &str) -> Result<String, DefinitionParseError> {
    let mut value = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(position) = rest.find('&') {
        value.push_str(&rest[..position]);
        rest = &rest[position..];
        let entity_end = rest
            .find(';')
            .ok_or(DefinitionParseError::UnsupportedEntity)?;
        let entity = &rest[1..entity_end];
        let decoded = match entity {
            "amp" => "&",
            "lt" => "<",
            "gt" => ">",
            "quot" => "\"",
            "apos" => "'",
            _ => return Err(DefinitionParseError::UnsupportedEntity),
        };
        value.push_str(decoded);
        rest = &rest[entity_end + 1..];
    }
    value.push_str(rest);
    Ok(value)
}

/// Why parsed definition bytes are not the known-good handoff definition.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DefinitionRefusal {
    /// The root element is not a dict.
    #[error("definition root must be a dict")]
    RootNotADict,
    /// A required top-level key is missing or malformed.
    #[error("definition is missing or has malformed {0}")]
    MissingOrMalformed(&'static str),
    /// Label differs from the maintained job label.
    #[error("definition label must be {LAUNCHD_JOB_LABEL}")]
    WrongLabel,
    /// Program arguments are not exactly executable, subcommand, opt-in flag.
    #[error(
        "definition ProgramArguments must be the executable, __gateway-run, and --listener-handoff"
    )]
    WrongProgramArguments,
    /// Sockets are not exactly the two loopback endpoints.
    #[error("definition Sockets must be exactly http=127.0.0.1:80 and https=127.0.0.1:443 over IPv4 TCP")]
    WrongSockets,
}

/// Validate parsed definition bytes against the known-good handoff shape.
///
/// This is the "unknown config refuses" control: any definition whose parsed
/// values differ from the generated contract — a different label, a wildcard
/// node, another port, extra socket keys, missing KeepAlive — refuses.
pub fn validate_launchd_definition(value: &PlistValue) -> Result<(), DefinitionRefusal> {
    let dict = match value {
        PlistValue::Dict(_) => value,
        _ => return Err(DefinitionRefusal::RootNotADict),
    };
    let label = dict
        .get("Label")
        .and_then(PlistValue::as_str)
        .ok_or(DefinitionRefusal::MissingOrMalformed("Label"))?;
    if label != LAUNCHD_JOB_LABEL {
        return Err(DefinitionRefusal::WrongLabel);
    }
    let arguments = match dict.get("ProgramArguments") {
        Some(PlistValue::Array(arguments)) if arguments.len() == 3 => arguments,
        _ => return Err(DefinitionRefusal::MissingOrMalformed("ProgramArguments")),
    };
    let program = arguments[0].as_str().unwrap_or_default();
    if program.is_empty()
        || arguments[1].as_str() != Some(HANDOFF_RUN_SUBCOMMAND)
        || arguments[2].as_str() != Some(HANDOFF_RUN_FLAG)
    {
        return Err(DefinitionRefusal::WrongProgramArguments);
    }
    for key in ["UserName", "GroupName"] {
        let name = dict
            .get(key)
            .and_then(PlistValue::as_str)
            .ok_or(DefinitionRefusal::MissingOrMalformed(key))?;
        if name.is_empty() {
            return Err(DefinitionRefusal::MissingOrMalformed(key));
        }
    }
    let environment =
        dict.get("EnvironmentVariables")
            .ok_or(DefinitionRefusal::MissingOrMalformed(
                "EnvironmentVariables",
            ))?;
    match environment {
        PlistValue::Dict(entries) if entries.len() == 1 => {
            let home = environment.get("HOME").and_then(PlistValue::as_str);
            if home.is_none_or(str::is_empty) {
                return Err(DefinitionRefusal::MissingOrMalformed("HOME"));
            }
        }
        _ => {
            return Err(DefinitionRefusal::MissingOrMalformed(
                "EnvironmentVariables",
            ))
        }
    }
    for key in ["RunAtLoad", "KeepAlive"] {
        if dict.get(key).and_then(PlistValue::as_boolean) != Some(true) {
            return Err(DefinitionRefusal::MissingOrMalformed(key));
        }
    }
    let sockets = match dict.get("Sockets") {
        Some(PlistValue::Dict(sockets)) if sockets.len() == 2 => sockets,
        _ => return Err(DefinitionRefusal::WrongSockets),
    };
    for spec in production_specs() {
        let Some(socket) = sockets
            .iter()
            .find(|(name, _)| name == spec.name)
            .map(|(_, value)| value)
        else {
            return Err(DefinitionRefusal::WrongSockets);
        };
        let matches =
            matches_socket_dict(socket, spec).map_err(|_| DefinitionRefusal::WrongSockets)?;
        if !matches {
            return Err(DefinitionRefusal::WrongSockets);
        }
    }
    Ok(())
}

fn matches_socket_dict(
    socket: &PlistValue,
    spec: HandedSocketSpec,
) -> Result<bool, DefinitionRefusal> {
    let PlistValue::Dict(entries) = socket else {
        return Err(DefinitionRefusal::MissingOrMalformed("Sockets"));
    };
    if entries.len() != 4 {
        return Ok(false);
    }
    let expected: [(&str, PlistValue); 4] = [
        ("SockFamily", PlistValue::String("IPv4".to_owned())),
        ("SockNodeName", PlistValue::String("127.0.0.1".to_owned())),
        (
            "SockPort",
            PlistValue::Integer(i64::from(spec.endpoint.port())),
        ),
        ("SockType", PlistValue::String("TCP".to_owned())),
    ];
    for (key, expected_value) in &expected {
        if socket.get(key) != Some(expected_value) {
            return Ok(false);
        }
    }
    Ok(true)
}

#[cfg(test)]
#[path = "listener_handoff/tests.rs"]
mod tests;
