#!/usr/bin/env bash
# Searches the built UI (app/ui/dist/assets/*.js) for what only the mock IPC or a preset file
# holds. Run from the repository root, after `npm run build` in app/ (CI's `app` job runs it twice:
# after that build, and again after `tauri build`, which builds the UI once more).
#
# The mock and the presets serve the tests and the visual check only. main.tsx imports the mock
# behind VITE_MOCK_IPC, which is unset in the real build, so the bundle drops it. A hit means one
# of them was bundled: the mock's sample machine (`TESTPC`), its sample home (`C:\Users\tester`,
# written with two backslashes in a JavaScript string and searched with one as well), or the id
# of any preset in presets/*.cmdtheme.
#
# Exit 0: nothing found. Exit 1: a hit, or anything that stops the search from proving its point
# (no bundle, a failed positive control, a preset without a usable id, grep itself failing).
set -euo pipefail
shopt -s nullglob

files=(app/ui/dist/assets/*.js)
if [ "${#files[@]}" -eq 0 ]; then
  echo "::error::app/ui/dist/assets holds no .js file: the UI build wrote nothing to check"
  exit 1
fi

# grep over the bundles with the given options. Returns 0 for a hit and 1 for none; any other exit
# status of grep (2 is an error) ends the script, so a broken search never reads as "nothing found".
bundle_grep() {
  local status=0
  grep -a "$@" "${files[@]}" || status=$?
  if [ "$status" -gt 1 ]; then
    echo "::error::grep failed with exit status $status"
    exit 1
  fi
  return "$status"
}

# The positive control: `CaskaydiaCove NF` is in the UI's own texts, so a bundle that does not hold
# it (an empty or misplaced build, a wrong path) fails here instead of passing with "nothing found".
if ! bundle_grep -q -F -e 'CaskaydiaCove NF'; then
  echo "::error::positive control failed: 'CaskaydiaCove NF' is not in app/ui/dist/assets/*.js, so this search would find nothing even if the mock were bundled"
  exit 1
fi

# The id of every preset. An id that is missing or short would match unrelated text (or nothing),
# so node ends with an error unless each is a string of at least 8 characters.
preset_ids="$(node - <<'JS' | tr -d '\r'
const fs = require("node:fs");
const dir = "presets";
const names = fs.readdirSync(dir).filter((n) => n.endsWith(".cmdtheme")).sort();
if (names.length === 0) {
  console.error(`::error::${dir} holds no .cmdtheme file`);
  process.exit(1);
}
for (const name of names) {
  const id = JSON.parse(fs.readFileSync(`${dir}/${name}`, "utf8")).id;
  if (typeof id !== "string" || id.length < 8) {
    console.error(`::error::${dir}/${name} has no usable id (a string of 8 or more characters)`);
    process.exit(1);
  }
  console.log(id);
}
JS
)"

needles=('TESTPC' 'C:\\Users\\tester' 'C:\Users\tester')
while IFS= read -r id; do
  needles+=("$id")
done <<< "$preset_ids"

leaked=0
for needle in "${needles[@]}"; do
  # -o -b prints each hit as file:offset:text; a bundle is one huge line, so -n would print all of it.
  if bundle_grep -o -b -F -e "$needle"; then
    echo "::error::the built UI contains '$needle': the mock IPC or a preset file was bundled"
    leaked=1
  fi
done
if [ "$leaked" -ne 0 ]; then
  exit 1
fi
echo "no mock data and none of ${#needles[@]} searched strings in ${#files[@]} bundled .js file(s)"
