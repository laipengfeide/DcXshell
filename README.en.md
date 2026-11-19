# Dc Xshell

[中文](README.md) | [English](README.en.md)

A desktop SSH client built with **Tauri 2 + Rust + Vue 3**, aimed at ops, QA and developers. It supports **password change, audit logging**, and **streaming file transfer between signed-in users on the same LAN**, with no USB key required. The UI supports **live switching between Chinese and English**.

## Features

### Sessions and connections
- Sign in with a local account or continue as a guest; **guests can open up to 3 parallel sessions, unlimited after signing in**
- Connection profiles are stored in a local SQLite database, grouped in a tree and isolated per signed-in user
- "Test connection" is available before saving, and **a connection can only be saved after a successful test**
- Sessions are shown as server cards plus a terminal on the right

### Terminal
- xterm.js-based terminal with automatic fit to the window size
- Commands that keep producing output (such as `top`) refresh continuously
- Copy and paste with `Ctrl+C` / `Ctrl+V`, or with the mouse context menu
- A command is logged only after the user **presses Enter to submit the whole line**, avoiding per-keystroke noise

### Stability
- 5-second heartbeat to detect connection state
- Automatic reconnect after a drop, up to 3 attempts; if all fail, the card turns red and shows offline

### Account and security
- Avatar plus username menu at the bottom left; signed-in users see "Change password" and "Operation logs", guests do not see "Change password"
- Changing the password requires verifying the old password, and the new password cannot be empty
- On sign-in, sign-out and guest enter/leave, the app records the machine's LAN IPv4 (the real NIC egress address, not 127.0.0.1) into the user and session records, so peers can be located later for LAN transfer

### Audit logs
- Every action by signed-in users and guests is written to a local SQLite audit table, **visible only to the owner** (guests are isolated by device ID)
- Recorded actions:
  - Sign-in and sign-out (including guest enter/leave)
  - Which server was connected (connection name + host)
  - The **whole command submitted with Enter** in the terminal
- **Password masking**: obvious password shapes in a command (`-p***`, `--password`, content after `passwd`) are masked; interactive password prompts such as `sudo`, `ssh`, `su`, `mysql -p` and `passwd` are detected and the following input is masked. Plaintext passwords are never stored
- The log page lists entries newest first and can filter by action type

### LAN file transfer
- Peers on the same subnet discover each other automatically once **both sides are signed in**; no manual IP entry
- Protocol: **UDP 45454 broadcast announcement (every 3 seconds, 10-second timeout) + TCP 45455 direct streaming transfer**
- Single file limit **10GB**; the whole transfer is streamed in 64KB chunks and **never loaded into memory as a whole**
- SHA-256 verification after the transfer; on mismatch the incomplete file is deleted and the transfer marked failed
- The receiver checks free space in the save directory first and fails immediately with a message if there is not enough room; mid-transfer failures (disk full, connection lost) delete the partial file
- A connection is treated as lost only after 120 seconds without progress on a single chunk; large transfers have no overall time limit
- The receiver confirms in a dialog and may choose another save directory; completed receive records offer an "Open folder" button that opens the file manager with the file selected
- The first listen triggers a Windows firewall prompt (choose **Private networks**) — accept it as prompted
- The toolbar adds a "File transfer" button next to "New connection"; **guests do not take part** in LAN transfer

### File transfer (SFTP)
- After connecting, click the folder icon on the session card to open the transfer dialog
- Upload: choose a target directory, the server directory is listed on the right, with common directories built in (default `/home`)
- Download: choose a remote file and save it locally
- The dialog shows file name, size and progress; small files report success immediately
- Right-click actions on server directories:
  - Copy / paste (the same file can keep being pasted after changing directories)
  - On a name conflict, choose "Overwrite" or "Rename the new file" (adds `(2)`)
  - Rename, delete (files and directories)
