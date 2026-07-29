import { copyFileSync, mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const apkName = process.argv[2] ?? "app-release.apk";
const source = resolve(root, "apps/companion/build/app/outputs/flutter-apk", apkName);
const destination = resolve(root, "apps/companion/releases/app-tester-companion.apk");

mkdirSync(dirname(destination), { recursive: true });
copyFileSync(source, destination);
