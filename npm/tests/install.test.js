const test = require("node:test");
const assert = require("node:assert/strict");
const path = require("node:path");
const winPath = path.win32;

const { createInstaller } = require("../scripts/install.js");

function createFs({ binPath, pendingPath, installDir, binFailuresBeforeSuccess = 0 }) {
  let binFailuresRemaining = binFailuresBeforeSuccess;
  const writes = [];
  const chmods = [];
  const mkdirs = [];
  const unlinks = [];

  return {
    writes,
    chmods,
    mkdirs,
    unlinks,
    existsSync(target) {
      return target === binPath;
    },
    writeFileSync(target, data) {
      writes.push({ target, data: Buffer.from(data).toString("utf8") });
      if (target === binPath && binFailuresRemaining > 0) {
        binFailuresRemaining -= 1;
        const error = new Error("binary is busy");
        error.code = "EPERM";
        throw error;
      }
    },
    chmodSync(target, mode) {
      chmods.push({ target, mode });
    },
    mkdirSync(target, options) {
      mkdirs.push({ target, options });
      assert.equal(target, installDir);
    },
    unlinkSync(target) {
      unlinks.push(target);
      assert.equal(target, pendingPath);
    },
  };
}

function createInstallerForTest({ fsOverrides, execFileSync, sleep, installDir }) {
  const logs = [];
  const errors = [];
  const processMock = {
    platform: "win32",
    arch: "x64",
    env: {},
    exit(code) {
      throw new Error(`unexpected exit ${code}`);
    },
  };
  const consoleMock = {
    log(message) {
      logs.push(message);
    },
    error(message) {
      errors.push(message);
    },
  };

  const installer = createInstaller({
    fs: fsOverrides,
    path: winPath,
    os: { homedir: () => "C:\\Users\\tester" },
    process: processMock,
    console: consoleMock,
    packageJson: { version: "0.3.9" },
    installDir,
    execSync: () => "tokenizor 0.3.8",
    execFileSync,
    sleep: sleep || (async () => {}),
    download: async () => Buffer.from("new-binary"),
  });

  return { installer, logs, errors };
}

test("locked Windows binary is replaced after stopping running Tokenizor processes", async () => {
  const installDir = winPath.join("C:\\Users\\tester", ".tokenizor", "bin");
  const binPath = winPath.join(installDir, "tokenizor-mcp.exe");
  const pendingPath = winPath.join(installDir, "tokenizor-mcp.pending.exe");
  const fsOverrides = createFs({
    binPath,
    pendingPath,
    installDir,
    binFailuresBeforeSuccess: 1,
  });
  const execCalls = [];
  const { installer, logs } = createInstallerForTest({
    fsOverrides,
    installDir,
    execFileSync(command, args, options) {
      execCalls.push({ command, args, options });
      return "[101,202]";
    },
  });

  await installer.main();

  assert.equal(execCalls.length, 1);
  assert.equal(execCalls[0].command, "powershell.exe");
  assert.equal(
    fsOverrides.writes.filter((entry) => entry.target === binPath).length,
    2
  );
  assert.equal(
    fsOverrides.writes.some((entry) => entry.target === pendingPath),
    false
  );
  assert.match(logs.join("\n"), /Stopping 2 running Tokenizor process/);
  assert.match(logs.join("\n"), /Installed:/);
});

test("installer stages a pending binary when the executable is still locked after stopping processes", async () => {
  const installDir = winPath.join("C:\\Users\\tester", ".tokenizor", "bin");
  const binPath = winPath.join(installDir, "tokenizor-mcp.exe");
  const pendingPath = winPath.join(installDir, "tokenizor-mcp.pending.exe");
  const fsOverrides = createFs({
    binPath,
    pendingPath,
    installDir,
    binFailuresBeforeSuccess: 9,
  });
  const { installer, logs } = createInstallerForTest({
    fsOverrides,
    installDir,
    execFileSync() {
      return "[404]";
    },
  });

  await installer.main();

  assert.equal(
    fsOverrides.writes.some((entry) => entry.target === pendingPath),
    true
  );
  assert.match(logs.join("\n"), /Staged update at:/);
  assert.match(logs.join("\n"), /Update will apply automatically on next launch/);
});
