import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { homedir, tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../', import.meta.url));
process.chdir(root);

const packagePath = 'packages/llt-wasm/package.json';
const distDir = 'packages/llt-wasm';
const distFiles = ['llt.js', 'llt.d.ts', 'llt_bg.wasm', 'llt_bg.wasm.d.ts'];
const toolsDir = join(root, 'target/wasm-tools');

function run(command, args, options = {}) {
  return execFileSync(command, args, { stdio: 'inherit', ...options });
}

function packages() {
  const output = run('cargo', ['metadata', '--no-deps', '--format-version', '1', '--locked'], {
    stdio: ['ignore', 'pipe', 'inherit'],
    encoding: 'utf8',
  });
  return JSON.parse(output).packages;
}

function checkVersions() {
  const crates = packages();
  const version = crates.find(pkg => pkg.name === 'llt').version;
  assert.equal(crates.find(pkg => pkg.name === 'llt-wasm').version, version, 'Rust versions differ');
  assert.equal(JSON.parse(readFileSync(packagePath, 'utf8')).version, version, 'npm version differs');
  console.log(`All packages: ${version}`);
}

function setVersion(version) {
  // SemVer validation also keeps workflow input safe in TOML and branch names.
  const semver = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-((?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*)(?:\.(?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*))*))?(?:\+([0-9a-zA-Z-]+(?:\.[0-9a-zA-Z-]+)*))?$/;
  assert.ok(typeof version === 'string' && semver.test(version), 'Expected a SemVer version without a v prefix');
  const manifest = readFileSync('Cargo.toml', 'utf8');
  const versionField = /^(version = ")[^"]+("\r?$)/gm;
  assert.equal([...manifest.matchAll(versionField)].length, 1, 'Expected one workspace version');
  writeFileSync('Cargo.toml', manifest.replace(versionField, `$1${version}$2`));
  const pkg = readFileSync(packagePath, 'utf8');
  writeFileSync(packagePath, pkg.replace(/("version": ")[^"]+("),/, `$1${version}$2,`));
  run('cargo', ['update', '--workspace']);
  checkVersions();
}

function installBindgen() {
  const wasm = packages().find(pkg => pkg.name === 'llt-wasm');
  const requirement = wasm.dependencies.find(dep => dep.name === 'wasm-bindgen').req;
  assert.ok(requirement.startsWith('='), 'wasm-bindgen must use an exact version');
  run('cargo', ['install', 'wasm-bindgen-cli', '--version', requirement, '--locked', '--root', toolsDir]);
}

function buildDist(outDir) {
  checkVersions();
  const cargoHome = process.env.CARGO_HOME ?? join(homedir(), '.cargo');
  // Panic locations must not embed machine-specific checkout or registry paths.
  const flags = [`--remap-path-prefix=${root}=/workspace/`, `--remap-path-prefix=${cargoHome}=/cargo`];
  const targetDir = join(root, 'target/wasm-dist');
  run('cargo', ['build', '-p', 'llt-wasm', '--release', '--locked',
    '--target', 'wasm32-unknown-unknown', '--target-dir', targetDir], {
    env: { ...process.env, CARGO_ENCODED_RUSTFLAGS: flags.join('\x1f') },
  });
  run(join(toolsDir, 'bin/wasm-bindgen'), ['--target', 'web', '--out-dir', outDir,
    '--out-name', 'llt', join(targetDir, 'wasm32-unknown-unknown/release/llt_wasm.wasm')]);
}

function checkDist() {
  const temp = mkdtempSync(join(tmpdir(), 'llt-dist-'));
  try {
    buildDist(temp);
    const stale = distFiles.filter(file => !readFileSync(join(temp, file)).equals(readFileSync(join(distDir, file))));
    assert.deepEqual(stale, [], 'Dist is stale. Run node scripts/release.mjs build and commit the output');
    console.log('Dist is up to date');
  } finally {
    rmSync(temp, { recursive: true });
  }
}

switch (process.argv[2]) {
  case 'set-version': setVersion(process.argv[3]); break;
  case 'check-versions': checkVersions(); break;
  case 'install-bindgen': installBindgen(); break;
  case 'build': buildDist(distDir); break;
  case 'check-dist': checkDist(); break;
  default: throw new Error('Usage: node scripts/release.mjs <set-version VERSION|check-versions|install-bindgen|build|check-dist>');
}
