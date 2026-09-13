# Encircled Desktop App

A lightweight, high-performance **Tauri v2** desktop application designed for **Hearts of Iron IV (HOI4)** multiplayer match telemetry, automated save file parsing, and competitive anti-cheat verification.

---

## Features

- **Automated Save File Monitoring**: Continuously watches the local Paradox HOI4 save games directory for `.hoi4` autosaves using efficient file system debouncing.
- **Streaming Disk Uploads**: Streams large save files directly from disk into HTTP multipart requests using asynchronous chunk buffers without buffering multi-hundred-megabyte files into RAM.
- **Anti-Cheat Telemetry**: Monitors running process command-line arguments to detect whether the game is running with `-debug` or `--debug` flags.
- **Offline SQLite Queue**: Stores save uploads in a local SQLite database (`rusqlite`) when offline, retrying upload tasks automatically in a background loop when connectivity is restored.
- **Native System Notifications & System Tray**: Runs unobtrusively in the background with native OS notifications and system tray controls.
- **Deep Linking & Session Sync**: Handles custom `encircled://` URL schemes and web lobby URLs (`https://encircledmp.com/lobbies/...`, `encircled.com`, or `localhost:5173`) to seamlessly sync match sessions with the desktop application.

---

## Tech Stack

- **Frontend**: React, TypeScript, Vite
- **Backend Core**: Rust (Tauri v2)
- **Database**: SQLite (`rusqlite` bundled)
- **Key Rust Crates**: `tauri` v2, `tokio`, `notify-debouncer-mini`, `reqwest`, `sha2`, `sysinfo`, `tauri-plugin-deep-link`, `tauri-plugin-notification`, `tauri-plugin-single-instance`

---

## Getting Started

### Prerequisites

- **Node.js** (v18+) and **npm**
- **Rust** and **Cargo** (latest stable toolchain)
- **System Dependencies** (Linux):
  ```bash
  sudo apt install -y build-essential pkg-config libssl-dev libgtk-3-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev
  ```

### Development Setup

1. **Clone the repository**:

   ```bash
   git clone https://github.com/KeremVona/EncircledDesktop.git
   cd EncircledDesktop
   ```

2. **Install Frontend Dependencies**:

   ```bash
   npm install
   ```

3. **Run in Development Mode**:
   ```bash
   npm run tauri dev
   ```

### Environment & API Configuration

| Environment | Backend API Base URL | Frontend Web / Lobby URL |
| :--- | :--- | :--- |
| **Production** | `https://api.encircledmp.com` | `https://encircledmp.com` (or `encircled.com`) |
| **Development** | `http://localhost:5292` | `http://localhost:5173` |

- **Automatic Environment Switching**: In debug mode (`npm run tauri dev`), the app connects to `http://localhost:5292`. In release/production builds (`npm run tauri build`), it points to `https://api.encircledmp.com`.
- **Manual API Override**: You can override the backend endpoint at any time by setting the `ENCIRCLED_API_URL` environment variable.
- **Link Acceptance**: The app accepts session URLs from `encircledmp.com` and `encircled.com` (with or without `www` and `https://`), `localhost:5173`, custom deep-link protocols (`encircled://`), or raw UUIDs.

---

## Building for Production

To build an optimized binary executable and installer package for your operating system:

```bash
npm run tauri build
```

The output installers will be generated under `src-tauri/target/release/bundle/`.

## Privacy and Data Handling

Encircled monitors your local Hearts of Iron IV save game directory solely to parse and upload match data to your account:

- **Monitored Path:** `%USERPROFILE%\Documents\Paradox Interactive\Hearts of Iron IV\save games` or the path under OneDrive
- **Data Collected:** Parsed game statistics, country tags, player identifiers, match timestamps and more, please refer to the [Privacy Policy](https://github.com/KeremVona/EncircledDesktop/blob/main/PrivacyPolicy.md) and [Terms of Service](https://github.com/KeremVona/EncircledDesktop/blob/main/TermsOfService.md).
- **Controls:** File watching can be stopped in the app or by closing the app.

---

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
