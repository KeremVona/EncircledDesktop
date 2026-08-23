use sysinfo::{ProcessRefreshKind, RefreshKind, System};

/// Checks if Hearts of Iron IV (hoi4.exe) is currently running and if it was launched with the `-debug` flag.
pub fn check_hoi4_process() -> (bool, bool) {
    let mut sys = System::new_with_specifics(
        RefreshKind::new().with_processes(ProcessRefreshKind::everything()),
    );
    sys.refresh_processes();
    let mut is_running = false;
    let mut has_debug = false;

    for process in sys.processes().values() {
        let name = process.name().to_lowercase();

        // 1. Exclude the companion app itself, helper daemons, or editors
        if name.contains("hoi4-save-monitor")
            || name.contains("hoi4_save_monitor")
            || name.contains("encircled")
            || name.contains("cargo")
            || name.contains("rust")
            || name.contains("code")
        {
            continue;
        }

        // 2. Check for exact Hearts of Iron IV executable
        let is_hoi4 = if name == "hoi4.exe" || name == "hoi4" {
            true
        } else if let Some(exe_path) = process.exe() {
            if let Some(file_name) = exe_path.file_name().and_then(|n| n.to_str()) {
                let file_name_lower = file_name.to_lowercase();
                file_name_lower == "hoi4.exe" || file_name_lower == "hoi4"
            } else {
                false
            }
        } else {
            false
        };

        if is_hoi4 {
            is_running = true;
            for arg in process.cmd() {
                let arg_str = arg.to_lowercase();
                if arg_str == "-debug" || arg_str == "--debug" || arg_str.starts_with("-debug=") {
                    has_debug = true;
                }
            }
        }
    }

    (is_running, has_debug)
}
