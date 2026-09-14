import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Header } from "./components/Header";
import { LobbyConnector } from "./components/LobbyConnector";
import { FolderSelector } from "./components/FolderSelector";
import { DiagnosticsHUD, StatusInfo } from "./components/DiagnosticsHUD";
import { ActivityTabs, TelemetryLog, QueuedUpload, UpdateInfo, UpdateProgress } from "./components/ActivityTabs";
import "./App.css";

interface ProcessStatusPayload {
  is_running: boolean;
  has_debug_flag: boolean;
}

interface UpdateCheckResponse {
  available: boolean;
  version?: string;
  current_version: string;
  body?: string;
}

function formatTimestamp(raw: string | number | undefined): string {
  if (!raw) return "Just now";
  const num = typeof raw === "number" ? raw : Number(raw);
  if (!isNaN(num) && num > 0) {
    const ms = num < 1e11 ? num * 1000 : num;
    const date = new Date(ms);
    return date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" });
  }
  return raw.toString();
}

const UUID_REGEX = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const API_KEY_REGEX = /^[a-zA-Z0-9_-]{1,128}$/;
const MAX_PATH_LENGTH = 512;
const MAX_URL_LENGTH = 2048;

function isValidUuid(id: string): boolean {
  return typeof id === "string" && UUID_REGEX.test(id.trim());
}

function isValidApiKey(key: string): boolean {
  return typeof key === "string" && API_KEY_REGEX.test(key.trim());
}

