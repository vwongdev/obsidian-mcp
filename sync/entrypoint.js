import { spawn, spawnSync } from "node:child_process";

const OB = "/app/node_modules/obsidian-headless/cli.js";
const NOT_CONFIGURED = 3;

const vaultPath = process.env.VAULT_PATH;
const vaultName = process.env.VAULT_NAME;
const deviceName = process.env.DEVICE_NAME ?? "obsidian-mcp";
const password = process.env.VAULT_PASSWORD;

for (const name of ["OBSIDIAN_AUTH_TOKEN", "VAULT_NAME"]) {
  if (!process.env[name]) {
    console.error(`${name} must be set`);
    process.exit(1);
  }
}

function ob(args, stdio = "inherit") {
  return spawnSync(process.execPath, [OB, ...args], { stdio }).status;
}

function mustSucceed(args, description) {
  if (ob(args) !== 0) {
    console.error(`${description} failed`);
    process.exit(1);
  }
}

const status = ob(["sync-status", "--path", vaultPath, "--json"], "ignore");
if (status === NOT_CONFIGURED) {
  const setup = ["sync-setup", "--vault", vaultName, "--path", vaultPath, "--device-name", deviceName, "--json"];
  if (password) setup.push("--password", password);
  mustSucceed(setup, "Vault setup");
} else if (status !== 0) {
  console.error(`Reading sync status failed with exit code ${status}`);
  process.exit(1);
}

// Leaves the .obsidian config directory out of sync, so the vault holds notes only.
mustSucceed(["sync-config", "--path", vaultPath, "--configs", "", "--json"], "Disabling config sync");

const sync = spawn(process.execPath, [OB, "sync", "--path", vaultPath, "--continuous"], { stdio: "inherit" });
for (const signal of ["SIGTERM", "SIGINT"]) {
  process.on(signal, () => sync.kill(signal));
}
sync.on("exit", (code, signal) => process.exit(code ?? (signal ? 0 : 1)));
