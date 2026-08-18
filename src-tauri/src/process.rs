use sysinfo::System;

/// Checks if Hearts of Iron IV is currently running and if it was launched with the `-debug` flag.
pub fn check_hoi4_process() -> (bool, bool) {
    let mut sys = System::new_all();
    sys.refresh_all();
    let mut is_running = false;
    let mut has_debug = false;
    for process in sys.processes().values() {
        let name = process.name().to_lowercase();
        let exe = process.exe().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        if name == "hoi4.exe" || name == "hoi4" || name.contains("hoi4") || exe.contains("hoi4.exe") || exe.contains("hoi4") {
            is_running = true;
            for arg in process.cmd() {
                let arg_str = arg.to_lowercase();
                if arg_str.contains("-debug") || arg_str.contains("--debug") {
                    has_debug = true;
                }
            }
        }
    }
    (is_running, has_debug)
}
