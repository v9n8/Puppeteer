//! Windows transport of the broker: a named pipe instead of a Unix socket.
//!
//! A pipe created with no security descriptor lets Everyone and the anonymous
//! account open it for reading (Microsoft Learn, "Named Pipe Security and Access
//! Rights"). The broker answers with tokens, so every instance gets a protected
//! DACL that grants access to the current user only. Remote clients are
//! rejected, and the first instance is created with `first_pipe_instance`, so a
//! pipe of the same name made earlier by someone else fails startup instead of
//! being joined.
//!
//! There is no peer check beyond the DACL, as on Unix (socket mode 0600 only).

use std::ffi::c_void;
use std::io;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::PathBuf;
use std::ptr::null_mut;

use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};

use super::{pipe_name, ConnCtx};

const TOKEN_QUERY: u32 = 0x0008;
/// `TOKEN_INFORMATION_CLASS::TokenUser`.
const TOKEN_USER: i32 = 1;
const SDDL_REVISION_1: u32 = 1;

/// `SECURITY_ATTRIBUTES` (minwinbase.h).
#[repr(C)]
struct SecurityAttributes {
    length: u32,
    security_descriptor: *mut c_void,
    inherit_handle: i32,
}

/// `TOKEN_USER` (winnt.h): one `SID_AND_ATTRIBUTES`.
#[repr(C)]
struct TokenUser {
    sid: *mut c_void,
    attributes: u32,
}

#[link(name = "kernel32")]
extern "system" {
    fn GetCurrentProcess() -> *mut c_void;
    fn ProcessIdToSessionId(process_id: u32, session_id: *mut u32) -> i32;
    fn LocalFree(mem: *mut c_void) -> *mut c_void;
}

#[link(name = "advapi32")]
extern "system" {
    fn OpenProcessToken(process: *mut c_void, access: u32, token: *mut *mut c_void) -> i32;
    fn GetTokenInformation(
        token: *mut c_void,
        class: i32,
        info: *mut c_void,
        len: u32,
        ret_len: *mut u32,
    ) -> i32;
    fn ConvertSidToStringSidW(sid: *mut c_void, out: *mut *mut u16) -> i32;
    fn ConvertStringSecurityDescriptorToSecurityDescriptorW(
        sddl: *const u16,
        revision: u32,
        sd: *mut *mut c_void,
        size: *mut u32,
    ) -> i32;
}

/// The current user's SID as a string (`S-1-5-21-…`).
fn current_user_sid() -> io::Result<String> {
    let mut raw = null_mut();
    // SAFETY: the pseudo-handle of GetCurrentProcess needs no closing; `raw`
    // receives a new token handle, owned (and closed) by `token` below.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw) } == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `raw` is a valid handle that nothing else owns.
    let token = unsafe { OwnedHandle::from_raw_handle(raw) };

    let mut len = 0u32;
    // Size query: fails with ERROR_INSUFFICIENT_BUFFER and sets `len`.
    // SAFETY: a null buffer of length 0 is allowed for the size query.
    unsafe { GetTokenInformation(token.as_raw_handle(), TOKEN_USER, null_mut(), 0, &mut len) };
    if len == 0 {
        return Err(io::Error::last_os_error());
    }
    // u64 cells keep the buffer aligned for the pointer inside TOKEN_USER.
    let mut buf = vec![0u64; (len as usize).div_ceil(8)];
    // SAFETY: `buf` holds at least `len` bytes.
    let ok = unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TOKEN_USER,
            buf.as_mut_ptr().cast(),
            len,
            &mut len,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: on success `buf` starts with a TOKEN_USER whose SID points into `buf`.
    let user = unsafe { &*(buf.as_ptr() as *const TokenUser) };

    let mut wide = null_mut();
    // SAFETY: `user.sid` is a valid SID; `wide` receives a LocalAlloc'd string.
    if unsafe { ConvertSidToStringSidW(user.sid, &mut wide) } == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `wide` is NUL-terminated; it is freed right after being copied.
    let sid = unsafe {
        let len = (0..).take_while(|&i| *wide.add(i) != 0).count();
        let s = String::from_utf16_lossy(std::slice::from_raw_parts(wide, len));
        LocalFree(wide.cast());
        s
    };
    Ok(sid)
}