- Right-click extraction for archives:
  - Extract into the current folder
  - Extract into a folder with the same name (e.g. `bundle.tar.gz` → `bundle/`)
  - Supports `.tar` / `.tar.gz` / `.tgz` / `.tar.bz2` / `.tbz2` / `.tar.xz` / `.txz` / `.zip` / `.gz` / `.bz2` / `.xz`
  - Extraction runs the server's local `tar` / `unzip` (falls back to `python3 -m zipfile` for zip when `unzip` is missing)

### Interface
- Dark theme with Element Plus components
- **Chinese and English**: switch between "中文 / English" on the **Settings** page; the change applies immediately and is persisted. The default follows the system language (non-Chinese systems start in English). Guests can open Settings and switch too
- All UI text and backend messages are bilingual (the backend returns error codes only and the frontend translates them), so there is no mixed-language output
- The browser context menu is suppressed; right-click actions use in-app menus
- The main toolbar uses two rows, leaving room for future features without crowding

### Settings
- Entry point: bottom-left avatar / guest menu → "Settings"; both signed-in users and guests can open it
- **Interface language**: 中文 / English, applied to the current page immediately without a restart
- **LAN transfer receive folder**: the default save directory, chooseable manually; the receive dialog uses it by default
  - When choosing, the app verifies that the folder exists and is writable; if not, it shows a clear message and does not save the value
  - "Restore default" clears the custom folder and falls back to the system Downloads folder
  - If a saved folder later becomes invalid (deleted, drive removed, no permission), the app falls back to the system Downloads folder and shows a notice
- Persistence: the language choice lives in local `localStorage`; the receive folder lives in the local SQLite `settings` table (key `lan_save_dir`). Both survive a restart

### Distribution and runtime
- Rust is built with **MSVC crt-static**: the standalone exe no longer needs `VCRUNTIME140.dll` on the target machine, so double-clicking works on any Win10/11 machine
- The `sshtest` test program is excluded from release bundles via the Cargo feature gate (`test-server`), so the installer does not contain it

## Tech stack

| Layer | Technology |
| --- | --- |
| Desktop framework | Tauri 2.11 |
| Backend | Rust: `russh` 0.65 (SSH), `russh-sftp` 3 (SFTP), `rusqlite` (SQLite), `tokio` (TCP), std `UdpSocket` (broadcast discovery), `sha2` (verification) |
| Frontend | Vue 3 + TypeScript + Vite 4 |
| UI / terminal | Element Plus 2.7, xterm.js 5.5 |
| Plugins | tauri-plugin-dialog, tauri-plugin-clipboard-manager |

## Requirements

