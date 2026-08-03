import { execFileSync } from "node:child_process";
import { mkdir, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { basename, join } from "node:path";

const packages = [
  ["bootstrap", "1.87.0"], ["boxicons-logos", "1.87.0"],
  ["boxicons-regular", "1.87.0"], ["boxicons-solid", "1.87.0"],
  ["crypto", "1.87.0"], ["entypo", "1.86.0"],
  ["entypo-social", "1.86.0"], ["evaicons-outline", "1.86.0"],
  ["evaicons-solid", "1.86.0"], ["evil", "1.86.0"],
  ["fa-brands", "1.87.0"], ["fa-regular", "1.87.0"],
  ["fa-solid", "1.87.0"], ["feather", "1.87.0"],
  ["fluentui-system-filled", "1.87.0"], ["fluentui-system-regular", "1.87.0"],
  ["foundation", "1.86.0"], ["heroicons-outline", "1.87.0"],
  ["heroicons-solid", "1.87.0"], ["icomoon", "1.86.0"],
  ["ionicons-outline", "1.86.0"], ["ionicons-sharp", "1.86.0"],
  ["ionicons-solid", "1.86.0"], ["material-filled", "1.87.0"],
  ["material-outlined", "1.87.0"], ["material-rounded", "1.87.0"],
  ["material-sharp", "1.87.0"], ["material-twotone", "1.87.0"],
  ["octicons", "1.87.0"], ["open-iconic", "1.86.0"],
  ["remix-editor", "1.86.0"], ["remix-fill", "1.86.0"],
  ["remix-line", "1.86.0"], ["simple-icons", "1.86.0"],
  ["typicons", "1.86.0"], ["zondicons", "1.86.0"],
];

// https://svglogos.dev brand logos, published under CC0-1.0 at gilbarbara/logos.
// Not distributed on npm, so it's fetched straight from a pinned GitHub commit.
const gitSources = [
  {
    pack: "svglogos",
    repository: "gilbarbara/logos",
    commit: "4de741f8503d5e81abf5dfa05214690e938296bf",
    svgDir: "logos",
  },
];

const cacheDir = ".cache/svg-icons-npm";
const gitCacheDir = ".cache/svg-icons-git";
const outputDir = "demo/icons";
await mkdir(cacheDir, { recursive: true });
await mkdir(gitCacheDir, { recursive: true });
await mkdir(outputDir, { recursive: true });

const icons = [];
for (const [pack, version] of packages) {
  const extractDir = join(cacheDir, `${pack}-${version}`);
  let packageDir = join(extractDir, "package");
  try {
    await readdir(packageDir);
  } catch {
    await rm(extractDir, { recursive: true, force: true });
    await mkdir(extractDir, { recursive: true });
    const npmCli = process.env.npm_execpath;
    if (!npmCli) throw new Error("Run this script through npm run prepare:icons");
    const packed = JSON.parse(execFileSync(
      process.execPath,
      [npmCli, "pack", `@svg-icons/${pack}@${version}`, "--pack-destination", cacheDir, "--json"],
      { encoding: "utf8" },
    ));
    execFileSync("tar", ["-xf", join(cacheDir, packed[0].filename), "-C", extractDir]);
  }

  const files = await readdir(packageDir);
  const svgFiles = files.filter((file) => file.endsWith(".svg")).sort();
  for (const file of svgFiles) {
    icons.push({
      name: basename(file, ".svg"),
      pack,
      svg: await readFile(join(packageDir, file), "utf8"),
    });
  }
  console.log(`${pack}: ${svgFiles.length} icons`);
}

for (const { pack, repository, commit, svgDir } of gitSources) {
  const extractDir = join(gitCacheDir, `${pack}-${commit.slice(0, 12)}`);
  let packageDir = join(extractDir, svgDir);
  try {
    await readdir(packageDir);
  } catch {
    await rm(extractDir, { recursive: true, force: true });
    await mkdir(extractDir, { recursive: true });
    const tarballPath = join(gitCacheDir, `${pack}-${commit.slice(0, 12)}.tar.gz`);
    const response = await fetch(`https://codeload.github.com/${repository}/tar.gz/${commit}`);
    if (!response.ok) {
      throw new Error(`Failed to download ${repository}@${commit}: ${response.status} ${response.statusText}`);
    }
    await writeFile(tarballPath, Buffer.from(await response.arrayBuffer()));
    execFileSync("tar", ["-xf", tarballPath, "-C", extractDir, "--strip-components=1"]);
  }

  const files = await readdir(packageDir);
  const svgFiles = files.filter((file) => file.endsWith(".svg")).sort();
  for (const file of svgFiles) {
    icons.push({
      name: basename(file, ".svg"),
      pack,
      svg: await readFile(join(packageDir, file), "utf8"),
    });
  }
  console.log(`${pack}: ${svgFiles.length} icons`);
}

await writeFile(join(outputDir, "catalog.json"), JSON.stringify(icons));
await writeFile(join(outputDir, "source.json"), JSON.stringify({
  generatedAt: new Date().toISOString(),
  iconCount: icons.length,
  sources: [
    {
      repository: "https://github.com/svg-icons/svg-icons",
      repositoryCommit: "135ccb6e1adc3bd58bd8f2282631379cd878fb00",
      packages: packages.map(([name, version]) => ({ name: `@svg-icons/${name}`, version })),
    },
    ...gitSources.map(({ pack, repository, commit }) => ({
      repository: `https://github.com/${repository}`,
      repositoryCommit: commit,
      packs: [pack],
    })),
  ],
}, null, 2));
console.log(`Wrote ${icons.length} icons to ${outputDir}/catalog.json`);
