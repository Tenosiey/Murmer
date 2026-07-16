// One-off helper for the runes migration: applies svelte/compiler's migrate()
// to every .svelte file under the directory given as argv[2].
import { migrate } from "svelte/compiler";
import { readFileSync, writeFileSync } from "node:fs";
import { globSync } from "node:fs";

const dir = process.argv[2];
if (!dir) {
  console.error("usage: node scripts/migrate-runes.mjs <dir>");
  process.exit(1);
}

const files = globSync(`${dir}/**/*.svelte`).sort();
let changed = 0;
const failed = [];
for (const file of files) {
  const source = readFileSync(file, "utf8");
  try {
    const { code } = migrate(source, { filename: file });
    if (code !== source) {
      writeFileSync(file, code);
      changed++;
      console.log(`migrated ${file}`);
    } else {
      console.log(`unchanged ${file}`);
    }
  } catch (e) {
    failed.push(file);
    console.error(`FAILED ${file}: ${e.message}`);
  }
}
console.log(`\n${changed}/${files.length} files migrated, ${failed.length} failed`);
if (failed.length) process.exit(2);