- Windows 10/11 x64 (WebView2 Runtime; usually preinstalled on Win11)
- Node.js ≥ 18 and npm
- Rust ≥ 1.89 (install via [rustup](https://rustup.rs/))
- Visual Studio Build Tools (C++ build tools / MSVC)

## Run locally

```bash
git clone https://github.com/laipengfeide/DcXshell.git
cd DcXshell

# Install frontend dependencies
npm install

# Start the desktop client in dev mode (frontend hot reload + Rust backend)
npm run tauri dev
```

## Build the installer

```bash
npm run tauri build
```

Artifacts:

- Executable: `src-tauri/target/release/dc-xshell.exe`
- Windows installer: `src-tauri/target/release/bundle/nsis/Dc Xshell_1.0.0_x64-setup.exe`

> To preview the UI only, run `npm run dev` in a browser. SSH, SFTP and LAN transfer depend on the Tauri backend, so use `npm run tauri dev` for those.

## Usage

1. Start the client, sign in or continue as a guest
2. Create a connection with host, port, username and password/key details
3. Click "Test connection", then save it to the connection tree on the left
4. Double-click the connection to open a session; every command submitted with Enter is **masked and written to the audit log**
5. Click the folder icon on the session card to upload, download and right-click copy/paste/extract/rename/delete
6. Open the operation log from the avatar menu at the bottom left or the "Operation logs" button at the top
7. Once both sides are signed in, open the "File transfer" panel to see peers on the same subnet, pick a peer and a file, and send it; the receiver confirms, then the transfer starts, and "Open folder" locates the saved file afterwards
8. To change the language or the LAN receive folder, open "Settings" from the bottom-left menu

## Language and settings

- Language entry: bottom-left avatar / guest menu → "Settings" → Interface language → 中文 / English
- Priority: saved choice > system language (`zh*` uses Chinese, everything else uses English)
- The change applies to the current page immediately, no restart needed; the choice is stored locally and survives restarts
- Receive-folder entry: the "LAN file transfer" block on the same Settings page, with "Choose folder" and "Restore default"
- Installer descriptions use bilingual side-by-side text (a single bundle resource cannot switch at runtime); the window title `Dc Xshell` is the brand name and contains no language-specific text

## LAN transfer — first run

- **Firewall prompt**: the first time UDP/TCP ports are opened, Windows shows "Windows Defender Firewall has blocked some features of this app". Tick **Private networks** and click "Allow access". On public networks the app will not be discovered by other devices.
- **Both sides must be signed in**: guests do not take part; only signed-in accounts announce themselves.

## Known limitations

- **Same-subnet discovery only**: based on UDP broadcast, so it only works inside one broadcast domain (typically the same switch or router). Cross-subnet, routed, VPN or AP-isolated networks cannot auto-discover
- **Integrity check only, no encryption**: the transfer uses plain TCP plus SHA-256 verification, so corruption is detected but confidentiality is not provided; other devices on the same subnet could in theory sniff the traffic, so do not use it for sensitive data
- **The receiver must confirm**: every incoming transfer needs a local "Accept" click; nothing lands on disk automatically
- **Firewall**: the first listen requires a Windows Firewall authorization for Private networks, otherwise peers cannot find this machine
- **Single files up to 10GB**, one file at a time; fixed ports 45454 (discovery) and 45455 (transfer), so occupation of those ports may break discovery or receiving

## Project layout

```
DcXshell/
├─ src/
│  ├─ components/
│  │  ├─ Sidebar.vue             connection tree + avatar menu + toolbar
│  │  ├─ SessionView.vue         terminal session card
│  │  ├─ AuditLogView.vue        audit log page
│  │  ├─ ChangePasswordDialog.vue change-password dialog
│  │  ├─ SettingsView.vue        settings page (language + LAN receive folder)
│  │  └─ TransferPanel.vue       LAN transfer panel
│  ├─ views/WorkspaceView.vue     main workspace
│  ├─ i18n/                       Chinese/English dictionaries and switching (core.ts / dicts/)
│  ├─ api.ts                      Tauri command wrappers
│  └─ types.ts                    shared types
├─ src-tauri/
│  ├─ src/
│  │  ├─ main.rs                 entry point and command registration
│  │  ├─ commands.rs             connections/sessions/password/open path
│  │  ├─ auth.rs                 sign-in, registration and session restore
│  │  ├─ audit.rs                audit log read/write and password masking
│  │  ├─ net.rs                  local LAN IPv4 detection
│  │  ├─ lan.rs                  UDP discovery + TCP streaming transfer
│  │  ├─ ssh_client.rs           SSH connection, heartbeat and reconnect
│  │  ├─ sftp.rs                 SFTP transfer, copy and extraction
│  │  ├─ models.rs               database schema
│  │  └─ db.rs                   SQLite initialization and device ID
│  ├─ .cargo/config.toml         MSVC crt-static, removes the VCRUNTIME140 dependency
│  ├─ capabilities/              Tauri permission declarations
│  └─ tauri.conf.json            app config and version resources
├─ package.json
├─ README.md                     Chinese guide
└─ README.en.md                  English guide (this file)
```

## Notes

- Dependencies and build output (`node_modules/`, `dist/`, `src-tauri/target/`, installers, etc.) are not versioned.
- If a server lacks `tar` / `unzip`, extraction asks you to install the tool; zip without `unzip` needs `python3`.
- The first run creates `dcxshell.db` (SQLite, holding connections, audit logs, the device ID and settings) and `secret.key` (used to encrypt local connection passwords) under `%APPDATA%\com.dcxshell.app\`. Uninstalling does not delete them.
