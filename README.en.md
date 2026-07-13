# StarPrison Launcher

English | [한국어](README.md)

StarPrison Launcher is a Windows-first Tauri v2 launcher for the StarPrison Minecraft server. It handles Microsoft account sign-in, Java 21 runtime setup, Fabric/Minecraft installation, server file synchronization, and game launch from one desktop app.

This README is the entry point for players, server operators, release maintainers, and developers. See `.docs` for deeper command and Rust architecture references.

## Quick Start

### Players

1. Run the latest `star-prison.exe` installer.
2. Sign in with your Microsoft account in the launcher.
3. Press `게임 시작` to download required Java, Minecraft/Fabric files, and server files.
4. If the game closes immediately or resources look wrong, open `프로필`, `로그`, or `스크린샷` from the settings tab under `설치 경로`.

### Developers

```bash
corepack enable
pnpm install
pnpm dev
```

From WSL, use the helper script to prepend Windows Node/Rust paths:

```bash
./run.sh
```

See [.docs/commands.en.md](.docs/commands.en.md) for the full command reference.

## Features

- Microsoft OAuth2 PKCE sign-in and Minecraft ownership/profile validation
- Windows DPAPI protection for stored auth sessions
- Managed Java 21 runtime download with SHA-256 verification
- Parallel Fabric/Minecraft library, native, and asset downloads
- Server release archive installation with checksum validation
- `options.txt` default merging while preserving user settings
- Installation state tracking for `mods`, `config`, and `shaderpacks`
- Settings shortcuts for profile, logs, and screenshots
- HTTPS and allowlist restrictions for external links and downloads
- NSIS installer generation

## Launcher Data Layout

On Windows, the launcher data root defaults to:

```text
%APPDATA%\star-prison-launcher
```

Important entries:

- `user-config.json`: user settings and auth session storage
- `install-state.json`: launcher install/run state
- `config/`: first-run seeded app/server config, the GitHub distribution manifest cache, and release archive state files
- `profile/`: actual Minecraft profile root
- `profile/logs/`: Minecraft logs
- `profile/screenshots/`: Minecraft screenshots
- `data/runtime/java-21/`: managed Java 21 runtime
- `data/downloads/`, `data/staged/`, `data/runtime/staged/`: temporary download/install caches

Downloaded zip files and staged folders are cleaned up when installation succeeds and the folders are empty. The Minecraft `profile` directory lives at the launcher root instead of under `data` to keep the active game path short.

## User Data Preservation

The launcher does not blindly overwrite user data on every run.

- `config/app.config.json` is the single source of truth for app configuration. When its embedded `configVersion` is newer than the local version, repository-managed keys are merged into the local seed while local-only keys are preserved.
- `client.config.json` and `server.manifest.json` are seeded on first run and local files take priority afterward.
- `distribution.json` is refreshed on every launch from the fixed GitHub URL in `app.config.json`; network failures fall back to the last verified cache, then the embedded default.
- `options.txt` is merged by key instead of being replaced wholesale.
- User-editable archives such as `config.zip` and `shaderpacks.zip` preserve existing files where appropriate.
- `mods.zip` is treated as the server-managed mod archive.

When changing release-managed values in `config/app.config.json`, increment `configVersion` so existing installs migrate their seed exactly once. Local development syncs this file automatically before `pnpm dev`; run `pnpm config:sync:local` to force an immediate sync. For other launcher defaults, check `config/client.config.json` and `config/server.manifest.json`. Update GitHub's `config/distribution.json` for mod, game-config, and shader distribution changes.

## Security and Reliability

- Auth tokens are protected with Windows DPAPI.
- DPAPI recovery failure clears only the auth session, not the full settings file.
- External links must use HTTPS and pass the browser allowlist.
- Downloads and remote JSON requests must use HTTPS and pass the download allowlist.
- Downloaded files are verified by size and SHA-256 where available.
- Zip extraction checks path traversal, abnormal sizes, and excessive file counts.
- Risky JVM/Game args show warnings before saving; the user is not forced to remove them.
- The `launcherCompanion` API is disabled by default. To enable it, `enabled` and `apiBaseUrl` must be valid in `config/app.config.json`; any bearer token is supplied only through local untracked configuration, and event submission requires a signed-in player.

## Developer Commands

```bash
corepack enable
pnpm install --frozen-lockfile
pnpm config:sync:local
pnpm dev
pnpm lint
pnpm format:check
```

Windows release:

```powershell
.\release.ps1
```

Release output:

```text
dist\star-prison.exe
```

Windows Rust checks:

```bash
cargo fmt --check --manifest-path src-tauri/Cargo.toml
cargo check --locked --manifest-path src-tauri/Cargo.toml
```

## Docs

- [.docs/commands.en.md](.docs/commands.en.md): command reference
- [.docs/rust_source_guide.md](.docs/rust_source_guide.md): Rust/Tauri backend guide
- [.docs/CHANGELOG.md](.docs/CHANGELOG.md): changelog
- [.docs/AGENTS.md](.docs/AGENTS.md): project working rules and implementation state

## Troubleshooting

- If installer build fails because an exe cannot be removed, close the running launcher first.
- If disk space errors occur, clean `src-tauri/target` or consider `cargo clean`.
- If WSL Tauri/Rust checks fail because of Linux dependencies, verify again with Windows Cargo.
- If the game exits immediately, open `로그` from settings and inspect the latest Minecraft log in `profile/logs`.
- For hard crashes, check `profile/crash-reports` first.