fn current_session_id() -> io::Result<u32> {
    let mut session = 0u32;
    // SAFETY: `session` is a valid out pointer.
    if unsafe { ProcessIdToSessionId(std::process::id(), &mut session) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(session)
}

/// A security descriptor whose protected DACL grants the current user, and
/// no one else, full access.
struct OwnerOnly(*mut c_void);

// SAFETY: the descriptor is only read after creation and freed once, in Drop.
unsafe impl Send for OwnerOnly {}
unsafe impl Sync for OwnerOnly {}

impl OwnerOnly {
    fn new(user_sid: &str) -> io::Result<Self> {
        // `P`: inherit nothing; one ACE: GENERIC_ALL for this user.
        let sddl: Vec<u16> = format!("D:P(A;;GA;;;{user_sid})")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let mut sd = null_mut();
        // SAFETY: `sddl` is NUL-terminated; `sd` receives a LocalAlloc'd descriptor.
        let ok = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut sd,
                null_mut(),
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(sd))
    }

    /// Create one server instance of the pipe with this descriptor.
    fn create(&self, name: &str, first: bool) -> io::Result<NamedPipeServer> {
        let mut attrs = SecurityAttributes {
            length: std::mem::size_of::<SecurityAttributes>() as u32,
            security_descriptor: self.0,
            inherit_handle: 0,
        };
        // SAFETY: `attrs` is a valid SECURITY_ATTRIBUTES; its descriptor lives as
        // long as `self`, and the system copies it into the pipe.
        unsafe {
            ServerOptions::new()
                .first_pipe_instance(first)
                .reject_remote_clients(true)
                .create_with_security_attributes_raw(
                    name,
                    (&mut attrs as *mut SecurityAttributes).cast(),
                )
        }
    }
}

impl Drop for OwnerOnly {
    fn drop(&mut self) {
        // SAFETY: allocated by ConvertStringSecurityDescriptorToSecurityDescriptorW.
        unsafe { LocalFree(self.0) };
    }
}

/// Create the pipe and spawn the accept loop. Each connected instance is
/// handed to its own task and a fresh instance takes its place.
pub(super) fn listen(
    label: &str,
    ctx: ConnCtx,
) -> Result<(PathBuf, tokio::task::JoinHandle<()>), String> {
    let sid = current_user_sid().map_err(|e| e.to_string())?;
    let name = pipe_name(
        &sid,
        current_session_id().map_err(|e| e.to_string())?,
        label,
    );
    let security = OwnerOnly::new(&sid).map_err(|e| e.to_string())?;
    let first = security.create(&name, true).map_err(|e| e.to_string())?;

    let path = PathBuf::from(&name);
    let task = tokio::spawn(async move {
        let mut next = Some(first);
        loop {
            let server = match next.take() {
                Some(s) => s,
                None => match security.create(&name, false) {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!(target: "puppeteer::broker", "pipe create error: {e}");
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        continue;
                    }
                },
            };
            if let Err(e) = server.connect().await {
                // Transient connect error; drop this instance and open another.
                tracing::warn!(target: "puppeteer::broker", "accept error: {e}");
                continue;
            }
            // Open the next instance before serving this one, so a shim that
            // arrives meanwhile waits for the pipe instead of finding none.
            next = security
                .create(&name, false)
                .map_err(|e| tracing::warn!(target: "puppeteer::broker", "pipe create error: {e}"))
                .ok();
            let (read, write) = tokio::io::split(server);
            ctx.serve(read, write);
        }
    });
    Ok((path, task))
}
