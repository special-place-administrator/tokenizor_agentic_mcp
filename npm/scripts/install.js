#!/usr/bin/env node
"use strict";

/**
 * symforge npm postinstall.
 *
 * Install policy (runs on every `npm install`/`npm install -g`):
 *
 *   1. No binary at target path → download and install.
 *   2. Binary exists:
 *      a. Read the recorded version from `symforge.version` (sibling file).
 *         If missing or unparseable, probe the binary via `--version`.
 *      b. Recorded version matches this package → SKIP download. Auto-init
 *         still runs so client configs stay up to date, and any stale
 *         `symforge.pending.*` files are cleaned up.
 *      c. Recorded version differs, is missing, OR the probe fails (dummy
 *         file, permission denied, truncated binary, non-executable) →
 *         RE-DOWNLOAD and overwrite.
 *
 * Corruption safety: `symforge.version` is written ONLY after the binary
 * is fully written and chmod'd (`writeInstalledBinary`). An interrupted
 * install therefore leaves the version file stale or absent, which forces
 * re-download on the next run — a truncated or dummy file at the target
 * path never counts as "already installed", even if it happens to exist.
 *
 * Windows lock handling: when the binary is locked by a running MCP server,
 * the new version is staged at `symforge.pending.exe` and applied by the
 * launcher on next start. See `installDownloadedBinary` for details.
 */

const childProcess = require("child_process");
const fs = require("fs");
const path = require("path");
const os = require("os");
const https = require("https");
const http = require("http");

const REPO = "special-place-administrator/symforge";

