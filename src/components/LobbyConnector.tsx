import { CheckIcon, AlertTriangleIcon, BoltIcon, RefreshIcon, StopIcon, LinkIcon } from "./Icons";

interface LobbyConnectorProps {
  sessionId: string;
  onSessionIdChange: (value: string) => void;
  apiKey: string;
  onApiKeyChange: (value: string) => void;
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
  apiKey,
  onApiKeyChange,
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
          Lobby ID or Pairing Link
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
            placeholder="Paste match URL, pairing link (UUID?key=...), or UUID..."
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
                {hasKey ? (
                  <span className="key-pill" title="Companion API Key is linked and authorized">
                    Key Linked ✓
                  </span>
                ) : (
                  <span className="key-pill warning" title="Pairing link without key. Paste the full pairing link from the lobby modal for verified host telemetry.">
                    No Auth Key
                  </span>
                )}
              </div>
            ) : (
              <div className="validation-item warning">
                <AlertTriangleIcon size={14} />
                <span>Please enter a valid 36-character UUID or match pairing link</span>
              </div>
            )}
          </div>
        )}
      </div>

      {isSessionValid && !hasKey && (
        <div className="input-block" style={{ marginTop: "0.5rem" }}>
          <label htmlFor="api-key-input" className="input-label">
            Companion Auth Key (from Lobby Pairing Modal)
          </label>
          <div className="input-wrapper">
            <input
              id="api-key-input"
              type="text"
              value={apiKey}
              onChange={(e) => onApiKeyChange(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && !isStarting) {
                  onStartWatching();
                }
              }}
              placeholder="Paste companion API key..."
              className="text-input font-code"
              spellCheck="false"
              autoComplete="off"
            />
          </div>
        </div>
      )}

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
