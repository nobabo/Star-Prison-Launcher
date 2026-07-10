# AGENTS

## StarPrison Tauri Working Rules

- Respond to the user in Korean unless they explicitly ask for another language.
- If a referenced local file, image, log, or artifact cannot be read, say that it was not read and do not infer or hallucinate its contents as if it had been inspected.
- `.files/` is gitignored, so do not trust `git status` alone for plugin, mod, wiki, or release asset work. Inspect the real local files and the distributed zip or release asset directly.
- For launcher runtime issues, check `AppData/Roaming/star-prison-launcher/logs/` and `.minecraft/crash-reports/*.txt` before relying on broader guesses or only reading `latest.log`.
- Treat bundled config as seed defaults, not launch-time overwrite truth. `client.config.json`, `config.zip`, and `shaderpacks.zip` must not be assumed to be rewritten on every run.
- For the launcher main page UI, render buttons as SVG/icon controls instead of visible text labels; keep accessible names in `aria-label`/`title`.
- For auth and security changes, keep OAuth2 PKCE token final exchange, log redaction, DPAPI failure recovery with settings preservation and re-login, browser allowlists, and download allowlists as separate concerns.
- `LauncherAccountBridge` work belongs under `.files/plugins/LauncherAccountBridge`. `LauncherCompanionProvider` is the server DB/API integration point.
- For Nexo, CustomFishing, and LiteFish work, fish visuals come from Nexo and CustomFishing should use `material: Nexo:<id>`. Keep `item-detection-order` as `Nexo` first, then `CUSTOM_MODEL_DATA`, `VANILLA`, and `PUFFERFISH/TROPICAL_FISH`, with vanilla fish as the last checkpoint.
- For wiki work, if Notion MCP is unstable, use the downloaded export under `.files/wiki/초안` and local plugin/world data. Do not rewrite whole export HTML unless necessary; replace only the `page-body` content. For fish pages, use image/name/lore blocks instead of tables.

## Runtime Log Evidence

- For hard Minecraft crashes, inspect `.minecraft/crash-reports/*.txt` before relying on `latest.log`.
- For non-crash proof, inspect `.minecraft/logs/latest.log` and dated `.log.gz` archives directly.
- Use exact player names or exact chat/system messages before broad keyword searches.
- `latest.log` can miss older same-day events; dated compressed logs may contain the decisive evidence.

## Windows Toolchain

- Discover Windows Cargo with `cmd.exe /c where cargo` before hardcoding a path.
- Current verified Cargo path on this machine: `C:\Users\82105\Dev\.cargo\bin\cargo.exe`.
- If WSL reports `cargo: command not found`, use the Windows toolchain for this repo before blaming Rust source changes.
