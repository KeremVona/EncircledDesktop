**RULES OF ENGAGEMENT & PLATFORM AGREEMENT**

# Terms of Service

_Last Updated: August 2026 · These Terms of Service govern your access to and use of the Encircled website, APIs, multiplayer matchmaking, and Encircled Desktop Companion application._

---

### Key Highlights

- **Competitive Integrity**  
  Tampering with save files, injecting altered stats, or sabotaging ranked matches is strictly prohibited and results in automatic match invalidation and account penalties.

- **Desktop Companion Safety**  
  The companion is open-source, zero-injection, and read-only. It parses standard autosaves from your local save directory to stream telemetry to your lobby.

- **Independent Fan Platform**  
  Encircled is an independent competitive hub and is not affiliated with, endorsed by, or sponsored by Paradox Interactive AB or Valve Corporation.

---

### 1. Acceptance of Terms

By accessing, browsing, or utilizing the Encircled platform (including web dashboards, SignalR telemetry hubs, APIs, and the Encircled Desktop Companion application), you acknowledge that you have read, understood, and agreed to be bound by these Terms of Service and our [Privacy Policy](https://github.com/KeremVona/EncircledDesktop/blob/main/PrivacyPolicy.md). If you do not agree to these terms, you must immediately cease using the platform.

---

### 2. Description of Service & Desktop Companion

Encircled provides real-time strategic match telemetry, match archiving, role-based TrueSkill/MMR ranking, and competitive match moderation for multiplayer Hearts of Iron IV games.

> **Encircled Desktop Companion Behavior:**
>
> - **Read-Only Disk Access:** The companion monitors the designated local directory (defaulting to Paradox save folders) for generated autosave files.
> - **Zero Process Hooking:** The companion does not inject code, hook system DLLs, or change the Hearts of Iron IV executable in memory.
> - **Match-Scoped Authentication:** Telemetry packets are authenticated using match session companion keys (`X-Companion-Key`) issued by the lobby host.

---

### 3. Commander Accounts & Security

You may enlist on Encircled with or without providing an email address:

- **Account Credentials:** You are responsible for safeguarding your password and account credentials.
- **Emergency Recovery Keys:** If you register without an email address, the platform issues a one-time 16-character cryptographic Emergency Recovery Key. You are solely responsible for saving this key. Without a linked email or your recovery key, lost accounts cannot be recovered.
- **Email Linking:** You may link a verified email address to your profile at any time to enable standard 1-click password resets.

---

### 4. Rules of Engagement & Fair Play

Encircled operates an automated telemetry audit engine (`MatchSaveAudits`) and community moderation tools. You agree **NOT** to:

- **✕ Save File Manipulation:** Editing game save variables, fabricating casualties, changing tag ownership, or altering factory counts before companion upload.
- **✕ Match Sabotage / Ghosting:** Intentionally deleting divisions, abandoning competitive rated games without surrender protocol, or sharing private faction telemetry.
- **✕ Abuse & Harassment:** Transmitting hate speech, slurs, harassment, or illegal content in lobby chat, lobby titles, or user callsigns.
- **✕ API Flooding & Exploit Probing:** Attempting to bypass rate limiters, spoofing SignalR user channels, or conducting denial-of-service attacks against backend endpoints.

---

### 5. Moderation & Disciplinary Enforcement

To protect the competitive environment, Encircled and designated lobby hosts maintain the right to take moderation action:

- **Host Moderation & Democratic Votes:** Lobby hosts and lobby vote sessions may kick or ban disruptive players from active sessions.
- **Integrity Trust Badges:** Accounts flagged with high anomaly rates or audit violations will automatically receive an **Unverified** or **Warning** Trust Badge on their career profile.
- **Account Suspension:** We reserve the right to suspend or permanently terminate accounts found engaging in save tampering, rating manipulation, or harassment without prior notice.

---

### 6. Intellectual Property & Third-Party Disclaimers

Hearts of Iron IV is a registered trademark of Paradox Interactive AB. Steam is a registered trademark of Valve Corporation.

Encircled is an independent, community-driven platform made by and for the Hearts of Iron IV competitive multiplayer community. Encircled is **not affiliated with, associated with, endorsed by, or sponsored by Paradox Interactive AB or Valve Corporation**. All game assets, nation flags, and terminology referenced on this platform are used strictly for non-commercial descriptive and gameplay intelligence purposes.

---

### 7. Disclaimer of Warranties & Limitation of Liability

THE ENCIRCLED PLATFORM, APIS, AND DESKTOP COMPANION ARE PROVIDED ON AN **"AS IS" AND "AS AVAILABLE"** BASIS WITHOUT WARRANTIES OF ANY KIND, EITHER EXPRESS OR IMPLIED.

In no event shall Encircled, its developers, or contributors be held liable for any direct, indirect, incidental, or consequential damages arising from:

- Game save corruption, match disconnects, or desynchronizations.
- Inaccuracies or delays in real-time parser telemetry computations.
- Third-party service outages (e.g., Steam network downtime or cloud provider interruptions).

---

### 8. Changes to Terms & Contact

We reserve the right to revise or update these Terms of Service as our platform features change. Continued use of the platform following the posting of revised terms constitutes your acceptance of the changes.

If you have questions regarding these Terms or want to report an integrity violation, you may contact our development team (**vonavona50@gmail.com**) or submit an issue via the [Encircled Desktop GitHub Repository](https://github.com/KeremVona/EncircledDesktop).
