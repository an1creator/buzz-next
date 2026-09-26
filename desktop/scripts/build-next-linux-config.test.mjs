import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import {
  mkdtempSync,
  mkdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const script = fileURLToPath(
  new URL("./build-next-linux-config.mjs", import.meta.url),
);
function build(endpoint, key = "fixture-public-key", artifacts = true) {
  const dir = mkdtempSync(join(tmpdir(), "buzz-next-linux-config-"));
  try {
    mkdirSync(join(dir, "src-tauri"));
    writeFileSync(
      join(dir, "src-tauri/tauri.release.conf.json"),
      JSON.stringify({
        bundle: { createUpdaterArtifacts: artifacts },
        plugins: { updater: { pubkey: key, endpoints: [endpoint] } },
      }),
    );
    writeFileSync(
      join(dir, "src-tauri/tauri.conf.json"),
      JSON.stringify({
        bundle: {
          externalBin: [
            "binaries/buzz-acp",
            "binaries/buzz-backend-kubernetes",
          ],
        },
      }),
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
              readFileSync(
                join(dir, "src-tauri/tauri.next.linux.conf.json"),
                "utf8",
              ),
            )
          : null,
    };
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

test("Linux AppImage retains Buzz Next identity, sidecars and signed private feed", () => {
  const result = build("https://buzz.n1creator.com/updates/linux/latest.json");
  assert.equal(result.status, 0);
  assert.equal(result.config.identifier, "com.n1creator.buzz-next");
  assert.equal(result.config.productName, "Buzz Next");
  assert.equal(result.config.bundle.createUpdaterArtifacts, true);
  assert.deepEqual(result.config.bundle.externalBin, [
    "binaries/buzz-acp",
    "binaries/buzz-backend-kubernetes",
    "binaries/buzz-ssh-askpass",
  ]);
});

test("Linux release overlay rejects another feed, missing key or unsigned bundle", () => {
  assert.notEqual(build("https://upstream.example/latest.json").status, 0);
  assert.notEqual(
    build("https://buzz.n1creator.com/updates/linux/latest.json", "").status,
    0,
  );
  assert.notEqual(
    build(
      "https://buzz.n1creator.com/updates/linux/latest.json",
      "fixture",
      false,
    ).status,
    0,
  );
});
