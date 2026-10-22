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
//! SOURCE ONLY: this module retains the adoption boundary only. The generated
//! setup definition and the experimental `setup-listener-handoff`,
//! `listener-handoff-status` and `remove-listener-handoff` commands were retired
//! in v0.15 (Q-009); the gateway runs as the operator through `effigy gateway
//! up`. Nothing here registers a launchd job, binds the privileged endpoints,
//! elevates, or migrates a live gateway.
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

/// launchd Sockets-dictionary key for the plain HTTP listener.
pub const HANDOFF_HTTP_SOCKET_NAME: &str = "http";
/// launchd Sockets-dictionary key for the TLS-terminating HTTPS listener.
pub const HANDOFF_HTTPS_SOCKET_NAME: &str = "https";

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

/// Errno mapping for the launchd activation call, which exists only on
/// macOS; on other platforms no activation is attempted and the enum's
/// `Other` label is never produced.
#[cfg(target_os = "macos")]
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
            crate::socket_ownership::GatewayTransport::Tcp
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
    pub(crate) fn activate_launchd_socket(
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
pub(crate) use launchd::activate_launchd_socket;

#[cfg(test)]
#[path = "listener_handoff/tests.rs"]
mod tests;
