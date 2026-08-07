import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./App.css";

interface ProcessStatusPayload {
  is_running: boolean;
  has_debug_flag: boolean;
}

interface TelemetryPayload {
  file_name: String;
  file_hash: String;
  status: String;
  verified: boolean;
  has_debug_flag: boolean;
  timestamp: String;
}

function App() {
  const [savePath, setSavePath] = useState("");
  const [sessionId, setSessionId] = useState("");
  const [status, setStatus] = useState("Idle");
  const [isHoi4Running, setIsHoi4Running] = useState(false);
  const [hasDebugFlag, setHasDebugFlag] = useState(false);
  const [telemetryLogs, setTelemetryLogs] = useState<TelemetryPayload[]>([]);

  useEffect(() => {
    let unlistenProcess: () => void;
    let unlistenDeepLink: () => void;
    let unlistenTelemetry: () => void;

    async function setupListeners() {
      unlistenProcess = await listen<ProcessStatusPayload>("process-status", (event) => {
        setIsHoi4Running(event.payload.is_running);
        setHasDebugFlag(event.payload.has_debug_flag);
      });

      unlistenDeepLink = await listen<string>("deep-link-received", (event) => {
        try {
          const url = new URL(event.payload);
          if (url.pathname.startsWith("//lobby/")) {
            const parts = url.pathname.split("/");
            const room = parts[parts.length - 1];
            if (room) {
              setSessionId(room);
              setStatus(`Joined lobby ${room}`);
            }
          }
        } catch (e) {
          console.error("Invalid deep link URL", e);
        }
      });

      unlistenTelemetry = await listen<TelemetryPayload>("telemetry-event", (event) => {
        setTelemetryLogs((prev) => [event.payload, ...prev]);
        setStatus(`Uploaded & Verified ${event.payload.file_name}`);
      });
    }

    setupListeners();

    return () => {
      if (unlistenProcess) unlistenProcess();
      if (unlistenDeepLink) unlistenDeepLink();
      if (unlistenTelemetry) unlistenTelemetry();
    };
  }, []);

  async function startWatching() {
    try {
      await invoke("start_watching", { sessionId, path: savePath || null });
      setStatus("Watching");
    } catch (e) {
      console.error(e);
      setStatus("Error: " + e);
    }
  }

  return (
    <div className="container">
      <h1>HOI4 Companion App</h1>
      <p>Phase 1 - Minimal UI</p>

      <div className="form-group">
        <label>Session ID:</label>
        <input
          value={sessionId}
          onChange={(e) => setSessionId(e.target.value)}
          placeholder="e.g. 12345"
        />
      </div>
      
      <div className="form-group">
        <label>Manual Save Path (Optional):</label>
        <input
          value={savePath}
          onChange={(e) => setSavePath(e.target.value)}
          placeholder="Leave blank for default"
        />
      </div>

      <button onClick={startWatching}>Start Watching</button>

      <div className="status">
        <strong>HOI4 Status:</strong>{" "}
        {isHoi4Running ? (
          <span style={{ color: "#4caf50" }}>Running 🟢</span>
        ) : (
          <span style={{ color: "#f44336" }}>Not Running 🔴</span>
        )}
        {hasDebugFlag && (
          <div style={{ color: "#ff9800", marginTop: "0.25rem", fontWeight: "bold" }}>
            ⚠️ WARNING: Process launched with -debug flag!
          </div>
        )}
      </div>

      <div className="status">
        <strong>App Status:</strong> {status}
      </div>

      {telemetryLogs.length > 0 && (
        <div className="status" style={{ marginTop: "1rem" }}>
          <strong>Live Telemetry HUD:</strong>
          <div style={{ maxHeight: "150px", overflowY: "auto", marginTop: "0.5rem", fontSize: "0.85rem" }}>
            {telemetryLogs.map((log, index) => (
              <div key={index} style={{ padding: "0.25rem 0", borderBottom: "1px solid #444" }}>
                <div><strong>{log.file_name}</strong></div>
                <div style={{ color: "#aaa" }}>Hash: {log.file_hash.substring(0, 16)}...</div>
                <div style={{ color: log.verified ? "#4caf50" : "#ff9800" }}>
                  {log.verified ? "Verified ✓" : "Upload Fallback"} | Debug Flag: {log.has_debug_flag ? "YES ⚠️" : "NO ✓"}
                </div>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

export default App;
