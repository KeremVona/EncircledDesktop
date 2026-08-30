use std::sync::Mutex;
use sysinfo::{ProcessRefreshKind, RefreshKind, System, UpdateKind};

static SYSTEM_INSTANCE: Mutex<Option<System>> = Mutex::new(None);

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
