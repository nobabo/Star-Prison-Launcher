import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const ROOT_DIR = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const TARGET_DIR = path.join(ROOT_DIR, "src-tauri", "target", "release");
const NSIS_WORK_DIR = path.join(TARGET_DIR, "nsis", "x64");
const BUNDLE_NSIS_DIR = path.join(TARGET_DIR, "bundle", "nsis");
const OUTPUT_DIR = path.join(ROOT_DIR, "dist");
const TAURI_CONF = path.join(ROOT_DIR, "src-tauri", "tauri.conf.json");
const INSTALLER_NAME = "star-prison.exe";

function fail(message) {
  console.error(message);
  process.exit(1);
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: ROOT_DIR,
    stdio: "inherit",
    windowsHide: false,
    shell: false,
    ...options,
  });

  if (result.error) {
    fail(result.error.message);
  }
  if (result.status !== 0) {
    fail(`${command} failed with exit code ${result.status}`);
  }
}

function removeExeFiles(directory) {
  if (!fs.existsSync(directory)) {
    return;
  }

  for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
    if (entry.isFile() && entry.name.toLowerCase().endsWith(".exe")) {
      fs.rmSync(path.join(directory, entry.name), { force: true });
    }
  }
}

function readJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, "utf8"));
}

function runPnpm(args) {
  run("pnpm", args, {
    shell: process.platform === "win32",
  });
}

function makeNsisPath() {
  const localAppData = process.env.LOCALAPPDATA;
  if (localAppData) {
    const bundled = path.join(localAppData, "tauri", "NSIS", "makensis.exe");
    if (fs.existsSync(bundled)) {
      return bundled;
    }
  }
  return "makensis.exe";
}

function buildTauriNsisTemplate() {
  console.log("Building Tauri NSIS template...");
  fs.mkdirSync(TARGET_DIR, { recursive: true });
  fs.mkdirSync(BUNDLE_NSIS_DIR, { recursive: true });
  removeExeFiles(NSIS_WORK_DIR);
  removeExeFiles(BUNDLE_NSIS_DIR);

  runPnpm(["exec", "tauri", "build", "--bundles", "nsis"]);
  removeExeFiles(BUNDLE_NSIS_DIR);
  console.log("Tauri NSIS template generated.");
}

function patchNsisScript() {
  const scriptPath = path.join(NSIS_WORK_DIR, "installer.nsi");
  if (!fs.existsSync(scriptPath)) {
    fail(`NSIS script was not generated: ${scriptPath}`);
  }

  let text = fs.readFileSync(scriptPath, "utf8").replace(/^\uFEFF/, "");

  text = text.replace(/!define OUTFILE ".*?"/, `!define OUTFILE "${INSTALLER_NAME}"`);

  text = text.replace(
    /(!define MUI_WELCOMEFINISHPAGE_BITMAP "\$\{SIDEBARIMAGE\}")(?!\r?\n\s*!define MUI_WELCOMEFINISHPAGE_BITMAP_NOSTRETCH)/,
    "$1\n  !define MUI_WELCOMEFINISHPAGE_BITMAP_NOSTRETCH",
  );

  text = text.replace(
    /(!define MUI_HEADERIMAGE_BITMAP\s+"\$\{HEADERIMAGE\}")(?!\r?\n\s*!define MUI_HEADERIMAGE_BITMAP_NOSTRETCH)/,
    "$1\n  !define MUI_HEADERIMAGE_BITMAP_NOSTRETCH",
  );

  text = text.replace(
    /; 1\. Welcome Page\r?\n(?:(?:!define MUI_WELCOMEPAGE_TITLE|!define MUI_WELCOMEPAGE_TEXT).*?\r?\n)*!define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfPassive\r?\n!insertmacro MUI_PAGE_WELCOME/,
    [
      "; 1. Welcome Page",
      '!define MUI_WELCOMEPAGE_TITLE "별도소 런처 설치를 시작합니다."',
      '!define MUI_WELCOMEPAGE_TEXT "별도소 런처를 설치합니다.$\\r$\\n$\\r$\\n계속하시려면 다음 버튼을 눌러 주세요."',
      "!define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfPassive",
      "!insertmacro MUI_PAGE_WELCOME",
    ].join("\n"),
  );

  text = text.replace(
    /!define MUI_FINISHPAGE_SHOWREADME\r?\n(?!\s*!define MUI_FINISHPAGE_SHOWREADME_NOTCHECKED)/,
    "!define MUI_FINISHPAGE_SHOWREADME\n!define MUI_FINISHPAGE_SHOWREADME_NOTCHECKED\n",
  );

  text = text.replace(
    'StrCpy $INSTDIR "$LOCALAPPDATA\\${PRODUCTNAME}"',
    'StrCpy $INSTDIR "$PROFILE\\Downloads\\${PRODUCTNAME}"',
  );
  text = text.replace(
    /\r?\n {4}Call RestorePreviousInstallLocation\r?\n/g,
    "\n    ; Previous install locations are intentionally ignored; default to Downloads.\n",
  );
  text = text.replace(
    /\r?\n {2}!define MULTIUSER_INSTALLMODE_FUNCTION RestorePreviousInstallLocation/g,
    "",
  );
  text = text.replace(
    /\r?\nFunction RestorePreviousInstallLocation\r?\n {2}ReadRegStr \$4 SHCTX "\$\{MANUPRODUCTKEY\}" ""\r?\n {2}StrCmp \$4 "" \+2 0\r?\n {4}StrCpy \$INSTDIR \$4\r?\nFunctionEnd\r?\n/g,
    "\n",
  );

  text = text.replace(
    / {2}; Delete app data if the checkbox is selected\r?\n {2}; and if not updating\r?\n {2}\$\{If\} \$DeleteAppDataCheckboxState = 1\r?\n {2}\$\{AndIf\} \$UpdateMode <> 1\r?\n {4}; Clear the install location \$INSTDIR from registry\r?\n {4}DeleteRegKey SHCTX "\$\{MANUPRODUCTKEY\}"\r?\n {4}DeleteRegKey \/ifempty SHCTX "\$\{MANUKEY\}"\r?\n\r?\n {4}; Clear the install language from registry\r?\n {4}DeleteRegValue HKCU "\$\{MANUPRODUCTKEY\}" "Installer Language"\r?\n {4}DeleteRegKey \/ifempty HKCU "\$\{MANUPRODUCTKEY\}"\r?\n {4}DeleteRegKey \/ifempty HKCU "\$\{MANUKEY\}"\r?\n\r?\n {4}SetShellVarContext current\r?\n {4}RmDir \/r "\$APPDATA\\\$\{BUNDLEID\}"\r?\n {4}RmDir \/r "\$LOCALAPPDATA\\\$\{BUNDLEID\}"\r?\n {2}\$\{EndIf\}/,
    [
      "  ; Remove launcher game data on uninstall.",
      "  ; If the app data checkbox is selected, remove the whole launcher data root.",
      "  ${If} $UpdateMode <> 1",
      "    SetShellVarContext current",
      "    ${If} $DeleteAppDataCheckboxState = 1",
      '      RmDir /r "$APPDATA\\star-prison-launcher"',
      "    ${Else}",
      '      RmDir /r "$APPDATA\\star-prison-launcher\\data"',
      "    ${EndIf}",
      "  ${EndIf}",
      "",
      "  ; Delete app registry data if the checkbox is selected",
      "  ; and if not updating",
      "  ${If} $DeleteAppDataCheckboxState = 1",
      "  ${AndIf} $UpdateMode <> 1",
      "    ; Clear the install location $INSTDIR from registry",
      '    DeleteRegKey SHCTX "${MANUPRODUCTKEY}"',
      '    DeleteRegKey /ifempty SHCTX "${MANUKEY}"',
      "",
      "    ; Clear the install language from registry",
      '    DeleteRegValue HKCU "${MANUPRODUCTKEY}" "Installer Language"',
      '    DeleteRegKey /ifempty HKCU "${MANUPRODUCTKEY}"',
      '    DeleteRegKey /ifempty HKCU "${MANUKEY}"',
      "  ${EndIf}",
    ].join("\n"),
  );

  fs.writeFileSync(scriptPath, `\uFEFF${text}`, "utf8");
  assertPatchedInstaller(text);
}

