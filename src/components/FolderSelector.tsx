import { useState } from "react";
import { FolderIcon, CopyIcon, CheckIcon, RefreshIcon } from "./Icons";

interface FolderSelectorProps {
  effectiveDisplayPath: string;
  isCustomActive: boolean;
  onBrowseFolder: () => void;
  onResetDefault: () => void;
}

export function FolderSelector({
  effectiveDisplayPath,
  isCustomActive,
  onBrowseFolder,
  onResetDefault,
}: FolderSelectorProps) {
  const [copied, setCopied] = useState(false);
  const [showResetConfirm, setShowResetConfirm] = useState(false);

  function handleCopy() {
    if (effectiveDisplayPath) {
      navigator.clipboard.writeText(effectiveDisplayPath);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    }
  }

  return (
    <section className="card-section">
      <div className="section-header">
        <div className="section-title-group">
          <span className="step-badge">02</span>
          <h2 className="section-heading">Save Games Directory</h2>
        </div>
        <span className={`pill-tag ${isCustomActive ? "pill-custom" : "pill-auto"}`}>
          {isCustomActive ? "CUSTOM PATH" : "AUTO-DETECTED"}
        </span>
      </div>

      <div className="path-box">
        <div
          className="path-text-container"
          onClick={handleCopy}
          title="Click to copy path to clipboard"
          role="button"
          tabIndex={0}
          onKeyDown={(e) => {
            if (e.key === "Enter" || e.key === " ") {
              handleCopy();
            }
          }}
        >
          <FolderIcon size={18} className="path-leading-icon" />
          <span className="path-string font-code">{effectiveDisplayPath}</span>
          <button
            type="button"
            className="btn-icon-copy"
            onClick={(e) => {
              e.stopPropagation();
              handleCopy();
            }}
            title="Copy path"
            aria-label="Copy folder path"
          >
            {copied ? <CheckIcon size={14} className="text-emerald" /> : <CopyIcon size={14} />}
          </button>
        </div>

        <div className="folder-actions-row">
          <button
            type="button"
            onClick={onBrowseFolder}
            className="btn-secondary-action"
          >
            <FolderIcon size={14} />
            <span>Choose Folder...</span>
          </button>

          {isCustomActive && !showResetConfirm && (
            <button
              type="button"
              onClick={() => setShowResetConfirm(true)}
              className="btn-ghost-action"
              title="Reset to default Paradox Documents directory"
            >
              <RefreshIcon size={13} />
              <span>Reset to Default</span>
            </button>
          )}

          {isCustomActive && showResetConfirm && (
            <div className="inline-confirm-box" role="alert">
              <span className="confirm-prompt">Reset to default path?</span>
              <button
                type="button"
                onClick={() => {
                  onResetDefault();
                  setShowResetConfirm(false);
                }}
                className="btn-confirm-yes"
              >
                Reset
              </button>
              <button
                type="button"
                onClick={() => setShowResetConfirm(false)}
                className="btn-confirm-no"
              >
                Cancel
              </button>
            </div>
          )}
        </div>
      </div>
      <p className="section-helper-text">
        The watcher automatically scans this directory for new monthly and manual autosaves.
      </p>
    </section>
  );
}