function createInstaller(overrides = {}) {
  const fsMod = overrides.fs || fs;
  const pathMod = overrides.path || path;
  const osMod = overrides.os || os;
  const processMod = overrides.process || process;
  const consoleMod = overrides.console || console;
  const execSyncFn = overrides.execSync || childProcess.execSync;
  const execFileSyncFn = overrides.execFileSync || childProcess.execFileSync;
  const sleep = overrides.sleep || ((ms) => new Promise((resolve) => setTimeout(resolve, ms)));
  const packageJson = overrides.packageJson || require("../package.json");

  function resolveInstallDir() {
    if (overrides.installDir) {
      return overrides.installDir;
    }
    if (processMod.env.SYMFORGE_HOME) {
      return pathMod.join(processMod.env.SYMFORGE_HOME, "bin");
    }
    return pathMod.join(osMod.homedir(), ".symforge", "bin");
  }

  // Binary lives outside node_modules so npm can update the JS wrapper
  // even while the MCP server holds a lock on the running .exe (Windows).
  const installDir = resolveInstallDir();
  const versionPath = pathMod.join(installDir, "symforge.version");
  const pendingVersionPath = pathMod.join(installDir, "symforge.pending.version");

  function comparablePath(targetPath) {
    const resolved = pathMod.resolve(targetPath);
    return processMod.platform === "win32" ? resolved.toLowerCase() : resolved;
  }

  function isPathInsideOrSame(parentPath, childPath) {
    if (!parentPath || !childPath) {
      return false;
    }

    try {
      if (comparablePath(parentPath) === comparablePath(childPath)) {
        return true;
      }

      const relative = pathMod.relative(
        pathMod.resolve(parentPath),
        pathMod.resolve(childPath)
      );
      return Boolean(relative) && !relative.startsWith("..") && !pathMod.isAbsolute(relative);
    } catch {
      return false;
    }
  }

  function symforgeHomeIsTemporary() {
    const explicitHome = processMod.env.SYMFORGE_HOME;
    if (!explicitHome || typeof osMod.tmpdir !== "function") {
      return false;
    }
    return isPathInsideOrSame(osMod.tmpdir(), explicitHome);
  }

  function getPlatformArtifact() {
    const platform = processMod.platform;
    const arch = processMod.arch;

    if (platform === "win32" && arch === "x64") return "symforge-windows-x64.exe";
    if (platform === "darwin" && arch === "arm64") return "symforge-macos-arm64";
    if (platform === "darwin" && arch === "x64") return "symforge-macos-x64";
    if (platform === "linux" && arch === "x64") return "symforge-linux-x64";

    consoleMod.error(`Unsupported platform: ${platform}-${arch}`);
    consoleMod.error("Build from source: https://github.com/" + REPO);
    processMod.exit(1);
  }

  function getVersion() {
    return packageJson.version;
  }

  function getBinaryPath() {
    const ext = processMod.platform === "win32" ? ".exe" : "";
    return pathMod.join(installDir, "symforge" + ext);
  }

  function getPendingPath() {
    const ext = processMod.platform === "win32" ? ".exe" : "";
    return pathMod.join(installDir, "symforge.pending" + ext);
  }

  function parseVersion(text) {
    if (!text) {
      return null;
    }
    const match = String(text).match(/(\d+\.\d+\.\d+)/);
    return match ? match[1] : null;
  }

  function readRecordedVersion(targetPath) {
    try {
      return parseVersion(fsMod.readFileSync(targetPath, "utf8").trim());
    } catch {
      return null;
    }
  }

  function writeRecordedVersion(targetPath, version) {
    if (!version) {
      return;
    }
    try {
      fsMod.writeFileSync(targetPath, `${version}\n`);
    } catch {
      // Best-effort metadata only.
    }
  }

  function download(url) {
    if (overrides.download) {
      return overrides.download(url);
    }
    return new Promise((resolve, reject) => {
      const client = url.startsWith("https") ? https : http;
      client.get(url, { headers: { "User-Agent": "symforge" } }, (res) => {
        if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
          return download(res.headers.location).then(resolve).catch(reject);
        }
        if (res.statusCode !== 200) {
          return reject(new Error(`HTTP ${res.statusCode} for ${url}`));
        }
        const chunks = [];
        res.on("data", (chunk) => chunks.push(chunk));
        res.on("end", () => resolve(Buffer.concat(chunks)));
        res.on("error", reject);
      }).on("error", reject);
    });
  }

  function getInstalledVersion(binPath) {
    const recordedVersion = readRecordedVersion(versionPath);
    if (recordedVersion) {
      return recordedVersion;
    }

    try {
      const output = execFileSyncFn(binPath, ["--version"], {
        encoding: "utf8",
        timeout: 5000,
      }).trim();
      const parsedVersion = parseVersion(output);
      writeRecordedVersion(versionPath, parsedVersion);
      return parsedVersion;
    } catch {
      return null;
    }
  }

  function isLockedError(error) {
    return error && (error.code === "EPERM" || error.code === "EBUSY");
  }

  function removePendingArtifacts(pendingPath) {
    for (const target of [pendingPath, pendingVersionPath]) {
      try {
        fsMod.unlinkSync(target);
      } catch {}
    }
  }

  function writeInstalledBinary(binPath, pendingPath, data) {
    fsMod.writeFileSync(binPath, data);
    fsMod.chmodSync(binPath, 0o755);
    writeRecordedVersion(versionPath, getVersion());
    removePendingArtifacts(pendingPath);
    consoleMod.log(`Installed: ${binPath}`);
  }

  function stopRunningWindowsProcesses(binPath) {
    if (processMod.platform !== "win32") {
      return [];
    }

    // NOTE: PowerShell -and operators must stay on the same line as their
    // operands — semicolons are statement terminators, not line joiners.
    const script = [
      "$target = [System.IO.Path]::GetFullPath($env:SYMFORGE_TARGET_BIN)",
      "$comparer = [System.StringComparer]::OrdinalIgnoreCase",
      "$procs = Get-CimInstance Win32_Process | Where-Object { $_.Name -eq 'symforge.exe' -and $_.ExecutablePath -and $comparer.Equals([System.IO.Path]::GetFullPath($_.ExecutablePath), $target) }",
      "$ids = @($procs | ForEach-Object { [int]$_.ProcessId })",
      "if ($ids.Count -gt 0) { Stop-Process -Id $ids -Force -ErrorAction SilentlyContinue; $ids | ConvertTo-Json -Compress }",
    ].join("; ");

    try {
      const output = execFileSyncFn(
        "powershell.exe",
        ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", script],
        {
          encoding: "utf8",
          env: { ...processMod.env, SYMFORGE_TARGET_BIN: binPath },
        }
      ).trim();

      if (!output) {
        return [];
      }
      const parsed = JSON.parse(output);
      return Array.isArray(parsed) ? parsed : [parsed];
    } catch (error) {
      consoleMod.log(
        `Failed to stop running SymForge processes automatically: ${error.message}`
      );
      return [];
    }
  }

  /**
   * Stop all running symforge processes before an update. This includes
   * active stdio MCP sessions; callers are expected to run updates only when
   * interrupting those sessions is acceptable.
   *
   * Returns an array of killed PIDs (Windows) or [] (Unix, best-effort).
   */
  function stopAllRunningProcesses(binPath) {
    if (processMod.platform === "win32") {
      return stopRunningWindowsProcesses(binPath);
    }

    try {
      execSyncFn("pkill -x symforge 2>/dev/null || true", {
        encoding: "utf8",
      });
    } catch {
      // Ignore — process may not exist
    }
    return [];
  }

  async function retryInstallAfterStop(binPath, pendingPath, data) {
    for (let attempt = 0; attempt < 8; attempt += 1) {
      try {
        writeInstalledBinary(binPath, pendingPath, data);
        return true;
      } catch (retryErr) {
        if (!isLockedError(retryErr)) {
          throw retryErr;
        }
        await sleep(250);
      }
    }
    return false;
  }

  async function installDownloadedBinary(binPath, pendingPath, data) {
    fsMod.mkdirSync(installDir, { recursive: true });

    try {
      writeInstalledBinary(binPath, pendingPath, data);
      return { status: "installed", stoppedProcessIds: [] };
    } catch (writeErr) {
      if (!isLockedError(writeErr)) {
        throw writeErr;
      }

      const stoppedProcessIds = stopRunningWindowsProcesses(binPath);
      if (stoppedProcessIds.length > 0) {
        consoleMod.log(
          `Stopping ${stoppedProcessIds.length} running SymForge process(es) so the update can be applied...`
        );
        const installedAfterStop = await retryInstallAfterStop(binPath, pendingPath, data);
        if (installedAfterStop) {
          return { status: "installed", stoppedProcessIds };
        }
      }

      fsMod.writeFileSync(pendingPath, data);
      fsMod.chmodSync(pendingPath, 0o755);
      writeRecordedVersion(pendingVersionPath, getVersion());
      consoleMod.log(`Binary is locked (MCP server running). Staged update at: ${pendingPath}`);
      consoleMod.log(`Update will apply automatically on next launch.`);
      return { status: "staged", stoppedProcessIds };
    }
  }

  /**
   * Detect durable home/global harnesses that can be safely initialized from a
   * global npm postinstall context.
   *
   * Workspace-local clients such as Kilo Code are intentionally excluded here:
   * global npm installs run from the package directory, not from a user
   * workspace, so emitting `.kilocode/*` there would be wrong.
   */
  function detectHomeScopedClients() {
    const clients = [];

    // Claude Code: check for ~/.claude directory
    const claudeDir = pathMod.join(osMod.homedir(), ".claude");
    if (fsMod.existsSync(claudeDir)) {
      clients.push("claude");
    }

    // Claude Desktop: check for the platform-specific config directory.
    const claudeDesktopDir = resolveClaudeDesktopConfigDir();
    if (fsMod.existsSync(claudeDesktopDir)) {
      clients.push("claude-desktop");
    }

    // Codex: check for ~/.codex directory
    const codexDir = pathMod.join(osMod.homedir(), ".codex");
    if (fsMod.existsSync(codexDir)) {
      clients.push("codex");
    }

    // Gemini: check for ~/.gemini directory
    const geminiDir = pathMod.join(osMod.homedir(), ".gemini");
    if (fsMod.existsSync(geminiDir)) {
      clients.push("gemini");
    }

    return clients;
  }

  function resolveClaudeDesktopConfigDir() {
    if (processMod.platform === "win32") {
      const appData =
        processMod.env.APPDATA || pathMod.join(osMod.homedir(), "AppData", "Roaming");
      return pathMod.join(appData, "Claude");
    }
    if (processMod.platform === "darwin") {
      return pathMod.join(osMod.homedir(), "Library", "Application Support", "Claude");
    }
    return pathMod.join(osMod.homedir(), ".config", "Claude");
  }

  /**
   * Run `symforge init` after successful install to configure hooks and MCP
   * server registration for detected global harnesses.
   */
  function runAutoInit(binPath) {
    if (symforgeHomeIsTemporary()) {
      consoleMod.log(
        "Auto-configuring skipped: SYMFORGE_HOME points to a temporary directory. " +
          "The binary was installed, but client configs were not changed to avoid " +
          "wiring harnesses to a disposable path."
      );
      consoleMod.log(
        "Use `npm install -g symforge` or a durable SYMFORGE_HOME for global harness setup."
      );
      return;
    }

    const clients = detectHomeScopedClients();
    const initCwd = osMod.homedir();
    if (clients.length === 0) {
      consoleMod.log(
        "Auto-configuring skipped: no home-scoped clients detected. " +
          "Run `symforge init --client <client>` manually if needed."
      );
      consoleMod.log(
        "Kilo Code is workspace-local; run `symforge init --client kilo-code` from your project directory."
      );
      return;
    }

    consoleMod.log(`Auto-configuring for detected client(s): ${clients.join(", ")}`);
    for (const client of clients) {
      try {
        const output = execFileSyncFn(binPath, ["init", "--client", client], {
          encoding: "utf8",
          timeout: 15000,
          env: processMod.env,
          cwd: initCwd,
        });
        if (output) {
          for (const line of output.trim().split(/\r?\n/)) {
            consoleMod.log(line);
          }
        }
      } catch (error) {
        consoleMod.log(
          `Auto-init warning for ${client}: ${error.message}\n` +
            "You can run manually: symforge init --client " +
            client
        );
      }
    }
    consoleMod.log(
      "Kilo Code is workspace-local; run `symforge init --client kilo-code` from your project directory."
    );
  }

  async function main() {
    const binPath = getBinaryPath();
    const pendingPath = getPendingPath();
    const version = getVersion();

    // Skip only if binary exists AND matches the expected version
    if (fsMod.existsSync(binPath)) {
      const installed = getInstalledVersion(binPath);
      if (installed === version) {
        removePendingArtifacts(pendingPath);
        consoleMod.log(`symforge v${version} already installed at ${binPath}`);
        // Still run init to ensure config is up to date
        runAutoInit(binPath);
        return;
      }
      consoleMod.log(
        `symforge v${installed || "unknown"} found, updating to v${version}...`
      );
    }

    // Stop all running SymForge processes before install so the binary can be
    // replaced in place, even if a live stdio MCP session is currently using it.
    const stoppedPids = stopAllRunningProcesses(binPath);
    if (stoppedPids.length > 0) {
      consoleMod.log(
        `Stopped ${stoppedPids.length} running SymForge process(es) for update`
      );
      // Brief pause to let OS release file handles
      await sleep(500);
    }

    const artifact = getPlatformArtifact();
    const url = `https://github.com/${REPO}/releases/download/v${version}/${artifact}`;

    consoleMod.log(
      `Downloading symforge v${version} for ${processMod.platform}-${processMod.arch}...`
    );
    consoleMod.log(`  ${url}`);

    try {
      const data = await download(url);
      const result = await installDownloadedBinary(binPath, pendingPath, data);

      // Clean up npm cache to reclaim disk space after download.
      // npm cache grows unbounded over time; verify removes stale entries.
      try {
        execSyncFn("npm cache verify --silent", {
          encoding: "utf8",
          timeout: 30000,
          stdio: "ignore",
        });
        consoleMod.log("npm cache verified (stale entries cleaned)");
      } catch {
        // Non-fatal — skip if npm isn't available or verify fails
      }

      if (result.status === "installed") {
        // Binary replaced in place — run init now.
        runAutoInit(binPath);
      } else if (result.status === "staged") {
        // Binary is staged as .pending because the MCP server is still running.
        // Init will run automatically on next launch via the launcher.
        consoleMod.log(
          "Auto-init deferred: will run on next launch when the pending update is applied."
        );
      }
    } catch (err) {
      consoleMod.error(`Failed to download binary: ${err.message}`);
      consoleMod.error("");
      consoleMod.error("You can build from source instead:");
      consoleMod.error("  git clone https://github.com/" + REPO);
      consoleMod.error("  cd symforge");
      consoleMod.error("  cargo build --release");
      processMod.exit(1);
    }
  }

  return {
    getBinaryPath,
    getPendingPath,
    getInstalledVersion,
    installDownloadedBinary,
    isLockedError,
    main,
    stopAllRunningProcesses,
    stopRunningWindowsProcesses,
    detectHomeScopedClients,
    runAutoInit,
  };
}

module.exports = { createInstaller };

if (require.main === module) {
  createInstaller().main();
}
