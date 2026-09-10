"""Build and verify isolated solver variants. Never run a timing screen or edit production sources."""
import argparse
import difflib
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tarfile
import time

REPO = Path(__file__).resolve().parents[1]
SPEC = REPO / "benchmarks/optimization"


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def same_source(first, second):
    # rustfmt may leave a preformatted Windows file's CRLF intact, but rewrite
    # another to LF. Frozen artifact hashes remain exact; reproduction permits
    # only this source line-ending difference.
    return first.read_bytes().replace(b"\r\n", b"\n") == second.read_bytes().replace(b"\r\n", b"\n")


def replace_once(text, before, after):
    if text.count(before) != 1:
        raise ValueError(f"Source anchor changed: {before[:100]!r}")
    return text.replace(before, after, 1)


def transform(sources, config):
    """Exact changes are compiled into each variant; no runtime or case-name dispatch."""
    sources = dict(sources)
    if "adaptive_grace_ms" in config and (not config.get("adaptive") or config["adaptive_grace_ms"] < 0):
        raise ValueError("Adaptive grace requires a nonnegative delay and the adaptive policy")
    if "root_order" in config:
        # Explicit ordering experiments start from ascending owner construction,
        # even when the production revision already constructs descending roots.
        descending = "for source in (0..self.problem.inputs.len() + task.profile.node_count() as usize).rev()"
        if descending in sources["lib.rs"]:
            sources["lib.rs"] = replace_once(sources["lib.rs"], descending,
                "for source in 0..self.problem.inputs.len() + task.profile.node_count() as usize")
    if "refine" in config or config.get("adaptive"):
        lib = sources["lib.rs"]
        has_second_source = "    second_source: Option<usize>," in lib
        if "fn refine_roots(" in lib:
            # Replace the production static scheduler, retaining its shared
            # encoding and diagnostics. Adaptive parents must start unsplit.
            start = lib.index("/// Children partition the parent")
            end = lib.index("struct RootLedger {", start)
            lib = lib[:start] + lib[end:]
            lib, count = re.subn(r"        let roots =\s+if self.options.mode == SolveMode::AllMinNL.*?\n            };\n", "", lib, count=1, flags=re.S)
            if count != 1:
                raise ValueError("Production static scheduling anchor changed")
        if not has_second_source:
            lib = replace_once(lib, "    source: Option<usize>,\n    impossible: bool,", "    source: Option<usize>,\n    second_source: Option<usize>,\n    impossible: bool,")
            # Every initial Root constructor has its source before impossible.
            lib, count = re.subn(r"(?m)^(\s+)source: (Some\([^\n]+\)|None),\n\1impossible", r"\1source: \2,\n\1second_source: None,\n\1impossible", lib)
            if count != 5:
                raise ValueError(f"Expected five Root constructors, got {count}")
        helper = "adaptive.rs" if config.get("adaptive") else "refine.rs"
        helper_source = (SPEC / helper).read_text()
        if config.get("adaptive"):
            helper_source = replace_once(helper_source, r'\"refinement_trigger_s\":{trigger}',
                r'\"refinement_trigger_s\":{trigger},\"refinement_grace_ms\":' + str(config.get("adaptive_grace_ms", 0)))
        if config.get("adaptive_grace_ms"):
            helper_source = replace_once(helper_source,
                "gate.filter(|_| active < workers && queue.is_empty())",
                f"gate.filter(|trigger| active < workers && queue.is_empty() && origin.elapsed().as_secs_f64() - *trigger >= Duration::from_millis({config['adaptive_grace_ms']}).as_secs_f64())")
        lib = replace_once(lib, "struct RootLedger {", helper_source + "\nstruct RootLedger {")
        if config.get("adaptive"):
            lib = replace_once(lib, "        let mut ledger = RootLedger::new(&roots, tasks.len());", """        if self.options.mode == SolveMode::AllMinNL && matches!(self.counts, Counts::Boolean)
            && self.options.worker_count > 1 && self.problem.outputs.len() >= 2 {
            return self.adaptive_group(nodes, links, tasks, &roots);
        }
        let mut ledger = RootLedger::new(&roots, tasks.len());""")
        else:
            gate = "" if config["refine"] == "both" else f" && matches!(self.counts, Counts::{config['refine'].title()})"
            lib = replace_once(lib, "        let mut ledger = RootLedger::new(&roots, tasks.len());", f"""        let roots = if self.options.mode == SolveMode::AllMinNL{gate} {{
            refine_roots(roots, tasks, self.problem.inputs.len(), self.problem.outputs.len(), self.options.worker_count)
        }} else {{ roots }};
        let mut ledger = RootLedger::new(&roots, tasks.len());""")
        if not has_second_source:
            lib = replace_once(lib, "                                root.source,\n", "                                root.source,\n                                root.second_source,\n")
            lib = replace_once(lib, "    source: Option<usize>,\n    mode: SolveMode,", "    source: Option<usize>,\n    second_source: Option<usize>,\n    mode: SolveMode,")
            lib = replace_once(lib, "session.write(&encoding.output_source_assertion(source)?)?;", "session.write(&encoding.output_source_assertion(0, source)?)?;")
            lib = replace_once(lib, "    let mut seen = BTreeSet::new();", "    if let Some(source) = second_source {\n        session.write(&encoding.output_source_assertion(1, source)?)?;\n    }\n    let mut seen = BTreeSet::new();")
            enc = replace_once(sources["encoding.rs"], "pub fn output_source_assertion(&self, source: usize)", "pub fn output_source_assertion(&self, output: usize, source: usize)")
            sources["encoding.rs"] = replace_once(enc, "edge.target == 0 && edge.source == source", "edge.target == output && edge.source == source")
            diag = replace_once(sources["diagnostics.rs"], "        format!(", "        let second_source = root_info.second_source.map_or_else(|| \"null\".to_owned(), |v| v.to_string());\n        format!(")
            sources["diagnostics.rs"] = replace_once(diag, r'\"source\":{source},', r'\"source\":{source},\"second_source\":{second_source},')
        sources["lib.rs"] = lib
    portfolio = sources["portfolio.rs"]
    if "sparse_quarters" in config or "solo" in config:
        if config.get("solo") == 0:
            portfolio = replace_once(portfolio, "return crate::run(problem, options, cancel, observer, Counts::Boolean);", "return crate::run(problem, options, cancel, observer, Counts::Sparse);")
        allocation = """if branch == 0 {
                    options.worker_count.div_ceil(2)
                } else {
                    options.worker_count / 2
                }"""
        if "solo" in config:
            expression = f"if branch == {config['solo']} {{ options.worker_count }} else {{ 0 }}"
        else:
            sparse = "options.worker_count.div_ceil(4)" if config["sparse_quarters"] == 1 else "options.worker_count - options.worker_count / 4"
            expression = f"if branch == 0 {{ ({sparse}).clamp(1, options.worker_count - 1) }} else {{ options.worker_count - ({sparse}).clamp(1, options.worker_count - 1) }}"
        portfolio = replace_once(portfolio, allocation, expression)
        if "solo" in config:
            portfolio = replace_once(portfolio, "            handles.push(scope.spawn(move || {", "            if options.worker_count == 0 { continue; }\n            handles.push(scope.spawn(move || {")
    if "delay_ms" in config:
        portfolio = replace_once(portfolio, "                let result = crate::run(problem, &options, stop, &bridge, counts);", f"""                if branch == {config['delay_branch']} {{
                    let wait = Instant::now();
                    while !stop.load(Ordering::Relaxed) && wait.elapsed() < Duration::from_millis({config['delay_ms']}) {{
                        thread::sleep(Duration::from_millis(5));
                    }}
                }}
                let result = crate::run(problem, &options, stop, &bridge, counts);""")
    sources["portfolio.rs"] = portfolio
    if "root_order" in config:
        lib = replace_once(sources["lib.rs"], "struct RootLedger {", (SPEC / "order.rs").read_text() + "\nstruct RootLedger {")
        outside = "true" if config["root_order"] == "outside-in" else "false"
        sources["lib.rs"] = replace_once(lib, "        let mut ledger = RootLedger::new(&roots, tasks.len());", f"        order_roots(&mut roots, {outside});\n        let mut ledger = RootLedger::new(&roots, tasks.len());")
    return sources


