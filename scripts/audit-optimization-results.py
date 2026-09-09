"""Audit independent proof ownership and rank expensive exact roots after a verified screen."""
import argparse
import csv
import json
from pathlib import Path


def audit(result):
    errors = []
    roots = result.get("roots", [])
    if not result.get("diagnostics_enabled"):
        if roots:
            errors.append("Timing run unexpectedly contains root diagnostics")
        return errors, None
    owners = [int(d["value"]["value"]) for d in result.get("diagnostics", [])
              if d["name"] == "solver.portfolio_proof_owner"]
    if len(owners) != 1 and result["workers"] > 1 and roots:
        errors.append("Expected one explicit returned proof owner")
    owner = owners[0] if len(owners) == 1 else 0
    identities = [(r.get("branch", 0), r["nodes"], r["links"], r["root"]) for r in roots]
    if len(identities) != len(set(identities)):
        errors.append("Duplicate root completion identity")
    if any(r["state"] not in ("exhausted", "optimum", "cancelled", "failed") for r in roots):
        errors.append("Unknown root state")
    owned = [r for r in roots if r.get("branch", 0) == owner]
    exhausted = sum(r["state"] == "exhausted" for r in owned)
    proof = result["outcome"]["result"]["proof"]
    if exhausted != proof["rootPartitionsExhausted"]:
        errors.append("Returned exhaustion count differs from its owner's root records")
    if result["mode"] != "all_min_nl" and any(r.get("second_source") is not None for r in roots):
        errors.append("Second-output partitioning escaped All min N/L")
    if any(r["state"] == "failed" for r in roots):
        errors.append("Worker failure in root diagnostics")
    return errors, dict(proof_owner=owner, roots=len(roots), owner_roots=len(owned),
                        owner_exhausted=exhausted, refined_roots=sum(r.get("second_source") is not None for r in roots),
                        longest=sorted(roots, key=lambda r: r["wall_s"], reverse=True)[:20],
                        # These times overlap across workers and are not CPU time.
                        overlapping_check_seconds=sum(r["check_s"] for r in roots))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("results", type=Path)
    args = parser.parse_args()
    records, failures = [], []
    with (args.results / "results.csv").open(encoding="utf-8-sig", newline="") as source:
        for row in csv.DictReader(source):
            result = json.loads(Path(row["result_file"]).read_text(encoding="utf-8-sig"))
            errors, detail = audit(result)
            failures.extend(f"{row['result_file']}: {error}" for error in errors)
            if detail is not None:
                records.append(dict(case=row["case"], mode=row["mode"], variant=row["variant"],
                                    workers=int(row["workers"]), repeat=int(row["repeat"]), **detail))
    output = dict(audits=records, failures=failures, passed=not failures)
    (args.results / "root-audit.json").write_text(json.dumps(output, indent=2) + "\n")
    if failures:
        raise SystemExit("\n".join(failures))
    print(f"Proof ownership verified for {len(records)} diagnostic records.")


if __name__ == "__main__":
    main()
