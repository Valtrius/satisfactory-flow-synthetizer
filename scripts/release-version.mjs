import { readFile, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const rootDir = fileURLToPath(new URL('../', import.meta.url));
const version = process.argv[2];

const CARGO_PACKAGE_NAMES = [
  'satisfactory-flow-synthetizer',
  'custom-solver-adapter',
  'solver-api',
  'solver-core',
  'solver-db',
  'solver-reference',
  'solver-validation',
  'solver-z3',
];

if (process.argv.length !== 3 || !isSemver(version)) {
  console.error('Usage: npm run release:version -- <version>');
  console.error('Example: npm run release:version -- 0.1.0');
  process.exitCode = 1;
} else {
  await updateVersion(version);
}

function isSemver(value) {
  if (typeof value !== 'string') {
    return false;
  }

  const match =
    /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-([^+]+))?(?:\+(.+))?$/.exec(
      value,
    );
  if (!match) {
    return false;
  }

  const [, , , , prerelease, build] = match;
  return validIdentifiers(prerelease, true) && validIdentifiers(build, false);
}

function validIdentifiers(value, rejectLeadingZero) {
  if (value === undefined) {
    return true;
  }

  return value.split('.').every((identifier) => {
    if (!/^[0-9A-Za-z-]+$/.test(identifier)) {
      return false;
    }

    return !(
      rejectLeadingZero &&
      /^\d+$/.test(identifier) &&
      identifier.length > 1 &&
      identifier.startsWith('0')
    );
  });
}

async function updateVersion(nextVersion) {
  const paths = {
    package: 'package.json',
    packageLock: 'package-lock.json',
    frontendPackage: 'frontend/package.json',
    frontendPackageLock: 'frontend/package-lock.json',
    tauri: 'src-tauri/tauri.conf.json',
    cargoWorkspace: 'Cargo.toml',
    cargoSolverApi: 'crates/solver-api/Cargo.toml',
    cargoLock: 'Cargo.lock',
  };

  const entries = await Promise.all(
    Object.entries(paths).map(async ([key, path]) => [
      key,
      path,
      normalizeLineEndings(await readFile(resolve(rootDir, path), 'utf8')),
    ]),
  );
  const files = Object.fromEntries(
    entries.map(([key, path, contents]) => [key, { path, contents }]),
  );

  const packageJson = parseJson(files.package);
  const packageLock = parseJson(files.packageLock);
  const frontendPackageJson = parseJson(files.frontendPackage);
  const frontendPackageLock = parseJson(files.frontendPackageLock);
  const tauriConfig = parseJson(files.tauri);

  if (!packageLock.packages?.['']) {
    throw new Error(
      'package-lock.json does not contain a root packages[""] entry',
    );
  }
  if (!packageLock.packages?.frontend) {
    throw new Error(
      'package-lock.json does not contain a packages["frontend"] entry',
    );
  }
  if (!frontendPackageLock.packages?.['']) {
    throw new Error(
      'frontend/package-lock.json does not contain a root packages[""] entry',
    );
  }

  assertVersion(packageJson.version, files.package.path);
  assertVersion(packageLock.version, files.packageLock.path);
  assertVersion(
    packageLock.packages[''].version,
    'package-lock.json packages[""]',
  );
  assertVersion(
    packageLock.packages.frontend.version,
    'package-lock.json packages["frontend"]',
  );
  assertVersion(frontendPackageJson.version, files.frontendPackage.path);
  assertVersion(frontendPackageLock.version, files.frontendPackageLock.path);
  assertVersion(
    frontendPackageLock.packages[''].version,
    'frontend/package-lock.json packages[""]',
  );
  assertVersion(tauriConfig.version, files.tauri.path);

  const updates = [
    [
      files.package,
      replaceJsonVersion(
        files.package.contents,
        files.package.path,
        nextVersion,
      ),
    ],
    [
      files.packageLock,
      updateRootPackageLockVersion(files.packageLock.contents, nextVersion),
    ],
    [
      files.frontendPackage,
      replaceJsonVersion(
        files.frontendPackage.contents,
        files.frontendPackage.path,
        nextVersion,
      ),
    ],
    [
      files.frontendPackageLock,
      updatePackageLockVersion(
        files.frontendPackageLock.contents,
        nextVersion,
        files.frontendPackageLock.path,
      ),
    ],
    [
      files.tauri,
      replaceJsonVersion(files.tauri.contents, files.tauri.path, nextVersion),
    ],
    [
      files.cargoWorkspace,
      updateSectionVersion(
        files.cargoWorkspace.contents,
        '[workspace.package]',
        nextVersion,
        files.cargoWorkspace.path,
      ),
    ],
    [
      files.cargoSolverApi,
      updateSectionVersion(
        files.cargoSolverApi.contents,
        '[package]',
        nextVersion,
        files.cargoSolverApi.path,
      ),
    ],
    [
      files.cargoLock,
      updateCargoLockVersions(files.cargoLock.contents, nextVersion),
    ],
  ];

  await Promise.all(
    updates.map(([file, contents]) =>
      writeFile(resolve(rootDir, file.path), contents),
    ),
  );

  console.log(
    `Set satisfactory-flow-synthetizer version to ${nextVersion} in:`,
  );
  for (const [file] of updates) {
    console.log(`- ${file.path}`);
  }
}

