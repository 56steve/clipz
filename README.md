# Clipz ✂️ — Clipboard Notch Hub

![Version](https://img.shields.io/badge/version-0.1.2-blue.svg)
![Framework](https://img.shields.io/badge/Tauri-v2-orange.svg)
![Language](https://img.shields.io/badge/Rust-TypeScript-brightgreen.svg)
![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS-blue.svg)
![License](https://img.shields.io/badge/license-MIT-green.svg)

**Clipz** is a modern, lightweight, high-performance desktop clipboard manager and productivity hub for **Windows and macOS**. Designed around a sleek **Dynamic Island / Top Notch** paradigm, Clipz listens for clipboard events in real-time, categorizes items, and provides lightning-fast full-text search. Sensitive items are encrypted at rest on both platforms; see [Security](#-security) for exactly what that protects against.

---

## ✨ Features

- 🏝️ **Dynamic Island Notch UI**: Floating top-center interface with smooth glassmorphism styling, hover-to-expand drawer, and keyboard accessibility.
- ⚡ **Real-Time Monitoring**: Low-overhead Rust clipboard loop. On Windows it uses the native clipboard sequence number; on macOS it polls the pasteboard.
- 🔍 **SQLite FTS5 Full-Text Search**: Instant, sub-millisecond search across your entire clipboard history.
- 🏷️ **Smart Categorization & Filters**: Auto-detects plain text, code snippets, web URLs, and sensitive credentials/passwords.
- 🛡️ **Enterprise-Grade Security**:
  - **Windows**: sensitive items are sealed with the native Data Protection API (DPAPI). The key belongs to your Windows account and never touches the disk.
  - **macOS**: sensitive items are sealed with AES-256-GCM. The key lives in `secret.key` next to the database, readable only by your user account (`0600`). Neither platform ever asks you for a password.
  - **What this does not stop**: software already running as you can ask the OS to unseal a clip, exactly as Clipz does. Encryption at rest protects the database file itself — in backups, on a shared disk, or in another account's hands.
  - **RAM TTL Protection**: High-security clip memory cleanup after 60 seconds.
- 🎯 **Paste Tracking & Source Apps**: Tracks active target windows and source applications.
- ⌨️ **Keyboard & Power User Friendly**:
  - `Arrow Up` / `Arrow Down`: Navigate clipboard history
  - `Enter`: Instant copy selected clip to active clipboard
  - `Esc`: Collapse notch drawer
  - Filter pills for quick focus (`All`, `Text`, `Code`, `Links`, `🔒 Sensitive`)

---

## 🛠️ Technology Stack

| Layer | Technology |
|---|---|
| **Frontend** | HTML5, Modern CSS3 (Vanilla Glassmorphism), TypeScript |
| **Build Tooling** | Vite, TypeScript compiler (`tsc`) |
| **Desktop Framework** | Tauri v2 (`@tauri-apps/api`, `@tauri-apps/cli`) |
| **Backend Core** | Rust, `windows` crate (Win32 API integration) |
| **Database** | SQLite3 with FTS5 (`rusqlite`) |
| **Security** | Windows DPAPI · AES-256-GCM on macOS with an owner-only key file |

---

## 💾 Where your clips are stored

The database lives in the per-user data folder for the platform, never beside the executable:

| Platform | Location |
|---|---|
| **Windows** | `%APPDATA%\clipz\clipz.db` |
| **macOS** | `~/Library/Application Support/Clipz/clipz.db` |
| **Linux** | `$XDG_DATA_HOME/clipz/clipz.db`, else `~/.local/share/clipz/clipz.db` |

Delete that folder to wipe your clipboard history. The file is a plain SQLite database: ordinary clips are stored as-is and anyone with access to your account can read them. Sensitive clips are the exception — only the `🔒 Password Protected` mask is stored in plain text, with the secret held sealed alongside it.

---

## 📋 Prerequisites

Before running or building Clipz on Windows, ensure you have installed:

1. **Node.js** (v18.x or higher) & `npm`: [Download Node.js](https://nodejs.org/)
2. **Rust Toolchain** (1.75+): [Install Rustup](https://rustup.rs/)
3. **C++ Build Tools**: Microsoft Visual C++ Build Tools / Visual Studio Community Edition with "Desktop development with C++" workload.

---

## 🚀 Getting Started

### 1. Clone & Switch to Development Branch

```bash
git clone https://github.com/56steve/clipz.git
cd clipz
git checkout development
```

### 2. Install Dependencies

```bash
npm install
```

### 3. Run in Development Mode

To launch the desktop application in live dev mode (with Hot-Module Reloading):

```bash
npx tauri dev
```

*Or run Vite frontend preview only:*

```bash
npm run dev
```

### 4. Build Production Executable

To compile the standalone Windows binary (`.exe` / `.msi` / bundle):

```bash
npx tauri build
```

---

## 📁 Directory Structure

```
clipz/
├── index.html            # Main Notch HTML Shell
├── styles.css            # Dynamic Island & Glassmorphism Styles
├── tsconfig.json         # TypeScript configuration
├── vite.config.ts        # Vite build & bundle configuration
├── package.json          # Node dependencies & NPM scripts
├── README.md             # Project documentation
├── src/                  # Frontend Application Logic
│   └── main.ts           # Clipz UI event listeners & Tauri invoke bindings
└── src-tauri/            # Tauri Rust Backend & Win32 Integration
    ├── Cargo.toml        # Rust dependencies & metadata
    ├── tauri.conf.json   # Tauri v2 window & permissions config
    └── src/
        ├── main.rs       # Entrypoint
        ├── lib.rs        # Tauri setup & command router
        ├── clipboard.rs  # Win32 clipboard monitoring thread
        ├── db.rs         # SQLite FTS5 database engine
        ├── paste_tracker.rs # Foreground window & paste detector
        └── security.rs   # Sensitive-clip detection and encryption at rest
```

---

## 🌳 Branching & Workflow

- `main`: Stable release branch.
- `development`: Active feature development branch (**Current active workspace**).

---

## 📄 License

This project is licensed under the [MIT License](LICENSE).
