//! Private inherited socketpair: workloads inherit neither endpoint. The
//! request tag binds one datagram's descriptors to its pipe control frame.
use nix::sys::socket::{sendmsg, ControlMessage, MsgFlags};
use std::io::{self, IoSlice};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::net::UnixDatagram;

pub(crate) fn send(
    socket: &UnixDatagram,
    request: u64,
    descriptors: &[RawFd; 3],
) -> io::Result<()> {
    let tag = request.to_be_bytes();
    let count = sendmsg::<()>(
        socket.as_raw_fd(),
        &[IoSlice::new(&tag)],
        &[ControlMessage::ScmRights(descriptors)],
        MsgFlags::empty(),
        None,
    )?;
    if count != tag.len() {
        return Err(io::Error::other(
            "incomplete interpolation descriptor datagram",
        ));
    }
    Ok(())
}
pub(crate) fn receive(socket: &UnixDatagram, request: u64) -> io::Result<Vec<OwnedFd>> {
    let mut tag = [0u8; 8];
    // Word-aligned storage for cmsghdr, including on platforms whose ancillary
    // alignment is wider than RawFd. Truncation is parsed before refusal so
    // every descriptor actually installed by the kernel is closed on error.
    // XNU UIPC_MAX_CMSG_FD is 512; Linux SCM_MAX_FD is 253. Receive a
    // complete maximum-sized rights message even though this private protocol
    // sends exactly three. Darwin can install rights it omits on truncation.
    let mut ancillary = [0usize; 512];
    let mut iov = libc::iovec {
        iov_base: tag.as_mut_ptr().cast(),
        iov_len: tag.len(),
    };
    // SAFETY: all-zero msghdr is valid with null addresses and zero counts;
    // the writable iovec and control ranges are installed immediately below.
    let mut message: libc::msghdr = unsafe { std::mem::zeroed() };
    message.msg_iov = &mut iov;
    message.msg_iovlen = 1;
    message.msg_control = ancillary.as_mut_ptr().cast();
    message.msg_controllen = std::mem::size_of_val(&ancillary) as _;
    // SAFETY: socket is borrowed live, the header points to one initialized
    // iovec with eight writable bytes and aligned writable control storage.
    // All ranges and the header outlive this synchronous call; recvmsg retains
    // no pointers. No Rust aliases access the ranges during the call.
    let count = unsafe { libc::recvmsg(socket.as_raw_fd(), &mut message, 0) };
    if count < 0 {
        return Err(io::Error::last_os_error());
    }

    let mut descriptors = Vec::new();
    let mut unexpected = false;
    let mut rights_messages = 0;
    // SAFETY: recvmsg initialized this header and its control range. FIRSTHDR
    // returns null for insufficient bytes and otherwise a header in that range.
    let mut control = unsafe { libc::CMSG_FIRSTHDR(&message) };
    while !control.is_null() {
        // SAFETY: FIRSTHDR/NXTHDR returned this aligned complete header within
        // the initialized control range; we only read it while storage is live.
        let header = unsafe { &*control };
        // SAFETY: CMSG_LEN performs size arithmetic only, and zero fits c_uint.
        let prefix = unsafe { libc::CMSG_LEN(0) } as usize;
        let offset = control as usize - message.msg_control as usize;
        let length = header.cmsg_len as usize;
        if length < prefix
            || offset
                .checked_add(length)
                .is_none_or(|end| end > message.msg_controllen as usize)
        {
            return Err(io::Error::other("invalid kernel ancillary length"));
        }
        if header.cmsg_level == libc::SOL_SOCKET && header.cmsg_type == libc::SCM_RIGHTS {
            rights_messages += 1;
            let bytes = length - prefix;
            unexpected |= !bytes.is_multiple_of(std::mem::size_of::<RawFd>());
            for index in 0..bytes / std::mem::size_of::<RawFd>() {
                // SAFETY: the checked payload range lies entirely inside this
                // recvmsg-initialized control message. Each full RawFd is read
                // unaligned, then adopted exactly once. SCM_RIGHTS installs
                // distinct fresh descriptors in this process, even on truncation.
                let fd = unsafe {
                    let data = libc::CMSG_DATA(control).cast::<RawFd>();
                    OwnedFd::from_raw_fd(std::ptr::read_unaligned(data.add(index)))
                };
                descriptors.push(fd);
            }
        } else {
            unexpected = true;
        }
        // SAFETY: current is a complete header in the initialized control range.
        // NXTHDR bounds-checks its successor against this same live msghdr.
        control = unsafe { libc::CMSG_NXTHDR(&message, control) };
    }
    for fd in &descriptors {
        nix::fcntl::fcntl(
            fd,
            nix::fcntl::FcntlArg::F_SETFD(nix::fcntl::FdFlag::FD_CLOEXEC),
        )?;
    }
    if unexpected
        || rights_messages != 1
        || message.msg_flags & (libc::MSG_TRUNC | libc::MSG_CTRUNC) != 0
        || count != 8
        || tag != request.to_be_bytes()
        || descriptors.len() != 3
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "invalid interpolation descriptor frame",
        ));
    }
    Ok(descriptors)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fd_count() -> usize {
        (0..4096)
            .filter(|fd| {
                // SAFETY: F_GETFD is read-only; EBADF is expected for unused slots.
                (unsafe { libc::fcntl(*fd, libc::F_GETFD) }) >= 0
            })
            .count()
    }
    #[test]
    fn descriptor_frames_bind_the_request_and_close_all_received_handles_on_refusal() {
        let root = tempfile::tempdir().unwrap();
        let file = std::fs::File::create(root.path().join("fixture")).unwrap();
        let (sender, receiver) = UnixDatagram::pair().unwrap();
        let baseline = fd_count();
        for (request, count) in [(4, 3), (5, 2), (5, 200)] {
            let tag = u64::to_be_bytes(request);
            sendmsg::<()>(
                sender.as_raw_fd(),
                &[IoSlice::new(&tag)],
                &[ControlMessage::ScmRights(&vec![file.as_raw_fd(); count])],
                MsgFlags::empty(),
                None,
            )
            .unwrap();
            assert!(receive(&receiver, 5).is_err());
            assert_eq!(
                fd_count(),
                baseline,
                "malformed/truncated frame must not leak installed fds"
            );
        }
        sendmsg::<()>(
            sender.as_raw_fd(),
            &[IoSlice::new(&[0; 9])],
            &[ControlMessage::ScmRights(&[file.as_raw_fd(); 3])],
            MsgFlags::empty(),
            None,
        )
        .unwrap();
        assert!(
            receive(&receiver, 5).is_err(),
            "truncated payload is refused and its rights closed"
        );
        assert_eq!(fd_count(), baseline);
        send(&sender, 6, &[file.as_raw_fd(); 3]).unwrap();
        let adopted = receive(&receiver, 6).unwrap();
        assert_eq!(adopted.len(), 3);
        for fd in &adopted {
            assert!(nix::fcntl::FdFlag::from_bits_truncate(
                nix::fcntl::fcntl(fd, nix::fcntl::FcntlArg::F_GETFD).unwrap()
            )
            .contains(nix::fcntl::FdFlag::FD_CLOEXEC));
        }
        drop(adopted);
        assert_eq!(fd_count(), baseline);
    }
}
