//! Starting the background updater without tying it to whoever started us.
//!
//! The Mod Tools linker reads our stdout/stderr through pipes and waits for end-of-file. A child that
//! inherited the write ends would keep those pipes open until it exited, stalling the link for the
//! whole update. `std::process::Command` always lets the child inherit every inheritable handle, so
//! the process is created directly with handle inheritance off: the child gets no handles at all and
//! null standard handles. It is detached from the console, and breaks away from the caller's job
//! object where the job allows it, so killing the linker's job does not stop an update mid-swap.

use std::io;
use std::path::Path;

#[cfg(windows)]
pub fn spawn_detached(exe: &Path, args: &str, cwd: &Path) -> io::Result<()> {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;

    #[repr(C)]
    struct StartupInfoW {
        cb: u32,
        reserved: *mut u16,
        desktop: *mut u16,
        title: *mut u16,
        x: u32,
        y: u32,
        x_size: u32,
        y_size: u32,
        x_count_chars: u32,
        y_count_chars: u32,
        fill_attribute: u32,
        flags: u32,
        show_window: u16,
        reserved2_len: u16,
        reserved2: *mut u8,
        std_input: *mut c_void,
        std_output: *mut c_void,
        std_error: *mut c_void,
    }

    #[repr(C)]
    struct ProcessInformation {
        process: *mut c_void,
        thread: *mut c_void,
        process_id: u32,
        thread_id: u32,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateProcessW(
            application: *const u16,
            command_line: *mut u16,
            process_attributes: *const c_void,
            thread_attributes: *const c_void,
            inherit_handles: i32,
            creation_flags: u32,
            environment: *const c_void,
            current_directory: *const u16,
            startup_info: *const StartupInfoW,
            process_information: *mut ProcessInformation,
        ) -> i32;
        fn CloseHandle(handle: *mut c_void) -> i32;
    }

    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const STARTF_USESTDHANDLES: u32 = 0x0000_0100;

    fn wide(text: &std::ffi::OsStr) -> Vec<u16> {
        text.encode_wide().chain(std::iter::once(0)).collect()
    }

    let application = wide(exe.as_os_str());
    let mut command_line: Vec<u16> = std::ffi::OsString::from(format!("\"{}\" {args}", exe.display()))
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let directory = wide(cwd.as_os_str());
    // Explicitly null standard handles; without STARTF_USESTDHANDLES the child could be handed our
    // handle values, which mean nothing (or something else) in a process that inherited nothing.
    let startup = StartupInfoW {
        cb: std::mem::size_of::<StartupInfoW>() as u32,
        reserved: std::ptr::null_mut(),
        desktop: std::ptr::null_mut(),
        title: std::ptr::null_mut(),
        x: 0,
        y: 0,
        x_size: 0,
        y_size: 0,
        x_count_chars: 0,
        y_count_chars: 0,
        fill_attribute: 0,
        flags: STARTF_USESTDHANDLES,
        show_window: 0,
        reserved2_len: 0,
        reserved2: std::ptr::null_mut(),
        std_input: std::ptr::null_mut(),
        std_output: std::ptr::null_mut(),
        std_error: std::ptr::null_mut(),
    };
    let base = DETACHED_PROCESS | CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP;
    let mut last_error = None;
    // A job without JOB_OBJECT_LIMIT_BREAKAWAY_OK refuses the breakaway; start inside it instead.
    for flags in [base | CREATE_BREAKAWAY_FROM_JOB, base] {
        let mut info = ProcessInformation {
            process: std::ptr::null_mut(),
            thread: std::ptr::null_mut(),
            process_id: 0,
            thread_id: 0,
        };
        // All strings are NUL-terminated and outlive the call; the command line buffer is mutable as
        // CreateProcessW requires; the environment is inherited (null).
        let ok = unsafe {
            CreateProcessW(
                application.as_ptr(),
                command_line.as_mut_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                0,
                flags,
                std::ptr::null(),
                directory.as_ptr(),
                &startup,
                &mut info,
            )
        };
        if ok != 0 {
            unsafe {
                CloseHandle(info.thread);
                CloseHandle(info.process);
            }
            return Ok(());
        }
        last_error = Some(io::Error::last_os_error());
    }
    Err(last_error.unwrap())
}

#[cfg(not(windows))]
pub fn spawn_detached(_exe: &Path, _args: &str, _cwd: &Path) -> io::Result<()> {
    Err(io::ErrorKind::Unsupported.into())
}