def run(command, cwd, log, env=None):
    with log.open("w", encoding="utf-8") as output:
        result = subprocess.run(command, cwd=cwd, env=env, stdout=output, stderr=subprocess.STDOUT, timeout=1800, check=False)
    if result.returncode:
        raise RuntimeError(f"Command failed ({result.returncode}): {' '.join(command)}; see {log}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True)
    parser.add_argument("--revision", default="HEAD")
    parser.add_argument("--variants", nargs="+")
    parser.add_argument("--resume", action="store_true")
    args = parser.parse_args()
    root = (REPO / args.output).resolve()
    if not root.is_relative_to(REPO / "target"):
        raise ValueError("Preparation output must be inside this repository's target directory")
    revision = subprocess.check_output(["git", "rev-parse", args.revision], cwd=REPO, text=True).strip()
    variants = json.loads((SPEC / "variants.json").read_text())
    selected = args.variants or list(variants)
    if any(name not in variants for name in selected):
        raise ValueError("Unknown variant")
    if root.exists() and not args.resume:
        raise ValueError(f"Output already exists: {root}")
    root.mkdir(parents=True, exist_ok=True)
    status = root / "PREPARATION-STATUS.txt"
    workspace = root / "workspace"
    recipe_files = [Path(__file__), SPEC / "variants.json", SPEC / "refine.rs", SPEC / "contracts.rs", SPEC / "order.rs", SPEC / "adaptive.rs", SPEC / "adaptive-contract.rs"]
    recipe = {str(p.relative_to(REPO)): digest(p) for p in recipe_files}
    preparation = {"revision": revision, "recipe": recipe}
    marker = root / "preparation.json"
    if marker.exists() and json.loads(marker.read_text())["revision"] != revision:
        raise ValueError("Resume source revision differs; use a new output directory")
    marker.write_text(json.dumps(preparation, indent=2) + "\n")
    if not workspace.exists():
        archive = root / "source.tar"
        subprocess.run(["git", "archive", "--format=tar", f"--output={archive}", revision], cwd=REPO, check=True)
        with tarfile.open(archive) as source:
            source.extractall(workspace, filter="data")
    # Always restore every changed file from the pinned Git tree, including on resume.
    names = ["lib.rs", "encoding.rs", "diagnostics.rs", "portfolio.rs"]
    originals = {name: subprocess.check_output(["git", "show", f"{revision}:crates/solver-core/src/{name}"], cwd=REPO).decode() for name in names}
    contract_path = "crates/solver-core/tests/contracts.rs"
    contracts = subprocess.check_output(["git", "show", f"{revision}:{contract_path}"], cwd=REPO).decode()
    if "fn compare_workers(" not in contracts:
        contracts = replace_once(contracts, "    let opts = options(mode, cap, 2);", "    let opts = options(mode, cap, workers);")
        contracts = replace_once(contracts, "fn compare(problem: &Problem, mode: SolveMode, cap: u32) -> SolveOutcome {", "fn compare(problem: &Problem, mode: SolveMode, cap: u32) -> SolveOutcome {\n    compare_workers(problem, mode, cap, 2)\n}\nfn compare_workers(problem: &Problem, mode: SolveMode, cap: u32, workers: usize) -> SolveOutcome {")
    if "fn enumeration_matches_reference_at_partitioning_worker_budgets(" not in contracts:
        contracts += "\n" + (SPEC / "contracts.rs").read_text()
    backend = REPO / "src-tauri/binaries/cvc5-x86_64-pc-windows-msvc.exe"
    if not backend.is_file():
        raise ValueError("Run npm run prepare:cvc5 first")
    env = dict(os.environ, SOLVER_CVC5=str(backend), SOLVER_DIAGNOSTICS="1", CARGO_TARGET_DIR=str(root / "cargo-target"))
    binary_map = {}
    try:
        for index, name in enumerate(selected, 1):
            config = variants[name]
            extra_contracts = "\n" + (SPEC / "adaptive-contract.rs").read_text() if config.get("adaptive") else ""
            (workspace / contract_path).write_text(contracts + extra_contracts, encoding="utf-8")
            destination = root / "variants" / name
            metadata_path = destination / "metadata.json"
            if metadata_path.exists():
                data = json.loads(metadata_path.read_text())
                if data["config"] != config or data["revision"] != revision:
                    raise ValueError(f"Frozen variant identity mismatch: {name}")
                for relative, expected in data["files"].items():
                    if digest(destination / relative) != expected:
                        raise ValueError(f"Frozen variant changed: {name}/{relative}")
                # A preparation-script fix may leave a verified variant unchanged.
                # Reuse only after comparing the newly generated, formatted source
                # and test harness with the exact files that passed its checks.
                for source, text in transform(originals, config).items():
                    (workspace / "crates/solver-core/src" / source).write_text(text, encoding="utf-8")
                run(["cargo", "fmt", "--all"], workspace, root / f"{name}-resume-format.log", env)
                compared = [f"src/{source}" for source in names] + ["tests/contracts.rs"]
                for relative in compared:
                    if not same_source(workspace / "crates/solver-core" / relative, destination / "solver-source/solver-core" / relative):
                        raise ValueError(f"Recipe changes verified source: {name}/{relative}; use a new output directory")
                binary_map[name] = str(destination / "bin")
                continue
            status.write_text(f"BUILDING AND TESTING {index}/{len(selected)}: {name}\nNo timing suite launched.\n")
            print(f"Preparing {index}/{len(selected)}: {name}", flush=True)
            destination.mkdir(parents=True, exist_ok=True)
            for source, text in transform(originals, config).items():
                (workspace / "crates/solver-core/src" / source).write_text(text, encoding="utf-8")
            run(["cargo", "fmt", "--all"], workspace, destination / "format.log", env)
            run(["cargo", "clippy", "-p", "solver-core", "-p", "synthetizer-app", "-p", "solver-api", "-p", "solver-reference", "-p", "solver-validation", "--all-targets", "--locked", "--", "-D", "warnings"], workspace, destination / "clippy.log", env)
            run(["cargo", "test", "-p", "solver-core", "-p", "synthetizer-app", "--locked", "--", "--test-threads=1"], workspace, destination / "tests.log", env)
            run(["cargo", "build", "--release", "-p", "synthetizer-app", "--example", "profile_solver", "--locked"], workspace, destination / "build.log", env)
            bin_dir = destination / "bin"
            bin_dir.mkdir(exist_ok=True)
            shutil.copy2(root / "cargo-target/release/examples/profile_solver.exe", bin_dir)
            source_dir = destination / "solver-source"
            source_dir.mkdir(exist_ok=True)
            for crate in ("solver-core", "solver-api", "solver-validation", "solver-reference", "synthetizer-app"):
                shutil.copytree(workspace / "crates" / crate, source_dir / crate, dirs_exist_ok=True)
            for filename in ("Cargo.toml", "Cargo.lock"):
                shutil.copy2(workspace / filename, source_dir)
            patch = ""
            for source in names:
                actual = (workspace / "crates/solver-core/src" / source).read_text()
                path = f"crates/solver-core/src/{source}"
                patch += "".join(difflib.unified_diff(originals[source].splitlines(True), actual.splitlines(True), f"a/{path}", f"b/{path}"))
            (destination / "candidate.patch").write_text(patch)
            files = {p.relative_to(destination).as_posix(): digest(p) for p in destination.rglob("*") if p.is_file() and p != metadata_path}
            data = dict(preparation, config=config, variant=name, backend_sha256=digest(backend), prepared_at=time.strftime("%Y-%m-%dT%H:%M:%S%z"), files=files)
            metadata_path.write_text(json.dumps(data, indent=2) + "\n")
            binary_map[name] = str(bin_dir)
        (root / "variant-binaries.json").write_text(json.dumps(binary_map, indent=2) + "\n")
        status.write_text("READY: selected variants built, tested, checked and frozen. No timing suite launched.\n")
        print(f"Ready: {root / 'variant-binaries.json'}", flush=True)
    except Exception as error:
        status.write_text(f"PREPARATION FAILED: {error}\nNo timing suite launched.\n")
        raise


if __name__ == "__main__":
    main()
