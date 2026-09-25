import { readFileSync, writeFileSync } from "node:fs";

// Keep upstream release config generic. This overlay belongs to Buzz Next only.
const release = JSON.parse(readFileSync("src-tauri/tauri.release.conf.json", "utf8"));
const windows = JSON.parse(readFileSync("src-tauri/tauri.windows.conf.json", "utf8"));
const updater = release.plugins?.updater;
if (
  !updater?.pubkey ||
  updater.endpoints?.length !== 1 ||
  updater.endpoints[0] !== "https://buzz.n1creator.com/updates/windows/latest.json"
) {
  throw new Error("Buzz Next requires its own signed update channel");
}
const config = {
  ...release,
  productName: "Buzz Next",
  identifier: "com.n1creator.buzz-next",
  bundle: {
    ...release.bundle,
    externalBin: [...windows.bundle.externalBin, "binaries/buzz-ssh-askpass"],
  },
};
writeFileSync("src-tauri/tauri.next.conf.json", `${JSON.stringify(config, null, 2)}\n`);