function assertPatchedInstaller(text) {
  if (text.includes("가능한 모든")) {
    fail("Welcome text still contains the removed program-closing paragraph.");
  }
  if (!text.includes(`!define OUTFILE "${INSTALLER_NAME}"`)) {
    fail("NSIS output filename was not patched.");
  }
  if (!text.includes("MUI_FINISHPAGE_SHOWREADME_NOTCHECKED")) {
    fail("Desktop shortcut checkbox default was not patched.");
  }
  if (!text.includes("MUI_WELCOMEFINISHPAGE_BITMAP_NOSTRETCH")) {
    fail("Sidebar bitmap no-stretch option was not patched.");
  }
  if (text.includes('StrCpy $INSTDIR "$LOCALAPPDATA\\${PRODUCTNAME}"')) {
    fail("Default install directory still points to LocalAppData.");
  }
  if (text.includes("Call RestorePreviousInstallLocation")) {
    fail("Installer still restores a previous install directory instead of defaulting to Downloads.");
  }
  if (!text.includes('StrCpy $INSTDIR "$PROFILE\\Downloads\\${PRODUCTNAME}"')) {
    fail("Default install directory was not patched to Downloads.");
  }
}

function rerunNsis() {
  console.log(`Running makensis to produce ${INSTALLER_NAME}...`);
  run(makeNsisPath(), ["installer.nsi"], { cwd: NSIS_WORK_DIR });
}

function copyFinalInstaller() {
  const installerPath = path.join(NSIS_WORK_DIR, INSTALLER_NAME);
  if (!fs.existsSync(installerPath)) {
    fail(`NSIS installer was not generated: ${installerPath}`);
  }

  fs.mkdirSync(BUNDLE_NSIS_DIR, { recursive: true });
  fs.mkdirSync(OUTPUT_DIR, { recursive: true });
  removeExeFiles(BUNDLE_NSIS_DIR);
  const releaseInstaller = path.join(OUTPUT_DIR, INSTALLER_NAME);
  fs.copyFileSync(installerPath, releaseInstaller);

  console.log(`Copied installer: ${releaseInstaller}`);
  console.log("");
  console.log(`Release file is ready: ${releaseInstaller}`);
}

if (process.platform !== "win32") {
  fail("release-nsis.mjs must be run on Windows. Use release.bat.");
}

readJson(TAURI_CONF);
buildTauriNsisTemplate();
patchNsisScript();
rerunNsis();
copyFinalInstaller();
