use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use sysinfo::{ProcessRefreshKind, RefreshKind, System, UpdateKind};

static IS_HOI4_RUNNING: AtomicBool = AtomicBool::new(false);
static HAS_DEBUG_FLAG: AtomicBool = AtomicBool::new(false);
static SYSTEM_INSTANCE: Mutex<Option<System>> = Mutex::new(None);

pub fn get_cached_hoi4_process_status() -> (bool, bool) {
    (
        IS_HOI4_RUNNING.load(Ordering::Relaxed),
        HAS_DEBUG_FLAG.load(Ordering::Relaxed),
    )
}

pub fn update_cached_hoi4_process_status(is_running: bool, has_debug: bool) {
    IS_HOI4_RUNNING.store(is_running, Ordering::Relaxed);
    HAS_DEBUG_FLAG.store(has_debug, Ordering::Relaxed);
}

pub fn is_hoi4_process_name(name: &str) -> bool {
    let trimmed = name.trim().to_lowercase();
    trimmed == "hoi4.exe" || trimmed == "hoi4"
}

pub fn parse_has_debug_flag<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    for arg in args {
        let trimmed = arg.as_ref().trim().to_lowercase();
        if trimmed == "-debug" || trimmed == "--debug" {
            return true;
        }
    }
    false
}

/// Checks if Hearts of Iron IV (hoi4.exe) is currently running and if it was launched with the `-debug` flag.
/// Uses targeted minimal process refresh: only refreshes process names (~1ms), querying command-lines
/// exclusively for matched HOI4 processes. Results are cached atomically for zero-latency retrieval.
pub fn check_hoi4_process() -> (bool, bool) {
    let mut lock = SYSTEM_INSTANCE.lock().unwrap();
    let sys = lock.get_or_insert_with(|| {
        System::new_with_specifics(
            RefreshKind::new().with_processes(ProcessRefreshKind::new()),
        )
    });

    // Lightning-fast process list refresh without querying memory or cmdline for 300+ processes
    sys.refresh_processes_specifics(ProcessRefreshKind::new());

    let mut is_running = false;
    let mut has_debug = false;
    let mut matched_pids = Vec::new();

    for (pid, process) in sys.processes() {
        if is_hoi4_process_name(process.name()) {
            is_running = true;
            matched_pids.push(*pid);
        }
    }

    // Only for HOI4 process(es), query command line to inspect -debug flag
    for pid in matched_pids {
        sys.refresh_process_specifics(
            pid,
            ProcessRefreshKind::new().with_cmd(UpdateKind::Always),
        );
        if let Some(proc) = sys.process(pid) {
            if parse_has_debug_flag(proc.cmd()) {
                has_debug = true;
            }
        }
    }

    update_cached_hoi4_process_status(is_running, has_debug);
    (is_running, has_debug)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_hoi4_process_name() {
        assert!(is_hoi4_process_name("hoi4.exe"));
        assert!(is_hoi4_process_name("HOI4.EXE"));
        assert!(is_hoi4_process_name("hoi4"));
        assert!(is_hoi4_process_name("  hoi4.exe  "));

        // Reject partial matches or tool names
        assert!(!is_hoi4_process_name("hoi4_mod_tool.exe"));
        assert!(!is_hoi4_process_name("hoi4-save-monitor.exe"));
        assert!(!is_hoi4_process_name("encircled.exe"));
        assert!(!is_hoi4_process_name("launcher.exe"));
    }

    #[test]
    fn test_parse_has_debug_flag() {
        assert!(parse_has_debug_flag(&["-debug"]));
        assert!(parse_has_debug_flag(&["--debug"]));
        assert!(parse_has_debug_flag(&["--other", "-debug", "--ui"]));
        assert!(parse_has_debug_flag(&["  -debug  "]));

        // Reject non-exact or substring matches
        assert!(!parse_has_debug_flag(&["-debugger"]));
        assert!(!parse_has_debug_flag(&["--crash-debug-reporter"]));
        assert!(!parse_has_debug_flag(&["debug"]));
        assert!(!parse_has_debug_flag(&[] as &[&str]));
    }
}


