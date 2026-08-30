import { CheckIcon, AlertTriangleIcon, BoltIcon, RefreshIcon, StopIcon, LinkIcon } from "./Icons";

interface LobbyConnectorProps {
  sessionId: string;
  onSessionIdChange: (value: string) => void;
  isSessionValid: boolean;
  parsedUuid?: string;
  hasKey: boolean;
  isWatching: boolean;
  isStarting: boolean;
  onStartWatching: () => void;
  onStopWatching: () => void;
  onDisconnectSession: () => void;
}

export function LobbyConnector({
  sessionId,
  onSessionIdChange,
  isSessionValid,
  parsedUuid,
  hasKey,
  isWatching,
  isStarting,
  onStartWatching,
  onStopWatching,
  onDisconnectSession,
}: LobbyConnectorProps) {
  const trimmed = sessionId.trim();

  return (
    <section className="card-section">
      <div className="section-header">
        <div className="section-title-group">
          <span className="step-badge">01</span>
          <h2 className="section-heading">Match Lobby</h2>
        </div>
        {trimmed && (
          <button
            type="button"
            onClick={onDisconnectSession}
            className="btn-text-action"
            title="Unlink lobby and clear active session"
          >
            <LinkIcon size={13} />
            <span>Unlink Lobby</span>
          </button>
        )}
      </div>

      <div className="input-block">
        <label htmlFor="lobby-input" className="input-label">
          Lobby ID or Invitation Link
        </label>
        <div className="input-wrapper">
          <input
            id="lobby-input"
            type="text"
            value={sessionId}
            onChange={(e) => onSessionIdChange(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !isStarting) {
                onStartWatching();
              }
            }}
            placeholder="Paste match URL or 36-character UUID..."
            className="text-input font-code"
            spellCheck="false"
            autoComplete="off"
          />
        </div>

        {trimmed && (
          <div className="validation-banner" role="status">
            {isSessionValid ? (
              <div className="validation-item success">
                <CheckIcon size={14} />
                <span>
                  Valid Match ID: <strong className="font-code">{parsedUuid?.slice(0, 18)}...</strong>
                </span>
                {hasKey && <span className="key-pill">Key Linked ✓</span>}
              </div>
            ) : (
              <div className="validation-item warning">
                <AlertTriangleIcon size={14} />
                <span>Please enter a valid 36-character UUID or match invitation link</span>
              </div>
            )}
          </div>
        )}
      </div>

      <div className="action-buttons-group">
        <button
          type="button"
          onClick={onStartWatching}
          disabled={isStarting}
          className={`btn-primary-action ${isWatching ? "btn-active-state" : ""}`}
        >
          {isStarting ? (
            <span className="btn-loading-content">
              <span className="action-spinner" />
              <span>Connecting Watcher...</span>
            </span>
          ) : isWatching ? (
            <span className="btn-icon-content">
              <RefreshIcon size={16} />
              <span>Restart Watcher</span>
            </span>
          ) : (
            <span className="btn-icon-content">
              <BoltIcon size={16} />
              <span>Start Save Watcher</span>
            </span>
          )}
        </button>

        {isWatching && (
          <button
            type="button"
            onClick={onStopWatching}
            disabled={isStarting}
            className="btn-danger-action"
            title="Stop monitoring and disconnect from server"
          >
            <StopIcon size={16} />
            <span>Stop Watcher</span>
          </button>
        )}
      </div>
    </section>
  );
}
