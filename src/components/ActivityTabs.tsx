import { useState } from "react";
import {
  ActivityIcon,
  LayersIcon,
  SettingsIcon,
  RefreshIcon,
  TrashIcon,
  SparklesIcon,
  CheckCircleIcon,
} from "./Icons";

export interface TelemetryLog {
  file_name: string;
  file_hash: string;
  status: string;
  verified: boolean;
  has_debug_flag: boolean;
  timestamp: string;
}

export interface QueuedUpload {
  id: number;
  session_id: string;
  file_path: string;
  file_hash: string;
  file_name: string;
  timestamp: string;
}

export interface UpdateInfo {
  version: string;
}

interface ActivityTabsProps {
  telemetryLogs: TelemetryLog[];
  offlineQueue: QueuedUpload[];
  isRetryingQueue: boolean;
  onRetryQueue: () => void;
  onClearQueue: () => void;
  autostartEnabled: boolean;
  onToggleAutostart: () => void;
  minimizeToTray: boolean;
  onToggleMinimizeToTray: () => void;
  updaterStatus: string;
  isCheckingUpdate: boolean;
  onCheckForUpdates: () => void;
  updateAvailable: UpdateInfo | null;
  formatTimestamp: (raw: string) => string;
}

export function ActivityTabs({
  telemetryLogs,
  offlineQueue,
  isRetryingQueue,
  onRetryQueue,
  onClearQueue,
  autostartEnabled,
  onToggleAutostart,
  minimizeToTray,
  onToggleMinimizeToTray,
  updaterStatus,
  isCheckingUpdate,
  onCheckForUpdates,
  updateAvailable,
  formatTimestamp,
}: ActivityTabsProps) {
  const [activeTab, setActiveTab] = useState<"telemetry" | "queue" | "settings">("telemetry");

  return (
    <section className="activity-container">
      <div className="tab-nav-bar" role="tablist">
        <button
          type="button"
          role="tab"
          aria-selected={activeTab === "telemetry"}
          onClick={() => setActiveTab("telemetry")}
          className={`tab-btn ${activeTab === "telemetry" ? "tab-active" : ""}`}
        >
          <ActivityIcon size={14} />
          <span>Save Events</span>
          {telemetryLogs.length > 0 && (
            <span className="tab-counter">{telemetryLogs.length}</span>
          )}
        </button>

        <button
          type="button"
          role="tab"
          aria-selected={activeTab === "queue"}
          onClick={() => setActiveTab("queue")}
          className={`tab-btn ${activeTab === "queue" ? "tab-active" : ""}`}
        >
          <LayersIcon size={14} />
          <span>Offline Queue</span>
          {offlineQueue.length > 0 && (
            <span className="tab-counter alert">{offlineQueue.length}</span>
          )}
        </button>

        <button
          type="button"
          role="tab"
          aria-selected={activeTab === "settings"}
          onClick={() => setActiveTab("settings")}
          className={`tab-btn ${activeTab === "settings" ? "tab-active" : ""}`}
        >
          <SettingsIcon size={14} />
          <span>Settings</span>
        </button>
      </div>

      <div className="tab-panel">
        {/* Tab 1: Live Telemetry */}
        {activeTab === "telemetry" && (
          <div className="panel-content">
            {telemetryLogs.length === 0 ? (
              <div className="empty-panel-state">
                <ActivityIcon size={24} className="empty-icon text-muted" />
                <p className="empty-title">No save telemetry recorded yet</p>
                <p className="empty-desc">
                  Start Hearts of Iron IV. Autosaves and manual saves will appear here automatically.
                </p>
              </div>
            ) : (
              <div className="feed-list">
                {telemetryLogs.map((log, index) => (
                  <div key={index} className="feed-card">
                    <div className="feed-card-header">
                      <strong className="feed-file-name">{log.file_name}</strong>
                      <span className={`pill-status ${log.verified ? "verified" : "uploaded"}`}>
                        {log.verified ? "Verified ✓" : "Uploaded"}
                      </span>
                    </div>
                    <div className="feed-card-meta">
                      <span className="feed-hash font-code">
                        SHA: {log.file_hash.substring(0, 16)}...
                      </span>
                      <span className="feed-time">{formatTimestamp(log.timestamp)}</span>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>
        )}

        {/* Tab 2: Offline Queue */}
        {activeTab === "queue" && (
          <div className="panel-content">
            <div className="queue-controls-bar">
              <span className="queue-status-text">
                {offlineQueue.length === 0
                  ? "All telemetry synchronized"
                  : `${offlineQueue.length} items waiting for network`}
              </span>
              {offlineQueue.length > 0 && (
                <div className="queue-btn-group">
                  <button
                    type="button"
                    onClick={onRetryQueue}
                    disabled={isRetryingQueue}
                    className="btn-secondary-action btn-sm"
                  >
                    <RefreshIcon size={12} />
                    <span>{isRetryingQueue ? "Syncing..." : "Sync Now"}</span>
                  </button>
                  <button
                    type="button"
                    onClick={onClearQueue}
                    className="btn-danger-action btn-sm"
                  >
                    <TrashIcon size={12} />
                    <span>Clear</span>
                  </button>
                </div>
              )}
            </div>

            {offlineQueue.length === 0 ? (
              <div className="empty-panel-state">
                <CheckCircleIcon size={24} className="empty-icon text-emerald" />
                <p className="empty-title">Queue is empty</p>
                <p className="empty-desc">All save uploads are fully synced with the Encircled server.</p>
              </div>
            ) : (
              <div className="feed-list">
                {offlineQueue.map((item) => (
                  <div key={item.id} className="feed-card">
                    <div className="feed-card-header">
                      <strong className="feed-file-name">{item.file_name}</strong>
                      <span className="pill-status warning">Pending Sync</span>
                    </div>
                    <div className="feed-card-meta">
                      <span className="feed-hash font-code">
                        SHA: {item.file_hash.substring(0, 16)}...
                      </span>
                      <span className="feed-time">{formatTimestamp(item.timestamp)}</span>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>
        )}

        {/* Tab 3: Settings */}
        {activeTab === "settings" && (
          <div className="panel-content settings-flow">
            <div className="settings-options-list">
              <label className="toggle-row">
                <div className="toggle-text">
                  <strong className="toggle-label">Launch on Windows Startup</strong>
                  <span className="toggle-description">
                    Automatically start Encircled Desktop in the background when you log in.
                  </span>
                </div>
                <input
                  type="checkbox"
                  checked={autostartEnabled}
                  onChange={onToggleAutostart}
                  className="switch-checkbox"
                />
              </label>

              <label className="toggle-row">
                <div className="toggle-text">
                  <strong className="toggle-label">Minimize to System Tray</strong>
                  <span className="toggle-description">
                    Keep the save watcher active in the system tray when closing the window.
                  </span>
                </div>
                <input
                  type="checkbox"
                  checked={minimizeToTray}
                  onChange={onToggleMinimizeToTray}
                  className="switch-checkbox"
                />
              </label>
            </div>

            <div className="updater-card">
              <div className="updater-info">
                <strong className="updater-title">Software Updates</strong>
                <span className="updater-state">{updaterStatus}</span>
              </div>
              <button
                type="button"
                onClick={onCheckForUpdates}
                disabled={isCheckingUpdate}
                className="btn-secondary-action btn-sm"
              >
                {isCheckingUpdate ? "Checking..." : "Check Now"}
              </button>
            </div>

            {updateAvailable && (
              <div className="update-available-banner">
                <SparklesIcon size={16} />
                <span>New version {updateAvailable.version} is available!</span>
              </div>
            )}
          </div>
        )}
      </div>
    </section>
  );
}
