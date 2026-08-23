import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./App.css";

interface ProcessStatusPayload {
  is_running: boolean;
  has_debug_flag: boolean;
}

interface TelemetryPayload {
  file_name: string;
  file_hash: string;
  status: string;
  verified: boolean;
  has_debug_flag: boolean;
  timestamp: string;
}

function App() {
  const [savePath, setSavePath] = useState<string>(() => {
    return localStorage.getItem("encircled_custom_save_path") || "";
  });
  const [defaultPath, setDefaultPath] = useState<string>("");
  const [sessionId, setSessionId] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [status, setStatus] = useState("Idle · Awaiting Session");
  const [isWatching, setIsWatching] = useState(false);
  const [isHoi4Running, setIsHoi4Running] = useState(false);
  const [hasDebugFlag, setHasDebugFlag] = useState(false);
  const [telemetryLogs, setTelemetryLogs] = useState<TelemetryPayload[]>([]);
  const [copiedPath, setCopiedPath] = useState(false);

  useEffect(() => {
    let unlistenProcess: () => void;
    let unlistenDeepLink: () => void;
    let unlistenTelemetry: () => void;

    let intervalId: any = null;

    async function initSystem() {
      // 1. Fetch default detected save directory
      try {
        const detected = await invoke<string>("get_default_save_path_cmd");
        if (detected) {
          setDefaultPath(detected);
        }
      } catch (err) {
        console.warn("Could not determine default save path:", err);
      }

      // 2. Initial & Periodic 2-Second HOI4 Process Polling
      const pollHoi4Status = async () => {
        try {
          const current = await invoke<ProcessStatusPayload>("get_process_status");
          if (current) {
            setIsHoi4Running(current.is_running);
            setHasDebugFlag(current.has_debug_flag);
          }
        } catch (err) {
          console.error("Error polling process status:", err);
        }
      };

      pollHoi4Status();
      intervalId = setInterval(pollHoi4Status, 2000);

      // 3. Listen for push HOI4 process status events
      unlistenProcess = await listen<ProcessStatusPayload>("process-status", (event) => {
        setIsHoi4Running(event.payload.is_running);
        setHasDebugFlag(event.payload.has_debug_flag);
      });

      // 4. Listen for browser deep link invitations (encircled://lobby/<id>?key=<key>)
      unlistenDeepLink = await listen<string>("deep-link-received", async (event) => {
        try {
          const raw = event.payload || "";
          let room = "";
          let key = "";
          try {
            const url = new URL(raw);
            if (url.searchParams.get("key")) {
              key = url.searchParams.get("key") || "";
            }
            if (url.host === "lobby" || url.hostname === "lobby") {
              room = url.pathname.replace(/^\/+/, "");
            } else if (url.pathname.includes("/lobby/")) {
              const parts = url.pathname.split("/lobby/");
              room = parts[1]?.split("/")[0] || "";
            } else {
              room = url.pathname.replace(/^\/+/, "");
            }
          } catch {
            const match = raw.match(/lobby\/([a-zA-Z0-9-]+)/i);
            if (match) {
              room = match[1];
            }
            const keyMatch = raw.match(/[?&]key=([a-zA-Z0-9]+)/i);
            if (keyMatch) {
              key = keyMatch[1];
            }
          }

          if (room) {
            setSessionId(room);
            if (key) setApiKey(key);
            setStatus(`Linked to Lobby ${room.slice(0, 8)}... Initializing watcher...`);
            
            const activePath = localStorage.getItem("encircled_custom_save_path") || null;
            try {
              await invoke("start_watching", {
                sessionId: room,
                apiKey: key || null,
                path: activePath,
              });
              setIsWatching(true);
              setStatus("Watching & Live Telemetry Linked ✓");
            } catch (err) {
              console.error("Watcher init error:", err);
              setStatus("Error starting watcher: " + err);
            }
          }
        } catch (e) {
          console.error("Invalid deep link URL", e);
        }
      });

      // 5. Listen for save telemetry upload events
      unlistenTelemetry = await listen<TelemetryPayload>("telemetry-event", (event) => {
        setTelemetryLogs((prev) => [event.payload, ...prev.slice(0, 19)]);
        setStatus(`Uploaded & Verified ${event.payload.file_name} ✓`);
      });
    }

    initSystem();

    return () => {
      if (intervalId) clearInterval(intervalId);
      if (unlistenProcess) unlistenProcess();
      if (unlistenDeepLink) unlistenDeepLink();
      if (unlistenTelemetry) unlistenTelemetry();
    };
  }, []);

  // Handler to visually browse and pick a save directory
  async function handleBrowseFolder() {
    try {
      const initial = savePath || defaultPath || null;
      const selected = await invoke<string | null>("select_save_folder", {
        initialPath: initial,
      });

      if (selected) {
        setSavePath(selected);
        localStorage.setItem("encircled_custom_save_path", selected);
        setStatus("Custom save folder configured ✓");
      }
    } catch (err) {
      console.error("Failed to select folder:", err);
      setStatus("Error choosing folder: " + err);
    }
  }

  function handleResetDefault() {
    setSavePath("");
    localStorage.removeItem("encircled_custom_save_path");
    setStatus("Reset to auto-detected default directory");
  }

  async function handleStartWatching() {
    if (!sessionId.trim()) {
      setStatus("Please enter a Lobby / Session ID");
      return;
    }

    try {
      setStatus("Starting filesystem watcher...");
      const effectivePath = savePath.trim() || null;
      await invoke("start_watching", {
        sessionId: sessionId.trim(),
        apiKey: apiKey.trim() || null,
        path: effectivePath,
      });
      setIsWatching(true);
      setStatus("Watching & Live Telemetry Active ✓");
    } catch (e) {
      console.error(e);
      setStatus("Watcher Error: " + e);
    }
  }

  const effectiveDisplayPath = savePath.trim() || defaultPath || "Detecting standard Paradox save directory...";
  const isCustomActive = Boolean(savePath.trim());

  function handleCopyPath() {
    if (effectiveDisplayPath) {
      navigator.clipboard.writeText(effectiveDisplayPath);
      setCopiedPath(true);
      setTimeout(() => setCopiedPath(false), 2000);
    }
  }

  return (
    <div className="container">
      {/* Header HUD */}
      <header className="app-header">
        <div className="header-brand">
          <span className="radar-icon">📡</span>
          <div>
            <h1>ENCIRCLED</h1>
            <span className="app-subtitle">Desktop Companion · Save Watcher</span>
          </div>
        </div>
        <div className={`status-pill ${isWatching ? "active" : "idle"}`}>
          <span className="pulse-dot" />
          <span>{isWatching ? "WATCHING" : "STANDBY"}</span>
        </div>
      </header>

      {/* Main Grid Card */}
      <main className="hud-card">
        {/* 1. Session Connection Section */}
        <section className="hud-section">
          <div className="section-title">
            <span className="section-num">01</span>
            <span>MATCH LOBBY LINK</span>
          </div>

          <div className="form-grid">
            <div className="input-group">
              <label>Lobby ID / Session Key:</label>
              <input
                type="text"
                value={sessionId}
                onChange={(e) => setSessionId(e.target.value)}
                placeholder="e.g. 039bd64b-5dd6-4d78-b201..."
                className="font-mono"
              />
            </div>
          </div>
        </section>

        {/* 2. Visual Save Folder Picker Section */}
        <section className="hud-section">
          <div className="section-title-row">
            <div className="section-title">
              <span className="section-num">02</span>
              <span>HOI4 SAVE GAMES FOLDER</span>
            </div>
            <span className={`badge ${isCustomActive ? "badge-custom" : "badge-auto"}`}>
              {isCustomActive ? "CUSTOM PATH" : "AUTO-DETECTED"}
            </span>
          </div>

          <div className="folder-picker-box">
            <div className="path-display" onClick={handleCopyPath} title="Click to copy path">
              <span className="folder-icon">📂</span>
              <span className="path-text font-mono">{effectiveDisplayPath}</span>
              <button
                type="button"
                className="copy-btn"
                onClick={(e) => {
                  e.stopPropagation();
                  handleCopyPath();
                }}
                title="Copy Path"
              >
                {copiedPath ? "✓" : "📋"}
              </button>
            </div>

            <div className="folder-actions">
              <button
                type="button"
                onClick={handleBrowseFolder}
                className="btn-secondary"
              >
                📁 Choose Folder...
              </button>
              {isCustomActive && (
                <button
                  type="button"
                  onClick={handleResetDefault}
                  className="btn-ghost"
                  title="Reset to default Paradox Documents directory"
                >
                  ↺ Reset to Default
                </button>
              )}
            </div>
          </div>
          <p className="field-hint">
            The watcher automatically monitors this folder for new monthly and manual autosaves.
          </p>
        </section>

        {/* 3. Action Control */}
        <div className="action-row">
          <button
            type="button"
            onClick={handleStartWatching}
            className={`btn-primary ${isWatching ? "btn-watching" : ""}`}
          >
            {isWatching ? "✓ Watcher Active (Re-Engage)" : "⚡ Start Save Watcher"}
          </button>
        </div>

        {/* 4. Diagnostics & Live Process HUD */}
        <section className="diagnostics-box">
          <div className="diag-row">
            <span className="diag-label">HOI4 Process:</span>
            {isHoi4Running ? (
              <span className="diag-val text-green">Running 🟢</span>
            ) : (
              <span className="diag-val text-muted">Not Running ⚪</span>
            )}
          </div>

          <div className="diag-row">
            <span className="diag-label">Anti-Cheat Integrity:</span>
            {hasDebugFlag ? (
              <span className="diag-val text-warn">⚠️ -debug Flag Active</span>
            ) : (
              <span className="diag-val text-green">Clean Runtime ✓</span>
            )}
          </div>

          <div className="diag-row">
            <span className="diag-label">Companion Status:</span>
            <span className="diag-val font-mono">{status}</span>
          </div>
        </section>

        {/* 5. Live Telemetry Stream */}
        {telemetryLogs.length > 0 && (
          <section className="telemetry-log-section">
            <div className="telemetry-header">
              <span>RECENT SAVE TELEMETRY EVENTS</span>
              <span className="badge-count">{telemetryLogs.length}</span>
            </div>

            <div className="telemetry-list">
              {telemetryLogs.map((log, index) => (
                <div key={index} className="telemetry-item">
                  <div className="telemetry-item-top">
                    <strong className="telemetry-name">{log.file_name}</strong>
                    <span className={`telemetry-badge ${log.verified ? "verified" : "uploaded"}`}>
                      {log.verified ? "Verified ✓" : "Uploaded"}
                    </span>
                  </div>
                  <div className="telemetry-item-bottom">
                    <span className="telemetry-hash">SHA: {log.file_hash.substring(0, 16)}...</span>
                    <span className="telemetry-time">{log.timestamp || "Just now"}</span>
                  </div>
                </div>
              ))}
            </div>
          </section>
        )}
      </main>
    </div>
  );
}

export default App;
