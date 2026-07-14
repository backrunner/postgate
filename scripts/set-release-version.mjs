import { readFile, writeFile } from "node:fs/promises";

const version = process.argv[2]?.replace(/^v/, "");
const semverPattern = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/;

if (!version || !semverPattern.test(version)) {
  throw new Error(`Invalid release version: ${process.argv[2] ?? "<missing>"}`);
}

for (const path of ["package.json", "apps/desktop/package.json", "apps/desktop/src-tauri/tauri.conf.json"]) {
  const document = JSON.parse(await readFile(path, "utf8"));
  document.version = version;
  await writeFile(path, `${JSON.stringify(document, null, 2)}\n`);
}

const cargoPath = "apps/desktop/src-tauri/Cargo.toml";
const cargoManifest = await readFile(cargoPath, "utf8");
const nextCargoManifest = cargoManifest.replace(
  /(\[package\][\s\S]*?\nversion\s*=\s*)"[^"]+"/,
  `$1"${version}"`,
);

if (nextCargoManifest === cargoManifest) {
  throw new Error("Could not update the package version in Cargo.toml");
}

await writeFile(cargoPath, nextCargoManifest);
console.log(`Prepared PostGate v${version}`);