function normalizeLineEndings(contents) {
  return contents.replace(/\r\n?/g, '\n');
}

function parseJson(file) {
  try {
    return JSON.parse(file.contents);
  } catch (error) {
    throw new Error(`Could not parse ${file.path}: ${error.message}`, {
      cause: error,
    });
  }
}

function assertVersion(value, label) {
  if (typeof value !== 'string') {
    throw new Error(`Expected a version string in ${label}`);
  }
}

function replaceJsonVersion(contents, path, nextVersion, indent = '  ') {
  const pattern = new RegExp(`^${indent}"version"\\s*:\\s*"[^"]+"(,?)$`, 'gm');
  const matches = [...contents.matchAll(pattern)];
  if (matches.length !== 1) {
    throw new Error(`Expected one version in ${path}, found ${matches.length}`);
  }

  return contents.replace(pattern, `${indent}"version": "${nextVersion}"$1`);
}

function updatePackageLockVersion(contents, nextVersion, path) {
  const rootMarker = '    "": {';
  const rootStart = contents.indexOf(rootMarker);
  if (rootStart === -1) {
    throw new Error(`Could not find packages[""] in ${path}`);
  }

  const rootEnd = contents.indexOf('\n    "', rootStart + rootMarker.length);
  if (rootEnd === -1) {
    throw new Error(`Could not find the end of packages[""] in ${path}`);
  }

  const rootPackage = contents.slice(rootStart, rootEnd);
  const updatedRootPackage = replaceJsonVersion(
    rootPackage,
    `${path} packages[""]`,
    nextVersion,
    '      ',
  );
  const updatedContents =
    contents.slice(0, rootStart) + updatedRootPackage + contents.slice(rootEnd);

  return replaceJsonVersion(updatedContents, path, nextVersion);
}

function updateRootPackageLockVersion(contents, nextVersion) {
  let updated = updatePackageLockVersion(
    contents,
    nextVersion,
    'package-lock.json',
  );

  const frontendMarker = '    "frontend": {';
  const frontendStart = updated.indexOf(frontendMarker);
  if (frontendStart === -1) {
    throw new Error('Could not find packages["frontend"] in package-lock.json');
  }

  const frontendEnd = updated.indexOf(
    '\n    "',
    frontendStart + frontendMarker.length,
  );
  if (frontendEnd === -1) {
    throw new Error(
      'Could not find the end of packages["frontend"] in package-lock.json',
    );
  }

  const frontendPackage = updated.slice(frontendStart, frontendEnd);
  const updatedFrontendPackage = replaceJsonVersion(
    frontendPackage,
    'package-lock.json packages["frontend"]',
    nextVersion,
    '      ',
  );

  return (
    updated.slice(0, frontendStart) +
    updatedFrontendPackage +
    updated.slice(frontendEnd)
  );
}

function updateSectionVersion(contents, header, nextVersion, path) {
  const start = contents.indexOf(`${header}\n`);
  if (start === -1) {
    throw new Error(`Could not find ${header} in ${path}`);
  }

  const end = contents.indexOf('\n[', start + header.length + 1);
  const sectionEnd = end === -1 ? contents.length : end;
  const section = contents.slice(start, sectionEnd);
  const updated = replaceOneVersion(section, path);

  return contents.slice(0, start) + updated + contents.slice(sectionEnd);

  function replaceOneVersion(value, label) {
    const matches = [...value.matchAll(/^version\s*=\s*"[^"]+"$/gm)];
    if (matches.length !== 1) {
      throw new Error(
        `Expected one package version in ${label}, found ${matches.length}`,
      );
    }

    return value.replace(
      /^version\s*=\s*"[^"]+"$/m,
      `version = "${nextVersion}"`,
    );
  }
}

function updateCargoLockVersions(contents, nextVersion) {
  let updated = contents;

  for (const packageName of CARGO_PACKAGE_NAMES) {
    updated = updateCargoLockPackageVersion(updated, packageName, nextVersion);
  }

  return updated;
}

function updateCargoLockPackageVersion(contents, packageName, nextVersion) {
  const starts = [...contents.matchAll(/^\[\[package\]\]$/gm)].map(
    (match) => match.index,
  );
  const matchingBlocks = [];
  const namePattern = new RegExp(
    `^name\\s*=\\s*"${escapeRegExp(packageName)}"$`,
    'm',
  );

  for (let index = 0; index < starts.length; index += 1) {
    const start = starts[index];
    const end = starts[index + 1] ?? contents.length;
    const block = contents.slice(start, end);
    if (namePattern.test(block)) {
      matchingBlocks.push({ start, end, block });
    }
  }

  if (matchingBlocks.length !== 1) {
    throw new Error(
      `Expected one ${packageName} package in Cargo.lock, found ${matchingBlocks.length}`,
    );
  }

  const [{ start, end, block }] = matchingBlocks;
  const versionMatches = [...block.matchAll(/^version\s*=\s*"[^"]+"$/gm)];
  if (versionMatches.length !== 1) {
    throw new Error(
      `Expected one ${packageName} version in Cargo.lock, found ${versionMatches.length}`,
    );
  }

  const updated = block.replace(
    /^version\s*=\s*"[^"]+"$/m,
    `version = "${nextVersion}"`,
  );
  return contents.slice(0, start) + updated + contents.slice(end);
}

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}
