import {
  CheckCircleIcon,
  XCircleIcon,
  AlertTriangleIcon,
  ClockIcon,
  ActivityIcon,
} from "./Icons";

export interface StatusInfo {
  type: "idle" | "active" | "success" | "warning" | "error" | "loading";
  text: string;
}

interface DiagnosticsHUDProps {
  isHoi4Running: boolean;
  hasDebugFlag: boolean;
  statusInfo: StatusInfo;
}

export function DiagnosticsHUD({
  isHoi4Running,
  hasDebugFlag,
  statusInfo,
}: DiagnosticsHUDProps) {
  function getStatusIcon() {
    switch (statusInfo.type) {
      case "active":
      case "success":
        return <CheckCircleIcon size={15} className="text-emerald" />;
      case "error":
        return <XCircleIcon size={15} className="text-rose" />;
      case "warning":
        return <AlertTriangleIcon size={15} className="text-amber" />;
      case "loading":
        return <ClockIcon size={15} className="text-sky" />;
      default:
        return <ActivityIcon size={15} className="text-muted" />;
    }
  }

  return (
    <div className="diagnostics-grid">
      <div className="diag-card">
        <span className="diag-title">HOI4 Process</span>
        <div className="diag-value-row">
          {isHoi4Running ? (
            <span className="status-badge-inline success">
              <CheckCircleIcon size={14} />
              <span>Game Running</span>
            </span>
          ) : (
            <span className="status-badge-inline muted">
              <XCircleIcon size={14} />
              <span>Not Running</span>
            </span>
          )}
        </div>
      </div>

      <div className="diag-card">
        <span className="diag-title">Integrity Check</span>
        <div className="diag-value-row">
          {hasDebugFlag ? (
            <span className="status-badge-inline warning">
              <AlertTriangleIcon size={14} />
              <span>-debug Active</span>
            </span>
          ) : (
            <span className="status-badge-inline success">
              <CheckCircleIcon size={14} />
              <span>Clean Runtime</span>
            </span>
          )}
        </div>
      </div>

      <div className="diag-card col-span-full">
        <span className="diag-title">Companion Status</span>
        <div className="diag-status-message">
          {getStatusIcon()}
          <span className={`status-text-body font-code status-${statusInfo.type}`}>
            {statusInfo.text}
          </span>
        </div>
      </div>
    </div>
  );
}
