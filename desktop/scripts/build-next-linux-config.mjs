import { readFileSync, writeFileSync } from "node:fs";

// The Linux release has its own updater feed and uses Linux sidecars from the
// base Tauri configuration. Keep the Windows release overlay independent.
const release = JSON.parse(
  readFileSync("src-tauri/tauri.release.conf.json", "utf8"),
);
const base = JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8"));
const updater = release.plugins?.updater;
if (
  !updater?.pubkey ||
  updater.endpoints?.length !== 1 ||
  updater.endpoints[0] !==
    "https://buzz.n1creator.com/updates/linux/latest.json" ||
  release.bundle?.createUpdaterArtifacts !== true ||
  !Array.isArray(base.bundle?.externalBin)
) {
  throw new Error(
    "Buzz Next Linux requires signed AppImage updates and Linux sidecars",
  );
}

const config = {
  ...release,
  productName: "Buzz Next",
  identifier: "com.n1creator.buzz-next",
  bundle: {
    ...release.bundle,
    externalBin: [...base.bundle.externalBin, "binaries/buzz-ssh-askpass"],
  },
};
writeFileSync(
  "src-tauri/tauri.next.linux.conf.json",
  `${JSON.stringify(config, null, 2)}\n`,
);
