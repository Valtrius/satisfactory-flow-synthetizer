"""Build and freeze the native/browser comparison without launching any solves."""
import argparse
import hashlib
import json
import os
import re
from pathlib import Path
import shutil
import subprocess
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def run(args, cwd=ROOT):
    print(' '.join(map(str, args)), flush=True)
    return subprocess.run(list(map(str, args)), cwd=cwd, check=True, text=True)


def output(args):
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def write(path, data):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data, indent=2) + '\n', encoding='utf-8')


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def validate_suite(suite):
    """Reject unsupported budgets before building or creating a run directory."""
    def integer(value, minimum, maximum):
        return type(value) is int and minimum <= value <= maximum

    if not integer(suite.get('session_seconds'), 1, 10800):
        raise ValueError('Session budget must be between 1 and 10800 seconds')
    if not integer(suite.get('repeats'), 2, 100) or suite['repeats'] % 2:
        raise ValueError('Use an even number of repeats for balanced order')
    for field in ['cleanup_seconds', 'verification_reserve_seconds']:
        if not integer(suite.get(field), 1, 600):
            raise ValueError(f'Invalid {field}')
    cases = suite.get('cases')
    if not isinstance(cases, list) or not cases:
        raise ValueError('At least one case is required')
    scopes = set()
    for case in cases:
        if not re.fullmatch(r'[a-zA-Z0-9_-]+', case.get('case', '')):
            raise ValueError('Case names must be simple filenames')
        if case.get('mode') not in ['one_min_nl', 'all_min_nl', 'all_min_n']:
            raise ValueError('Unknown solve mode')
        if not integer(case.get('seconds'), 1, 10800) or not integer(case.get('max_nodes'), 1, 1000):
            raise ValueError('Invalid case deadline or node bound')
        workers = case.get('workers')
        if not isinstance(workers, list) or not workers or any(not integer(n, 8, 256) for n in workers):
            raise ValueError('Performance runs require at least 8 workers')
        for count in workers:
            scope = (case['case'], case['mode'], count)
            if scope in scopes:
                raise ValueError('Duplicate case/mode/worker configuration')
            scopes.add(scope)
        required = 3 * (case['seconds'] + suite['cleanup_seconds'] + 30) + suite['verification_reserve_seconds']
        if required > suite['session_seconds']:
            raise ValueError('A matched triple cannot fit the session budget')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', required=True)
    parser.add_argument('--suite', default='benchmarks/platform/suite.json')
    parser.add_argument('--baseline', help='Override the suite native reference revision')
    args = parser.parse_args()
    if os.name != 'nt':
        parser.error('This launcher currently requires Windows, PowerShell 7 and installed Chrome')
    suite = json.loads((ROOT / args.suite).read_text(encoding='utf-8'))
    if args.baseline:
        suite['baseline'] = args.baseline
    validate_suite(suite)
    baseline = output(['git', 'rev-parse', '--verify', f"{suite['baseline']}^{{commit}}"])
    current = output(['git', 'rev-parse', 'HEAD'])
    dest = (ROOT / args.output).resolve()
    dest.mkdir(parents=True, exist_ok=False)
    (dest / 'BENCHMARK-STATUS.txt').write_text('PREPARING: no solver benchmarks launched.\n')
    # Freeze the complete tracked tree and new benchmark tools, including any
    # local changes. Builds and the bundle use this snapshot exclusively.
    sources = dest / 'sources'
    candidate = sources / 'current'
    files = output(['git', 'ls-files', '-z']).split('\0')
    files += output(['git', 'ls-files', '--others', '--exclude-standard', '-z', '--', 'crates', 'frontend', 'src-tauri', 'vendor', 'scripts', 'benchmarks']).split('\0')
    for name in sorted(set(files)):
        if not name or not (ROOT / name).is_file():
            continue
        target = candidate / name
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(ROOT / name, target)
    archive = sources / 'baseline.zip'
    run(['git', 'archive', '--format=zip', f'--output={archive}', baseline])
    control = sources / 'baseline'
    with zipfile.ZipFile(archive) as bundle:
        bundle.extractall(control)
    binaries = dest / 'bin'
    binaries.mkdir(exist_ok=True)
    build_target = ROOT / 'target/platform-benchmark-native-build'
    for variant, source in [('native_baseline', control), ('native_current', candidate)]:
        run(['cargo', 'build', '--release', '--locked', '-j', '2', '-p', 'synthetizer-app', '--example', 'profile_solver', '--target-dir', build_target], cwd=source)
        shutil.copy2(build_target / 'release/examples/profile_solver.exe', binaries / f'{variant}.exe')
    run(['cargo', 'build', '--release', '--locked', '-j', '2', '-p', 'solver-portable-tests', '--example', 'verify_browser_jobs', '--target-dir', build_target], cwd=candidate)
    shutil.copy2(build_target / 'release/examples/verify_browser_jobs.exe', binaries / 'verify_browser_jobs.exe')
    backend = ROOT / 'src-tauri/binaries/cvc5-x86_64-pc-windows-msvc.exe'
    shutil.copy2(backend, binaries / 'cvc5.exe')
    backend_version = output([str(backend), '--version'])
    native_package = json.loads((candidate / 'src-tauri/cvc5-package.json').read_text())
    if not re.search(r'\bcvc5\s+(?:version\s+)?' + re.escape(native_package['version']) + r'\b', backend_version):
        raise RuntimeError(f'Unexpected bundled backend: {backend_version}')
    # Rebuild current Rust Wasm from the frozen tree with existing local tools.
    # Dependencies and compiler caches are shared; output assets belong to this run.
    wasm_tools = ROOT / 'target/web-tools'
    shutil.copytree(wasm_tools, candidate / 'target/web-tools', dirs_exist_ok=True)
    env = os.environ.copy()
    env['CARGO_TARGET_DIR'] = str(ROOT / 'target')
    subprocess.run(['cargo', 'build', '-p', 'solver-browser', '--target', 'wasm32-unknown-unknown', '--release', '--locked', '-j', '2'], cwd=candidate, env=env, check=True)
    lock = (candidate / 'Cargo.lock').read_text()
    bindgen_version = re.search(r'name = "wasm-bindgen"\s+version = "([^"]+)"', lock).group(1)
    bindgen = candidate / f'target/web-tools/wasm-bindgen-{bindgen_version}-x86_64-pc-windows-msvc/wasm-bindgen.exe'
    if output([str(bindgen), '--version']) != f'wasm-bindgen {bindgen_version}':
        raise RuntimeError('The cached wasm-bindgen executable must match Cargo.lock')
    solver_output = candidate / 'target/web-backends/solver'
    solver_output.mkdir(parents=True, exist_ok=True)
    run([bindgen, '--target', 'web', '--out-dir', solver_output, ROOT / 'target/wasm32-unknown-unknown/release/solver_browser.wasm'], cwd=candidate)
    shutil.copytree(ROOT / 'target/web-backends/cvc5', candidate / 'target/web-backends/cvc5', dirs_exist_ok=True)
    # Junction to installed build dependencies only; their lockfile and the
    # benchmark runtime dependency itself are frozen below.
    if not (candidate / 'node_modules').exists():
        run(['cmd.exe', '/c', 'mklink', '/J', str(candidate / 'node_modules'), str(ROOT / 'node_modules')])
    run([ROOT / 'node_modules/.bin/vite.cmd', 'build', '--config', candidate / 'benchmarks/platform/vite.config.mts', '--outDir', dest / 'web'], cwd=candidate)
    for name in ['run-platform-benchmarks.mjs', 'platform-benchmark-report.mjs', 'start-platform-benchmarks.ps1']:
        shutil.copy2(candidate / 'scripts' / name, dest / name)
    shutil.copytree(ROOT / 'node_modules/playwright-core', dest / 'node_modules/playwright-core', dirs_exist_ok=True)
    cases = {}
    for item in suite['cases']:
        file = candidate / 'benchmarks/cases' / (item['case'] + '.json')
        cases[item['case']] = json.loads(file.read_text())
        write(dest / 'cases' / file.name, cases[item['case']])
    schedule = []
    for repeat in range(1, suite['repeats'] + 1):
        for item in suite['cases']:
            for workers in item['workers']:
                variants = ['native_baseline', 'native_current', 'web_current']
                if repeat % 2 == 0:
                    variants.reverse()
                for variant in variants:
                    job = {**item, 'workers': workers, 'repeat': repeat, 'variant': variant, 'problem': cases[item['case']]['problem']}
                    job['id'] = f"{item['case']}-{item['mode']}-w{workers}-r{repeat}-{variant}"
                    schedule.append(job)
    write(dest / 'schedule.json', schedule)
    chrome_paths = [Path(os.environ.get('PROGRAMFILES', 'C:/Program Files')) / 'Google/Chrome/Application/chrome.exe', Path(os.environ.get('LOCALAPPDATA', '')) / 'Google/Chrome/Application/chrome.exe']
    chrome = next(p for p in chrome_paths if p.is_file())
    preflight = json.loads(output(['node', str(dest / 'run-platform-benchmarks.mjs'), '--preflight', str(dest), str(chrome)]))
    if preflight['hardware_concurrency'] < max(j['workers'] for j in schedule):
        raise RuntimeError('Browser reports too few logical processors')
    machine = output(['pwsh', '-NoProfile', '-Command', 'Get-CimInstance Win32_Processor | Select-Object Name,NumberOfCores,NumberOfLogicalProcessors | ConvertTo-Json'])
    topology = dest / 'topology.json'
    run(['pwsh', '-NoProfile', '-Command', f". '{ROOT / 'scripts/benchmark-affinity.ps1'}'; Save-BenchmarkTopology '{topology}'"])
    metadata = {'baseline_revision': baseline, 'current_revision': current, 'working_tree': output(['git', 'status', '--short']), 'suite': suite, 'native_cvc5_version': backend_version,
                'browser_executable': str(chrome), 'browser_sha256': digest(chrome), 'browser_preflight': preflight, 'machine': json.loads(machine), 'rustc': output(['rustc', '-Vv']), 'node': output(['node', '--version']),
                'timing_cap_sum_seconds': sum(j['seconds'] for j in schedule), 'scheduled_runs': len(schedule), 'source_note': 'current snapshot includes working tree benchmark harness changes; baseline is the exact Git tree'}
    write(dest / 'metadata.json', metadata)
    # The junction is not an artifact and must not be traversed or hashed. The
    # actual runtime dependency is copied under dest/node_modules.
    frozen = {}
    for base in [dest / 'bin', dest / 'web', dest / 'cases', dest / 'node_modules', sources]:
        for directory, dirs, names in os.walk(base, followlinks=False):
            dirs[:] = [d for d in dirs if not (Path(directory) / d).is_junction()]
            for name in names:
                path = Path(directory) / name
                frozen[path.relative_to(dest).as_posix()] = digest(path)
    for name in ['metadata.json', 'schedule.json', 'topology.json', 'run-platform-benchmarks.mjs', 'platform-benchmark-report.mjs', 'start-platform-benchmarks.ps1']:
        frozen[name] = digest(dest / name)
    write(dest / 'frozen-hashes.json', frozen)
    (dest / 'BENCHMARK-STATUS.txt').write_text(f'PREPARED: {len(schedule)} runs; no benchmark solves launched.\n')
    print(f'Prepared {dest}', flush=True)


if __name__ == '__main__':
    main()
