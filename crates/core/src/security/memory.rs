use std::sync::atomic::{AtomicBool, Ordering};
use tracing::{debug, info};
use zeroize::Zeroize;

static MEMORY_LOCKED: AtomicBool = AtomicBool::new(false);
static DUMPABLE_DISABLED: AtomicBool = AtomicBool::new(false);

/// Revokes non-root ptrace attachment and blocks `/proc/<pid>/mem` inspection
/// by other applications running under the same user UID.
pub fn apply_process_memory_shield() {
    #[cfg(target_os = "linux")]
    {
        if !DUMPABLE_DISABLED.swap(true, Ordering::SeqCst) {
            unsafe {
                let res = libc::prctl(libc::PR_SET_DUMPABLE, 0);
                if res == 0 {
                    debug!("Process memory shield active: PR_SET_DUMPABLE=0 applied");
                } else {
                    debug!(
                        "Failed to apply PR_SET_DUMPABLE=0: errno={}",
                        std::io::Error::last_os_error()
                    );
                }
            }
        }
    }
}

/// Locks current and future process memory pages into RAM using `mlockall`
/// to prevent paging sensitive clipboard data, encryption keys, and tokens
/// to unencrypted swap or zram partitions.
pub fn lock_process_memory() {
    #[cfg(target_os = "linux")]
    {
        if !MEMORY_LOCKED.swap(true, Ordering::SeqCst) {
            unsafe {
                // MCL_CURRENT = 1, MCL_FUTURE = 2
                let res = libc::mlockall(libc::MCL_CURRENT | libc::MCL_FUTURE);
                if res == 0 {
                    info!("Process memory locked via mlockall (swap paging disabled)");
                } else {
                    // Graceful fallback if RLIMIT_MEMLOCK is constrained
                    let err = std::io::Error::last_os_error();
                    debug!(
                        "mlockall returned {}: continuing with standard virtual memory",
                        err
                    );
                }
            }
        }
    }
}

/// Marks a memory buffer with `mlock` and `MADV_DONTDUMP` to prevent paging to swap/zram
/// and explicitly exclude it from process crash dumps and `/proc/<pid>/coredump_filter`.
pub fn protect_sensitive_buffer(slice: &[u8]) {
    #[cfg(target_os = "linux")]
    {
        if !slice.is_empty() {
            unsafe {
                let ptr = slice.as_ptr() as *mut libc::c_void;
                let len = slice.len();
                let _ = libc::mlock(ptr, len);
                let _ = libc::madvise(ptr, len, libc::MADV_DONTDUMP);
            }
        }
    }
}

/// Securely zeroizes memory and advises kernel not to dump
pub fn wipe_and_protect(buffer: &mut [u8]) {
    protect_sensitive_buffer(buffer);
    buffer.zeroize();
}
