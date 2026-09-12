use std::sync::Mutex;
use sysinfo::{ProcessRefreshKind, RefreshKind, System, UpdateKind};

static SYSTEM_INSTANCE: Mutex<Option<System>> = Mutex::new(None);

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
/// Reuses a single static System instance with minimal process refresh specifics to keep CPU usage near 0%.
pub fn check_hoi4_process() -> (bool, bool) {
    let mut lock = SYSTEM_INSTANCE.lock().unwrap();
    let sys = lock.get_or_insert_with(|| {
        System::new_with_specifics(
            RefreshKind::new().with_processes(
                ProcessRefreshKind::new()
                    .with_cmd(UpdateKind::OnlyIfNotSet)
                    .with_exe(UpdateKind::OnlyIfNotSet),
            ),
        )
    });

    sys.refresh_processes_specifics(
        ProcessRefreshKind::new()
            .with_cmd(UpdateKind::OnlyIfNotSet)
            .with_exe(UpdateKind::OnlyIfNotSet),
    );

    let mut is_running = false;
    let mut has_debug = false;

    for process in sys.processes().values() {
        let name = process.name();

        // 1. Check process name
        let mut is_hoi4 = is_hoi4_process_name(name);

        // 2. If not matched, verify executable file name if path is available
        if !is_hoi4 {
            if let Some(exe_path) = process.exe() {
                if let Some(file_name) = exe_path.file_name().and_then(|n| n.to_str()) {
                    is_hoi4 = is_hoi4_process_name(file_name);
                }
            }
        }

        if is_hoi4 {
            is_running = true;
            if parse_has_debug_flag(process.cmd()) {
                has_debug = true;
            }
        }
    }

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


