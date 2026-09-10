"""Check release-runner proof evidence. Timings from these validation probes are not benchmarks."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import hashlib
from benchmark_policy import result_comparison_errors

REPO = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("root_audit", Path(__file__).with_name("audit-optimization-results.py"))
auditor = importlib.util.module_from_spec(spec)
spec.loader.exec_module(auditor)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binaries", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--promotion", action="store_true")
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    binaries = json.loads(args.binaries.read_text(encoding="utf-8-sig"))
    backend = REPO / "src-tauri/binaries/cvc5-x86_64-pc-windows-msvc.exe"
    env = dict(os.environ, SOLVER_CVC5=str(backend), SOLVER_DIAGNOSTICS="1")
    results = []
    references = {}
    if args.promotion:
        probes = [(variant, "acyclic36", workers, 9, 20) for workers in (8,16,32)
                  for variant in ("baseline", "pairs-boolean")]
        probes += [("baseline","medium258",32,12,90),("pairs-boolean","medium258",32,12,90),
                   ("pairs-boolean","medium258",32,12,4)]
    else:
        probes = [(variant, "acyclic36", workers, 9, 30) for workers in (12,24)
                  for variant in ("baseline", "order-reverse", "order-outside-in", "adaptive-boolean")]
        probes += [("baseline","medium258",32,12,90),("adaptive-boolean","medium258",32,12,90),
                   ("adaptive-boolean","medium258",32,12,4)]
    for variant, case, workers, cap, seconds in probes:
        label = f"{variant}-{case}-w{workers}-limit{seconds}"
        path = args.output / f"{label}.json"
        binary = Path(binaries[variant]) / "profile_solver.exe"
        print(f"Validating {label}", flush=True)
        command = [str(binary),str(seconds),str(workers),str(cap),str(path.resolve()),"all_min_nl",str(REPO/"benchmarks/cases"/f"{case}.json")]
        with (args.output / f"{label}.log").open("w") as log:
            subprocess.run(command, env=env, stdout=log, stderr=subprocess.STDOUT, check=True, timeout=seconds+30)
        result = json.loads(path.read_text())
        assert result["validated"] is True
        errors, audit = auditor.audit(result)
        assert not errors, (label,errors)
        if seconds != 4:
            assert result["outcome"]["kind"] == "optimal", label
            key = (case,workers)
            reference = references.setdefault(key,result)
            assert not result_comparison_errors(result,reference,"all_min_nl","any_optimum"), label
        else:
            assert result["deadline_fired"] and result["outcome"]["kind"] == "incomplete"
            if not args.promotion:
                assert result["outcome"]["result"]["bestKnown"] is not None
        if variant == "adaptive-boolean" and case == "medium258":
            assert audit["adaptive_roots"] > 0 and audit["refined_roots"] > 0, label
        if args.promotion and variant == "pairs-boolean" and case == "medium258":
            assert audit["refined_roots"] > 0, label
        results.append(dict(label=label, passed=True, audit=audit))
    hashes = {name:hashlib.sha256((Path(directory)/"profile_solver.exe").read_bytes()).hexdigest() for name,directory in binaries.items()}
    evidence = dict(passed=True, benchmark=False, runner_hashes=hashes,
                    backend_sha256=hashlib.sha256(backend.read_bytes()).hexdigest(),probes=results)
    (args.output/"qualification.json").write_text(json.dumps(evidence,indent=2)+"\n")
    print(f"Passed {len(results)} release evidence probes; timings are excluded from benchmark analysis.")


if __name__ == "__main__":
    main()