function validateAndSanitizePath(path: unknown): string {
  if (typeof path !== "string") return "";
  const trimmed = path.trim();
  if (!trimmed || trimmed.length > MAX_PATH_LENGTH) return "";

  // Reject null bytes, control characters, newlines, angle brackets, pipe, question mark, wildcard
  if (/[\x00-\x1f\0\r\n<>|?*"]/.test(trimmed)) return "";
  // Reject path traversal
  if (trimmed.includes("..")) return "";

  // Validate standard Windows absolute path (e.g. C:\...) or Unix absolute path (e.g. /home/...)
  const isWindowsPath = /^[a-zA-Z]:\\(?:[^\\/:*?"<>|\r\n]+\\)*[^\\/:*?"<>|\r\n]*$/.test(trimmed);
  const isUnixPath = /^\/(?:[^\/\0\r\n]+\/)*[^\/\0\r\n]*$/.test(trimmed);
  if (!isWindowsPath && !isUnixPath) return "";

  // Reject root filesystem paths
  if (/^[a-zA-Z]:\\?$/i.test(trimmed) || trimmed === "/") return "";

  return trimmed;
}

interface ParsedSessionInput {
  sessionId: string;
  apiKey?: string;
}

function parseAndValidateSessionInput(input: string): ParsedSessionInput | null {
  if (typeof input !== "string") return null;
  const trimmed = input.trim();
  if (!trimmed || trimmed.length > MAX_URL_LENGTH) return null;

  // Direct UUID check
  if (isValidUuid(trimmed)) {
    return { sessionId: trimmed.toLowerCase() };
  }

  // Check for delimiter formats: <uuid>:<key> or <uuid>#<key> or <uuid> <key>
  const delimiterMatch = trimmed.match(
    /^([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})[:#\s]([a-zA-Z0-9_-]{1,128})$/i
  );
  if (delimiterMatch) {
    return {
      sessionId: delimiterMatch[1].toLowerCase(),
      apiKey: delimiterMatch[2].trim(),
    };
  }

  try {
    let normalized = trimmed;
    if (normalized.startsWith("encircled://") || normalized.startsWith("encircled-desktop://")) {
      normalized = normalized.replace(/^encircled(-desktop)?:\/\//i, "http://dummy/");
    } else if (!/^https?:\/\//i.test(normalized)) {
      if (/^(?:www\.)?encircled(?:mp)?\.com/i.test(normalized)) {
        normalized = "https://" + normalized;
      } else if (/^localhost(?::\d+)?/i.test(normalized)) {
        normalized = "http://" + normalized;
      } else if (normalized.includes("?key=") || normalized.includes("&key=")) {
        normalized = "http://dummy/" + normalized.replace(/^\/+/, "");
      } else if (normalized.includes("/lobbies/") || normalized.includes("/lobby/")) {
        normalized = "http://dummy/" + normalized.replace(/^\/+/, "");
      } else {
        return null;
      }
    }

    const urlObj = new URL(normalized);

    let room = "";
    const pathname = urlObj.pathname;
    if (urlObj.hostname === "lobby" || urlObj.hostname === "lobbies") {
      room = pathname.replace(/^\/+/, "").split("/")[0] || "";
    } else if (pathname.includes("/lobbies/")) {
      room = pathname.split("/lobbies/")[1]?.split("/")[0] || "";
    } else if (pathname.includes("/lobby/")) {
      room = pathname.split("/lobby/")[1]?.split("/")[0] || "";
    } else {
      room = pathname.replace(/^\/+/, "").split("/")[0] || "";
    }

    room = room.trim().toLowerCase();
    if (!isValidUuid(room)) {
      return null;
    }

    let key: string | undefined = undefined;
    const rawKey = urlObj.searchParams.get("key") || urlObj.searchParams.get("companion_key");
    if (rawKey && isValidApiKey(rawKey)) {
      key = rawKey.trim();
    }

    return { sessionId: room, apiKey: key };
  } catch {
    return null;
  }
}

export function App() {
  const [savePath, setSavePath] = useState<string>(() => {
    const stored = localStorage.getItem("encircled_custom_save_path");
    const validated = validateAndSanitizePath(stored);
    if (stored && !validated) {
      localStorage.removeItem("encircled_custom_save_path");
    }
    return validated;
  });
  const [defaultPath, setDefaultPath] = useState<string>("");
  const [sessionId, setSessionId] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [statusInfo, setStatusInfo] = useState<StatusInfo>({
    text: "Idle · Awaiting Session Link",
    type: "idle",
  });
  const [isWatching, setIsWatching] = useState(false);
  const [isStartingWatcher, setIsStartingWatcher] = useState(false);
  const [isHoi4Running, setIsHoi4Running] = useState(false);
  const [hasDebugFlag, setHasDebugFlag] = useState(false);
  const [telemetryLogs, setTelemetryLogs] = useState<TelemetryLog[]>([]);
  const [hasMoreTelemetry, setHasMoreTelemetry] = useState(true);
  const [isLoadingMoreTelemetry, setIsLoadingMoreTelemetry] = useState(false);

  // Offline Queue State
  const [offlineQueue, setOfflineQueue] = useState<QueuedUpload[]>([]);
  const [isRetryingQueue, setIsRetryingQueue] = useState(false);

  // Settings State
  const [autostartEnabled, setAutostartEnabled] = useState(false);
  const [minimizeToTray, setMinimizeToTray] = useState(true);

  // App version state
  const [appVersion, setAppVersion] = useState<string>("1.0.0");

  // Theme state (dark / light)
  const [theme, setTheme] = useState<"dark" | "light">(() => {
    const saved = localStorage.getItem("encircled_theme");
    if (saved === "light" || saved === "dark") return saved;
    return window.matchMedia && window.matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark";
  });

  useEffect(() => {
    document.documentElement.setAttribute("data-theme", theme);
    localStorage.setItem("encircled_theme", theme);
  }, [theme]);

  function toggleTheme() {
    setTheme((prev) => (prev === "dark" ? "light" : "dark"));
  }

  // Deep-link Invitation Prompt State
  const [pendingInvite, setPendingInvite] = useState<{ sessionId: string; apiKey?: string } | null>(null);

  // Updater state
  const [updaterStatus, setUpdaterStatus] = useState<string>("v1.0.0");
  const [updateAvailable, setUpdateAvailable] = useState<UpdateInfo | null>(null);
  const [isCheckingUpdate, setIsCheckingUpdate] = useState(false);
  const [isInstallingUpdate, setIsInstallingUpdate] = useState(false);
  const [updateProgress, setUpdateProgress] = useState<UpdateProgress | null>(null);

  function setAppStatus(text: string, type: StatusInfo["type"] = "idle") {
    setStatusInfo({ text, type });
  }

  async function fetchOfflineQueue() {
    try {
      const queue = await invoke<QueuedUpload[]>("get_offline_queue");
      if (queue) {
        setOfflineQueue(queue);
      }
    } catch {
      // Ignored
    }
  }

  useEffect(() => {
    let unlistenProcess: () => void;
    let unlistenDeepLink: () => void;
    let unlistenTelemetry: () => void;
    let unlistenUpdateProgress: () => void;

    async function initSystem() {
      // Parallel hydration of independent startup data for instant responsiveness
      const [verRes, defaultPathRes, processRes, historyRes, queueRes, autoRes, minRes] =
        await Promise.allSettled([
          invoke<string>("get_app_version"),
          invoke<string>("get_default_save_path_cmd"),
          invoke<ProcessStatusPayload>("get_process_status"),
          invoke<TelemetryLog[]>("get_telemetry_history", { limit: 15, offset: 0 }),
          invoke<QueuedUpload[]>("get_offline_queue"),
          invoke<boolean>("get_autostart_status"),
          invoke<boolean>("get_minimize_to_tray_status"),
        ]);

      if (verRes.status === "fulfilled" && verRes.value) {
        setAppVersion(verRes.value);
        setUpdaterStatus(`v${verRes.value}`);
      }

      if (defaultPathRes.status === "fulfilled" && defaultPathRes.value) {
        const validated = validateAndSanitizePath(defaultPathRes.value);
        if (validated) {
          setDefaultPath(validated);
        }
      }

      if (processRes.status === "fulfilled" && processRes.value) {
        setIsHoi4Running(processRes.value.is_running);
        setHasDebugFlag(processRes.value.has_debug_flag);
      }

      if (historyRes.status === "fulfilled" && Array.isArray(historyRes.value)) {
        setTelemetryLogs(historyRes.value);
        setHasMoreTelemetry(historyRes.value.length >= 15);
      }

      if (queueRes.status === "fulfilled" && Array.isArray(queueRes.value)) {
        setOfflineQueue(queueRes.value);
      }

      if (autoRes.status === "fulfilled") {
        setAutostartEnabled(Boolean(autoRes.value));
      }

      if (minRes.status === "fulfilled") {
        setMinimizeToTray(Boolean(minRes.value));
      }

      // 5. Listen for push HOI4 process status events
      unlistenProcess = await listen<ProcessStatusPayload>("process-status", (event) => {
        setIsHoi4Running(event.payload.is_running);
        setHasDebugFlag(event.payload.has_debug_flag);
      });

      // 6. Listen for browser deep link invitations (encircled://lobby/<id>?key=<key>)
      unlistenDeepLink = await listen<string>("deep-link-received", (event) => {
        try {
          const raw = typeof event.payload === "string" ? event.payload : "";
          const parsed = parseAndValidateSessionInput(raw);
          if (!parsed) {
            console.warn("Received invalid or malformed deep-link payload; rejected.");
            setAppStatus("Rejected invalid deep link (must be a valid UUID)", "warning");
            return;
          }

          // Security: Prompt user to accept connection instead of auto-exfiltrating saves
          setPendingInvite(parsed);
          setAppStatus(`Lobby invitation received for ${parsed.sessionId.slice(0, 8)}... Action required.`, "active");
        } catch {
          setAppStatus("Error processing browser invitation link.", "error");
        }
      });

      // 7. Listen for save telemetry upload events
      unlistenTelemetry = await listen<TelemetryLog>("telemetry-event", (event) => {
        setTelemetryLogs((prev) => {
          const exists = prev.some(
            (l) => l.file_hash === event.payload.file_hash && l.status === event.payload.status
          );
          if (exists) return prev;
          return [event.payload, ...prev.slice(0, 49)];
        });
        if (event.payload.verified) {
          setAppStatus(`Verified ${event.payload.file_name}`, "success");
        } else if (
          event.payload.status.startsWith("ERROR") ||
          event.payload.status.startsWith("FATAL") ||
          event.payload.status.startsWith("REJECTED")
        ) {
          setAppStatus(event.payload.status, "error");
        } else {
          setAppStatus(event.payload.status, "active");
        }
        fetchOfflineQueue();
      });

      // 8. Listen for auto-updater download progress
      unlistenUpdateProgress = await listen<UpdateProgress>("update-progress", (event) => {
        setUpdateProgress(event.payload);
        if (event.payload.status === "restarting") {
          setAppStatus("Update downloaded! Restarting Encircled...", "success");
        }
      });
    }

    async function fetchProcessStatus() {
      try {
        const current = await invoke<ProcessStatusPayload>("get_process_status");
        if (current) {
          setIsHoi4Running(current.is_running);
          setHasDebugFlag(current.has_debug_flag);
        }
      } catch {
        // Non-fatal
      }
    }

    initSystem();

    const queueInterval = setInterval(fetchOfflineQueue, 10000);
    const processInterval = setInterval(fetchProcessStatus, 15000);

    return () => {
      clearInterval(queueInterval);
      clearInterval(processInterval);
      if (unlistenProcess) unlistenProcess();
      if (unlistenDeepLink) unlistenDeepLink();
      if (unlistenTelemetry) unlistenTelemetry();
      if (unlistenUpdateProgress) unlistenUpdateProgress();
    };
  }, []);

  // Visual folder browse handler
  async function handleBrowseFolder() {
    try {
      const initial = savePath || defaultPath || null;
      const selected = await invoke<string | null>("select_save_folder", {
        initialPath: initial,
      });

      if (selected) {
        const validated = validateAndSanitizePath(selected);
        if (validated) {
          setSavePath(validated);
          localStorage.setItem("encircled_custom_save_path", validated);
          setAppStatus("Custom save folder configured", "success");
        } else {
          setAppStatus("Selected folder path is invalid or contains forbidden characters", "warning");
        }
      }
    } catch {
      setAppStatus("Unable to open folder selection dialog", "error");
    }
  }

  function handleResetDefault() {
    setSavePath("");
    localStorage.removeItem("encircled_custom_save_path");
    setAppStatus("Reset to auto-detected default directory", "idle");
  }

  function handleSessionInputChange(value: string) {
    setSessionId(value);
    const parsed = parseAndValidateSessionInput(value);
    if (parsed?.apiKey) {
      setApiKey(parsed.apiKey);
    }
  }

  // Deep-link Invitation Handlers
  async function handleAcceptInvite() {
    if (!pendingInvite) return;
    const invite = pendingInvite;
    setPendingInvite(null);
    setSessionId(invite.sessionId);
    if (invite.apiKey) {
      setApiKey(invite.apiKey);
    }
    setAppStatus(`Connecting to Lobby ${invite.sessionId.slice(0, 8)}...`, "loading");
    setIsStartingWatcher(true);

    const rawStored = localStorage.getItem("encircled_custom_save_path");
    const activePath = validateAndSanitizePath(rawStored) || null;

    try {
      await invoke("start_watching", {
        sessionId: invite.sessionId,
        apiKey: invite.apiKey || null,
        path: activePath,
      });
      setIsWatching(true);
      setAppStatus("Watching & Live Telemetry Linked 🟢", "active");
      fetchOfflineQueue();
    } catch (err: any) {
      setIsWatching(false);
      const errMsg = typeof err === "string" ? err : err?.message || "Failed to start save watcher.";
      setAppStatus(errMsg, "error");
    } finally {
      setIsStartingWatcher(false);
    }
  }

  function handleDeclineInvite() {
    setPendingInvite(null);
    setAppStatus("Lobby invitation declined", "idle");
  }

  // Start Watcher Handler
  async function handleStartWatching() {
    const raw = sessionId.trim();
    if (!raw) {
      setAppStatus("Please enter a Lobby ID / Match URL before starting", "warning");
      return;
    }

    const parsed = parseAndValidateSessionInput(raw);
    if (!parsed) {
      setAppStatus("Invalid Lobby ID format. Must be a valid 36-character UUID.", "error");
      return;
    }

    const room = parsed.sessionId;
    const effectiveKey = (apiKey.trim() || parsed.apiKey || "");
    if (effectiveKey && !isValidApiKey(effectiveKey)) {
      setAppStatus("Invalid Companion Key format (alphanumeric, dashes and underscores only)", "warning");
      return;
    }

    setSessionId(room);
    if (effectiveKey) setApiKey(effectiveKey);

    setIsStartingWatcher(true);
    setAppStatus("Connecting to Encircled server...", "loading");

    try {
      const effectivePath = validateAndSanitizePath(savePath) || null;
      await invoke("start_watching", {
        sessionId: room,
        apiKey: effectiveKey || null,
        path: effectivePath,
      });
      setIsWatching(true);
      setAppStatus("Watching & Live Telemetry Active 🟢", "active");
      fetchOfflineQueue();
    } catch (err: any) {
      setIsWatching(false);
      const errMsg = typeof err === "string" ? err : err?.message || "Could not initialize watcher. Verify connection and save folder.";
      setAppStatus(errMsg, "error");
    } finally {
      setIsStartingWatcher(false);
    }
  }

  // Stop Watcher Handler
  async function handleStopWatching() {
    try {
      await invoke("stop_watching");
      setIsWatching(false);
      setAppStatus("Watcher Stopped · Standby", "idle");
    } catch {
      setIsWatching(false);
      setAppStatus("Watcher Stopped", "idle");
    }
  }

  // Save Events Actions
  async function handleLoadMoreTelemetry() {
    if (isLoadingMoreTelemetry || !hasMoreTelemetry) return;
    setIsLoadingMoreTelemetry(true);
    try {
      const older = await invoke<TelemetryLog[]>("get_telemetry_history", {
        limit: 15,
        offset: telemetryLogs.length,
      });
      if (older && Array.isArray(older)) {
        if (older.length < 15) {
          setHasMoreTelemetry(false);
        }
        setTelemetryLogs((prev) => {
          const existingKeys = new Set(prev.map((l) => `${l.file_hash}-${l.timestamp}`));
          const filtered = older.filter((l) => !existingKeys.has(`${l.file_hash}-${l.timestamp}`));
          return [...prev, ...filtered];
        });
      } else {
        setHasMoreTelemetry(false);
      }
    } catch (err) {
      console.warn("Could not load more telemetry:", err);
    } finally {
      setIsLoadingMoreTelemetry(false);
    }
  }

  async function handleClearTelemetry() {
    try {
      await invoke("clear_telemetry_history_cmd");
      setTelemetryLogs([]);
      setHasMoreTelemetry(false);
      setAppStatus("Save events history cleared", "idle");
    } catch (err) {
      console.error("Failed to clear telemetry history:", err);
      setAppStatus("Failed to clear save events", "error");
    }
  }

  // Offline Queue Actions
  async function handleClearOfflineQueue() {
    try {
      await invoke("clear_offline_queue");
      setOfflineQueue([]);
      setAppStatus("Offline retry queue cleared", "success");
    } catch {
      setAppStatus("Failed to clear offline queue", "error");
    }
  }

  async function handleRetryOfflineQueue() {
    setIsRetryingQueue(true);
    setAppStatus("Syncing offline save queue...", "loading");
    try {
      const count = await invoke<number>("retry_offline_queue_now");
      setAppStatus(`Retry complete: ${count} saves synced`, "success");
      fetchOfflineQueue();
    } catch {
      setAppStatus("Retry finished with some network failures", "warning");
    } finally {
      setIsRetryingQueue(false);
    }
  }

  // Autostart Handler
  async function handleToggleAutostart() {
    const nextState = !autostartEnabled;
    try {
      await invoke("set_autostart", { enable: nextState });
      setAutostartEnabled(nextState);
      setAppStatus(nextState ? "Auto-start on Windows login enabled" : "Auto-start disabled", "success");
    } catch {
      setAppStatus("Failed to update auto-start preference", "error");
    }
  }

  // Minimize-to-tray Handler
  async function handleToggleMinimizeToTray() {
    const nextState = !minimizeToTray;
    try {
      await invoke("set_minimize_to_tray", { enable: nextState });
      setMinimizeToTray(nextState);
      setAppStatus(nextState ? "Minimize to tray on close enabled" : "Minimize to tray on close disabled", "success");
    } catch {
      setAppStatus("Failed to update minimize preference", "error");
    }
  }

  // Disconnect Session Handler
  async function handleDisconnectSession() {
    if (isWatching) {
      await handleStopWatching();
    }
    setSessionId("");
    setApiKey("");
    setAppStatus("Session unlinked · Ready for new Match Lobby", "idle");
  }

  // Check for Updates Handler
  async function handleCheckForUpdates() {
    setIsCheckingUpdate(true);
    setUpdaterStatus("Checking...");
    try {
      const res = await invoke<UpdateCheckResponse>("check_for_updates_cmd");
      if (res && res.available && res.version) {
        setUpdateAvailable({ version: res.version, notes: res.body });
        setUpdaterStatus(`Update ${res.version} available!`);
        setAppStatus(`Update ${res.version} available!`, "success");
      } else {
        setUpdateAvailable(null);
        setUpdaterStatus(`v${res?.current_version || appVersion} (Latest ✓)`);
        setAppStatus("Encircled is up to date", "success");
      }
    } catch {
      setUpdaterStatus("Check failed");
      setAppStatus("Failed to check for updates", "warning");
    } finally {
      setIsCheckingUpdate(false);
    }
  }

  // Install & Download Update Handler
  async function handleInstallUpdate() {
    setIsInstallingUpdate(true);
    setUpdateProgress({ downloaded: 0, status: "downloading" });
    setAppStatus("Downloading update installer from GitHub Releases...", "loading");

    try {
      await invoke("install_update_cmd");
      setAppStatus("Update complete! Restarting...", "success");
    } catch (err: any) {
      setIsInstallingUpdate(false);
      setUpdateProgress(null);
      const errMsg = typeof err === "string" ? err : err?.message || "Failed to download and install update.";
      setAppStatus(errMsg, "error");
    }
  }

  const effectiveDisplayPath = savePath.trim() || defaultPath || "Detecting standard Paradox save directory...";
  const isCustomActive = Boolean(savePath.trim());

  const trimmedSessionInput = sessionId.trim();
  const parsedInputPreview = trimmedSessionInput ? parseAndValidateSessionInput(trimmedSessionInput) : null;
  const isSessionInputValid = Boolean(parsedInputPreview);

  return (
    <div className="app-canvas">
      <div className="app-shell">
        <Header
          appVersion={appVersion}
          isWatching={isWatching}
          theme={theme}
          onToggleTheme={toggleTheme}
        />

        <main className="main-content-layout">
          {pendingInvite && (
            <div className="invite-prompt-overlay" role="alert">
              <div className="invite-prompt-content">
                <div className="invite-prompt-title">
                  <span>🔗</span>
                  <span>Lobby Invitation Received</span>
                </div>
                <div className="invite-prompt-desc">
                  A browser invitation requested connection to Match Lobby <code>{pendingInvite.sessionId.slice(0, 8)}...</code>
                  {pendingInvite.apiKey ? " with Companion Key." : "."} Do you want to connect?
                </div>
              </div>
              <div className="invite-prompt-actions">
                <button className="invite-btn-accept" onClick={handleAcceptInvite} type="button">
                  Accept &amp; Connect
                </button>
                <button className="invite-btn-decline" onClick={handleDeclineInvite} type="button">
                  Decline
                </button>
              </div>
            </div>
          )}

          <LobbyConnector
            sessionId={sessionId}
            onSessionIdChange={handleSessionInputChange}
            apiKey={apiKey}
            onApiKeyChange={setApiKey}
            isSessionValid={isSessionInputValid}
            parsedUuid={parsedInputPreview?.sessionId}
            hasKey={Boolean(apiKey)}
            isWatching={isWatching}
            isStarting={isStartingWatcher}
            onStartWatching={handleStartWatching}
            onStopWatching={handleStopWatching}
            onDisconnectSession={handleDisconnectSession}
          />

          <FolderSelector
            effectiveDisplayPath={effectiveDisplayPath}
            isCustomActive={isCustomActive}
            onBrowseFolder={handleBrowseFolder}
            onResetDefault={handleResetDefault}
          />

          <DiagnosticsHUD
            isHoi4Running={isHoi4Running}
            hasDebugFlag={hasDebugFlag}
            statusInfo={
              statusInfo.type === "idle" && isHoi4Running
                ? { text: "HOI4 Active · Ready to Link Lobby", type: "active" }
                : statusInfo
            }
          />

          <ActivityTabs
            telemetryLogs={telemetryLogs}
            hasMoreTelemetry={hasMoreTelemetry}
            isLoadingMoreTelemetry={isLoadingMoreTelemetry}
            onLoadMoreTelemetry={handleLoadMoreTelemetry}
            onClearTelemetry={handleClearTelemetry}
            offlineQueue={offlineQueue}
            isRetryingQueue={isRetryingQueue}
            onRetryQueue={handleRetryOfflineQueue}
            onClearQueue={handleClearOfflineQueue}
            autostartEnabled={autostartEnabled}
            onToggleAutostart={handleToggleAutostart}
            minimizeToTray={minimizeToTray}
            onToggleMinimizeToTray={handleToggleMinimizeToTray}
            updaterStatus={updaterStatus}
            isCheckingUpdate={isCheckingUpdate}
            onCheckForUpdates={handleCheckForUpdates}
            updateAvailable={updateAvailable}
            isInstallingUpdate={isInstallingUpdate}
            onInstallUpdate={handleInstallUpdate}
            updateProgress={updateProgress}
            formatTimestamp={formatTimestamp}
          />
        </main>
      </div>
    </div>
  );
}

export default App;
