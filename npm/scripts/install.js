#!/usr/bin/env node
"use strict";

const childProcess = require("child_process");
const fs = require("fs");
const path = require("path");
const os = require("os");
const https = require("https");
const http = require("http");

const REPO = "special-place-administrator/tokenizor_agentic_mcp";

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
    if (processMod.env.TOKENIZOR_HOME) {
      return pathMod.join(processMod.env.TOKENIZOR_HOME, "bin");
    }
    return pathMod.join(osMod.homedir(), ".tokenizor", "bin");
  }

  // Binary lives outside node_modules so npm can update the JS wrapper
  // even while the MCP server holds a lock on the running .exe (Windows).
  const installDir = resolveInstallDir();

  function getPlatformArtifact() {
    const platform = processMod.platform;
    const arch = processMod.arch;

    if (platform === "win32" && arch === "x64") return "tokenizor-mcp-windows-x64.exe";
    if (platform === "darwin" && arch === "arm64") return "tokenizor-mcp-macos-arm64";
    if (platform === "darwin" && arch === "x64") return "tokenizor-mcp-macos-x64";
    if (platform === "linux" && arch === "x64") return "tokenizor-mcp-linux-x64";

    consoleMod.error(`Unsupported platform: ${platform}-${arch}`);
    consoleMod.error("Build from source: https://github.com/" + REPO);
    processMod.exit(1);
  }

  function getVersion() {
    return packageJson.version;
  }

  function getBinaryPath() {
    const ext = processMod.platform === "win32" ? ".exe" : "";
    return pathMod.join(installDir, "tokenizor-mcp" + ext);
  }

  function getPendingPath() {
    const ext = processMod.platform === "win32" ? ".exe" : "";
    return pathMod.join(installDir, "tokenizor-mcp.pending" + ext);
  }

  function download(url) {
    if (overrides.download) {
      return overrides.download(url);
    }
    return new Promise((resolve, reject) => {
      const client = url.startsWith("https") ? https : http;
      client.get(url, { headers: { "User-Agent": "tokenizor-mcp" } }, (res) => {
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
    try {
      const output = execSyncFn(`"${binPath}" --version`, {
        encoding: "utf8",
        timeout: 5000,
      }).trim();
      const match = output.match(/(\d+\.\d+\.\d+)/);
      return match ? match[1] : null;
    } catch {
      return null;
    }
  }

  function isLockedError(error) {
    return error && (error.code === "EPERM" || error.code === "EBUSY");
  }

  function removePendingIfPresent(pendingPath) {
    try {
      fsMod.unlinkSync(pendingPath);
    } catch {}
  }

  function writeInstalledBinary(binPath, pendingPath, data) {
    fsMod.writeFileSync(binPath, data);
    fsMod.chmodSync(binPath, 0o755);
    removePendingIfPresent(pendingPath);
    consoleMod.log(`Installed: ${binPath}`);
  }

  function stopRunningWindowsProcesses(binPath) {
    if (processMod.platform !== "win32") {
      return [];
    }

    const script = [
      "$target = [System.IO.Path]::GetFullPath($env:TOKENIZOR_TARGET_BIN)",
      "$comparer = [System.StringComparer]::OrdinalIgnoreCase",
      "$procs = Get-CimInstance Win32_Process | Where-Object {",
      "  $_.Name -eq 'tokenizor-mcp.exe' -and $_.ExecutablePath -and",
      "  $comparer.Equals([System.IO.Path]::GetFullPath($_.ExecutablePath), $target)",
      "}",
      "$ids = @($procs | ForEach-Object { [int]$_.ProcessId })",
      "if ($ids.Count -gt 0) {",
      "  Stop-Process -Id $ids -Force -ErrorAction SilentlyContinue",
      "  $ids | ConvertTo-Json -Compress",
      "}",
    ].join("; ");

    try {
      const output = execFileSyncFn(
        "powershell.exe",
        ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", script],
        {
          encoding: "utf8",
          env: { ...processMod.env, TOKENIZOR_TARGET_BIN: binPath },
        }
      ).trim();

      if (!output) {
        return [];
      }
      const parsed = JSON.parse(output);
      return Array.isArray(parsed) ? parsed : [parsed];
    } catch (error) {
      consoleMod.log(
        `Failed to stop running Tokenizor processes automatically: ${error.message}`
      );
      return [];
    }
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
          `Stopping ${stoppedProcessIds.length} running Tokenizor process(es) so the update can be applied...`
        );
        const installedAfterStop = await retryInstallAfterStop(binPath, pendingPath, data);
        if (installedAfterStop) {
          return { status: "installed", stoppedProcessIds };
        }
      }

      fsMod.writeFileSync(pendingPath, data);
      fsMod.chmodSync(pendingPath, 0o755);
      consoleMod.log(`Binary is locked (MCP server running). Staged update at: ${pendingPath}`);
      consoleMod.log(`Update will apply automatically on next launch.`);
      return { status: "staged", stoppedProcessIds };
    }
  }

  async function main() {
    const binPath = getBinaryPath();
    const pendingPath = getPendingPath();
    const version = getVersion();

    // Skip only if binary exists AND matches the expected version
    if (fsMod.existsSync(binPath)) {
      const installed = getInstalledVersion(binPath);
      if (installed === version) {
        removePendingIfPresent(pendingPath);
        consoleMod.log(`tokenizor-mcp v${version} already installed at ${binPath}`);
        return;
      }
      consoleMod.log(
        `tokenizor-mcp v${installed || "unknown"} found, updating to v${version}...`
      );
    }

    const artifact = getPlatformArtifact();
    const url = `https://github.com/${REPO}/releases/download/v${version}/${artifact}`;

    consoleMod.log(
      `Downloading tokenizor-mcp v${version} for ${processMod.platform}-${processMod.arch}...`
    );
    consoleMod.log(`  ${url}`);

    try {
      const data = await download(url);
      await installDownloadedBinary(binPath, pendingPath, data);
    } catch (err) {
      consoleMod.error(`Failed to download binary: ${err.message}`);
      consoleMod.error("");
      consoleMod.error("You can build from source instead:");
      consoleMod.error("  git clone https://github.com/" + REPO);
      consoleMod.error("  cd tokenizor_agentic_mcp");
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
    stopRunningWindowsProcesses,
  };
}

module.exports = { createInstaller };

if (require.main === module) {
  createInstaller().main();
}
