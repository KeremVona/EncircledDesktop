import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

function App() {
  const [savePath, setSavePath] = useState("");
  const [sessionId, setSessionId] = useState("");
  const [status, setStatus] = useState("Idle");

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
        <strong>Status:</strong> {status}
      </div>
    </div>
  );
}

export default App;
