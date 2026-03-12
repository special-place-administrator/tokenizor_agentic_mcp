const test = require("node:test");
const assert = require("node:assert/strict");
const path = require("node:path");
const winPath = path.win32;

const { createLauncher } = require("../bin/launcher.js");

function createFs({ binPath, pendingPath, hasBinary = true, hasPending = false }) {
  let binaryExists = hasBinary;
  let pendingExists = hasPending;
  const renames = [];

  return {
    renames,
    existsSync(target) {
      if (target === binPath) return binaryExists;
      if (target === pendingPath) return pendingExists;
      return false;
    },
    renameSync(from, to) {
      renames.push({ from, to });
      if (from === pendingPath && to === binPath) {
        pendingExists = false;
        binaryExists = true;
        return;
      }
      throw new Error("unexpected rename");
    },
  };
}

function createLauncherForTest({
  fsOverrides,
  execFileSync,
  spawnSync,
  installDir,
  packageVersion = "0.3.12",
  env = {},
}) {
  const logs = [];
  const errors = [];
  const processMock = {
    platform: "win32",
    arch: "x64",
    env,
    execPath: "C:\\node\\node.exe",
  };
  const consoleMock = {
    log(message) {
      logs.push(message);
    },
    error(message) {
      errors.push(message);
    },
  };

  const launcher = createLauncher({
    fs: fsOverrides,
    path: winPath,
    os: { homedir: () => "C:\\Users\\tester" },
    process: processMock,
    console: consoleMock,
    packageJson: { version: packageVersion },
    installDir,
    execFileSync,
    spawnSync,
  });

  return { launcher, logs, errors };
}

test("launcher runs installer when installed binary version lags wrapper version", () => {
  const installDir = winPath.join("C:\\Users\\tester", ".tokenizor", "bin");
  const binPath = winPath.join(installDir, "tokenizor-mcp.exe");
  const pendingPath = winPath.join(installDir, "tokenizor-mcp.pending.exe");
  const fsOverrides = createFs({ binPath, pendingPath });
  const execCalls = [];
  let versionCalls = 0;

  const { launcher, errors } = createLauncherForTest({
    fsOverrides,
    installDir,
    execFileSync(command, args) {
      execCalls.push({ command, args });
      if (command === binPath) {
        versionCalls += 1;
        return versionCalls === 1 ? "tokenizor 0.3.11" : "tokenizor 0.3.12";
      }
      return "";
    },
    spawnSync() {
      return { status: 0 };
    },
  });

  const status = launcher.main(["--version"]);

  assert.equal(status, 0);
  assert.equal(execCalls[1].command, "C:\\node\\node.exe");
  assert.match(errors.join("\n"), /does not match wrapper version 0.3.12/);
});

test("launcher applies pending update before checking installed version", () => {
  const installDir = winPath.join("C:\\Users\\tester", ".tokenizor", "bin");
  const binPath = winPath.join(installDir, "tokenizor-mcp.exe");
  const pendingPath = winPath.join(installDir, "tokenizor-mcp.pending.exe");
  const fsOverrides = createFs({
    binPath,
    pendingPath,
    hasBinary: true,
    hasPending: true,
  });

  const { launcher, errors } = createLauncherForTest({
    fsOverrides,
    installDir,
    execFileSync(command) {
      if (command === binPath) {
        return "tokenizor 0.3.12";
      }
      throw new Error("installer should not run");
    },
    spawnSync() {
      return { status: 0 };
    },
  });

  const status = launcher.main([]);

  assert.equal(status, 0);
  assert.equal(fsOverrides.renames.length, 1);
  assert.match(errors.join("\n"), /applied pending update/);
});

test("launcher honors TOKENIZOR_HOME for binary resolution", () => {
  const installDir = winPath.join("D:\\sandbox", "tokenizor-home", "bin");
  const binPath = winPath.join(installDir, "tokenizor-mcp.exe");
  const pendingPath = winPath.join(installDir, "tokenizor-mcp.pending.exe");
  const fsOverrides = createFs({ binPath, pendingPath, hasBinary: false, hasPending: false });

  const { launcher } = createLauncherForTest({
    fsOverrides,
    installDir: undefined,
    env: { TOKENIZOR_HOME: winPath.join("D:\\sandbox", "tokenizor-home") },
    execFileSync() {
      return "";
    },
    spawnSync() {
      return { status: 0 };
    },
  });

  assert.equal(launcher.getBinaryPath(), binPath);
  assert.equal(launcher.getPendingPath(), pendingPath);
});

test("launcher relays installer stdout to stderr so MCP stdout stays clean", () => {
  const installDir = winPath.join("C:\\Users\\tester", ".tokenizor", "bin");
  const binPath = winPath.join(installDir, "tokenizor-mcp.exe");
  const pendingPath = winPath.join(installDir, "tokenizor-mcp.pending.exe");
  const fsOverrides = createFs({ binPath, pendingPath });

  const { launcher, logs, errors } = createLauncherForTest({
    fsOverrides,
    installDir,
    execFileSync(command) {
      if (command === binPath) {
        return "tokenizor 0.3.11";
      }
      return "Downloading tokenizor-mcp v0.3.12...\nInstalled: C:\\Users\\tester\\.tokenizor\\bin\\tokenizor-mcp.exe\n";
    },
    spawnSync() {
      return { status: 0 };
    },
  });

  const status = launcher.main([]);

  assert.equal(status, 0);
  assert.equal(logs.length, 0);
  assert.match(errors.join("\n"), /Downloading tokenizor-mcp v0.3.12/);
  assert.match(errors.join("\n"), /Installed:/);
});
