import assert from "node:assert/strict";
import {
  mkdtempSync,
  mkdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import test from "node:test";
const script = fileURLToPath(
  new URL("./build-next-config.mjs", import.meta.url),
);
function build(endpoint, key = "fixture-public-key") {
  const dir = mkdtempSync(join(tmpdir(), "buzz-next-config-"));
  try {
    mkdirSync(join(dir, "src-tauri"));
    writeFileSync(
      join(dir, "src-tauri/tauri.release.conf.json"),
      JSON.stringify({
        bundle: { createUpdaterArtifacts: true },
        plugins: { updater: { pubkey: key, endpoints: [endpoint] } },
      }),
    );
    writeFileSync(
      join(dir, "src-tauri/tauri.windows.conf.json"),
      JSON.stringify({ bundle: { externalBin: ["binaries/buzz-acp"] } }),
    );
    const result = spawnSync(process.execPath, [script], {
      cwd: dir,
      encoding: "utf8",
    });
    return {
      status: result.status,
      config:
        result.status === 0
          ? JSON.parse(
              readFileSync(join(dir, "src-tauri/tauri.next.conf.json"), "utf8"),
            )
          : null,
    };
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}
test("Buzz Next retains app identity and the signed private update channel", () => {
  const result = build(
    "https://buzz.n1creator.com/updates/windows/latest.json",
  );
  assert.equal(result.status, 0);
  assert.equal(result.config.identifier, "com.n1creator.buzz-next");
  assert.equal(result.config.productName, "Buzz Next");
  assert.equal(result.config.bundle.createUpdaterArtifacts, true);
  assert.deepEqual(result.config.bundle.externalBin, [
    "binaries/buzz-acp",
    "binaries/buzz-ssh-askpass",
  ]);
});
test("wrong channel or missing public key cannot produce a release overlay", () => {
  assert.notEqual(build("https://upstream.example/latest.json").status, 0);
  assert.notEqual(
    build("https://buzz.n1creator.com/updates/windows/latest.json", "").status,
    0,
  );
});
