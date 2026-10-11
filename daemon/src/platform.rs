//! Small cross-platform helpers so the daemon builds and runs on Windows too.
//!
//! Everything Unix-specific (signals, process groups, `sh`) is wrapped here and
//! given a Windows counterpart, so the rest of the code stays platform-agnostic.

use std::process::Command;

/// A shell invocation for `cmd`. `sh -c` on Unix, `cmd /C` on Windows.
#[cfg(unix)]
pub fn shell(cmd: &str) -> Command {
    let mut c = Command::new("sh");
    c.args(["-c", cmd]);
    c
}

#[cfg(windows)]
pub fn shell(cmd: &str) -> Command {
    let mut c = Command::new("cmd");
    c.args(["/C", cmd]);
    c
}

/// Puts the child in its own process group so the whole tree can be signalled.
#[cfg(unix)]
pub fn configure_process_group(cmd: &mut Command) {
    use std::os::unix::process::CommandExt;
    cmd.process_group(0);
}

#[cfg(windows)]
pub fn configure_process_group(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    // INFO: CREATE_NEW_PROCESS_GROUP lets us later signal the whole tree.
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    cmd.creation_flags(CREATE_NEW_PROCESS_GROUP);
}

/// Is a process with this pid alive?
#[cfg(unix)]
pub fn process_alive(pid: u32) -> bool {
    // INFO: `kill(pid, 0)` checks existence/permission without sending a signal.
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

#[cfg(windows)]
pub fn process_alive(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    // INFO: No signal 0 on Windows; open the process and read its exit code.
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return false;
        }
        let mut code: u32 = 0;
        let ok = GetExitCodeProcess(handle, &mut code);
        CloseHandle(handle);
        ok != 0 && code == STILL_ACTIVE as u32
    }
}

/// Terminates a process tree. `force` escalates the kill strength.
#[cfg(unix)]
pub fn terminate(pid: u32, force: bool) {
    let signal = if force { libc::SIGKILL } else { libc::SIGTERM };
    // INFO: Negative pid targets the whole process group.
    unsafe {
        libc::kill(-(pid as i32), signal);
    }
}

#[cfg(windows)]
pub fn terminate(pid: u32, force: bool) {
    // INFO: `taskkill /T` kills the whole tree. Without `/F` it asks politely
    //       first; with `/F` it force-kills (matching SIGTERM then SIGKILL).
    let mut args = vec!["/PID".to_string(), pid.to_string(), "/T".to_string()];
    if force {
        args.push("/F".to_string());
    }
    let _ = Command::new("taskkill").args(&args).status();
}
