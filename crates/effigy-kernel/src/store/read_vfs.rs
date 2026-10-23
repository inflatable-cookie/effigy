//! SQLite's ordinary read-only connection can create an empty WAL. This
//! process-lifetime Unix VFS delegates all locking and I/O to SQLite's own
//! Unix VFS, but never permits a file open for writing or creation/deletion.
use super::StoreError;
use rusqlite::{ffi, Connection, OpenFlags};
use std::sync::OnceLock;

const NAME: &str = "effigy-read-only";
static REGISTERED: OnceLock<Result<(), i32>> = OnceLock::new();

pub(super) fn open(uri: &str, flags: OpenFlags) -> Result<Connection, StoreError> {
    let registered = REGISTERED.get_or_init(|| {
        // SAFETY: static NUL-terminated name; SQLite initializes its built-in
        // VFS registry before returning this process-lifetime pointer.
        let original = unsafe { ffi::sqlite3_vfs_find(c"unix".as_ptr()) };
        if original.is_null() {
            return Err(ffi::SQLITE_NOTFOUND);
        }
        // SAFETY: the built-in VFS is initialized and process-lived. SQLite
        // documents every field as immutable after registration except pNext,
        // which is deliberately never read. Copy the exact native callbacks
        // and application data; keep their original file allocation size.
        let wrapper = Box::new(unsafe {
            ffi::sqlite3_vfs {
                iVersion: (*original).iVersion,
                szOsFile: (*original).szOsFile,
                mxPathname: (*original).mxPathname,
                pAppData: (*original).pAppData,
                xAccess: (*original).xAccess,
                xFullPathname: (*original).xFullPathname,
                xDlOpen: (*original).xDlOpen,
                xDlError: (*original).xDlError,
                xDlSym: (*original).xDlSym,
                xDlClose: (*original).xDlClose,
                xRandomness: (*original).xRandomness,
                xSleep: (*original).xSleep,
                xCurrentTime: (*original).xCurrentTime,
                xGetLastError: (*original).xGetLastError,
                xCurrentTimeInt64: (*original).xCurrentTimeInt64,
                xSetSystemCall: (*original).xSetSystemCall,
                xGetSystemCall: (*original).xGetSystemCall,
                xNextSystemCall: (*original).xNextSystemCall,
                zName: c"effigy-read-only".as_ptr(),
                pNext: std::ptr::null_mut(),
                xOpen: Some(open_read_only),
                xDelete: Some(refuse_delete),
            }
        });
        // SQLite retains this pointer. A successful registration deliberately
        // owns one process-lifetime allocation, never one allocation per read.
        let raw = Box::into_raw(wrapper);
        // SAFETY: initialized, uniquely owned VFS; register without changing
        // the process default. OnceLock serializes registration.
        let rc = unsafe { ffi::sqlite3_vfs_register(raw, 0) };
        if rc == ffi::SQLITE_OK {
            Ok(())
        } else {
            // SAFETY: registration failed, so SQLite retained no ownership.
            drop(unsafe { Box::from_raw(raw) });
            Err(rc)
        }
    });
    if let Err(code) = registered {
        return Err(StoreError::Sqlite {
            code: *code,
            message: "read-only VFS unavailable".into(),
        });
    }
    Connection::open_with_flags_and_vfs(uri, flags, NAME).map_err(StoreError::from)
}

unsafe extern "C" fn open_read_only(
    _vfs: *mut ffi::sqlite3_vfs,
    name: ffi::sqlite3_filename,
    file: *mut ffi::sqlite3_file,
    flags: i32,
    out: *mut i32,
) -> i32 {
    // SAFETY: SQLite supplies at least the native szOsFile writable extent,
    // including this properly aligned sqlite3_file header. The xOpen contract
    // requires a valid or null pMethods even on a refused open.
    unsafe {
        (*file).pMethods = std::ptr::null();
    }
    // No temporary or unnamed files are needed for a pure store read.
    if name.is_null() {
        return ffi::SQLITE_READONLY;
    }
    // SAFETY: same static built-in VFS selected at registration. SQLite owns
    // all callback arguments and their documented lifetimes.
    let original = unsafe { ffi::sqlite3_vfs_find(c"unix".as_ptr()) };
    if original.is_null() {
        return ffi::SQLITE_NOTFOUND;
    }
    // SAFETY: non-null registered VFS.
    let Some(open) = (unsafe { (*original).xOpen }) else {
        return ffi::SQLITE_NOTFOUND;
    };
    let flags = (flags
        & !(ffi::SQLITE_OPEN_READWRITE
            | ffi::SQLITE_OPEN_CREATE
            | ffi::SQLITE_OPEN_DELETEONCLOSE
            | ffi::SQLITE_OPEN_EXCLUSIVE))
        | ffi::SQLITE_OPEN_READONLY;
    // SAFETY: preserve SQLite's file allocation, filename URI and output
    // pointer, delegating to its native implementation with narrower flags.
    unsafe { open(original, name, file, flags, out) }
}

unsafe extern "C" fn refuse_delete(
    _vfs: *mut ffi::sqlite3_vfs,
    _name: *const std::ffi::c_char,
    _sync: i32,
) -> i32 {
    ffi::SQLITE_READONLY
}
