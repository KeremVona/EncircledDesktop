import { SunIcon, MoonIcon } from "./Icons";

interface HeaderProps {
  appVersion: string;
  isWatching: boolean;
  theme: "dark" | "light";
  onToggleTheme: () => void;
}

export function Header({
  appVersion,
  isWatching,
  theme,
  onToggleTheme,
}: HeaderProps) {
  return (
    <header className="app-header">
      <div className="header-brand">
        <div className={`brand-logo-badge ${isWatching ? "logo-active" : ""}`}>
          <img src="/encircled_logo.svg" alt="Encircled" className="brand-logo-img" />
        </div>
        <div className="brand-text">
          <div className="title-row">
            <h1 className="brand-title">ENCIRCLED</h1>
            <span className="version-tag">v{appVersion}</span>
          </div>
          <span className="brand-subtitle">Desktop Companion</span>
        </div>
      </div>

      <div className="header-controls">
        <button
          type="button"
          onClick={onToggleTheme}
          className="btn-theme-toggle"
          title={`Switch to ${theme === "dark" ? "light" : "dark"} mode`}
          aria-label="Toggle theme"
        >
          {theme === "dark" ? <SunIcon size={16} /> : <MoonIcon size={16} />}
        </button>

        <div className={`status-badge ${isWatching ? "status-live" : "status-standby"}`}>
          <span className="status-indicator-dot" />
          <span>{isWatching ? "WATCHING" : "STANDBY"}</span>
        </div>
      </div>
    </header>
  );
}
