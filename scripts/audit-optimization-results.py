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
    exhausted = sum(r["state"] == "exhausted" and r.get("proof_committed", True) for r in owned)
    adaptive = [r for r in roots if "parent_root" in r]
    if any("proof_committed" in r and "parent_root" not in r for r in roots):
        errors.append("Proof selection flag without an adaptive parent")
    covers = {}
    for root in adaptive:
        key = (root.get("branch", 0), root["nodes"], root["links"], root["parent_root"])
        covers.setdefault(key, []).append(root)
        if root.get("branch", 0) != 1 or result["mode"] != "all_min_nl":
            errors.append("Adaptive refinement escaped Boolean All min N/L")
        if not isinstance(root.get("proof_committed"), bool):
            errors.append("Missing explicit adaptive proof selection")
        if root.get("proof_committed") and root["state"] != "exhausted":
            errors.append("Unfinished adaptive search committed to proof")
        if root.get("second_source") is not None:
            trigger = root.get("refinement_trigger_s")
            if trigger is None or trigger < 0 or trigger > root["start_s"]:
                errors.append("Child started before its optimum trigger")
            grace = root.get("refinement_grace_ms", 0)
            if not isinstance(grace, int) or grace < 0:
                errors.append("Invalid adaptive refinement grace")
            elif trigger is not None and trigger + grace / 1000 > root["start_s"] + 1e-6:
                errors.append("Child started before its refinement grace elapsed")
    for key, cover in covers.items():
        parents = [r for r in cover if r.get("second_source") is None]
        if len(parents) != 1 or parents[0]["root"] != key[-1]:
            errors.append("Adaptive cover has no unique original parent")
            continue
        parent = parents[0]
        children = [r for r in cover if r.get("second_source") is not None]
        if children and (not isinstance(parent.get("child_count"), int) or parent["child_count"] <= 0):
            errors.append("Adaptive children have no valid declared count")
            continue
        if any(not isinstance(r["second_source"], int) or not 0 <= r["second_source"] < parent["child_count"] for r in children):
            errors.append("Adaptive child owner outside the complete cover")
        if any(r["profile"] != parent["profile"] or r.get("source") != parent.get("source") or r.get("child_count") != parent.get("child_count") for r in children):
            errors.append("Adaptive child differs from its parent obligation")
        selected = [r for r in children if r.get("proof_committed")]
        if selected and parent.get("proof_committed"):
            errors.append("Parent and children both counted in the proof")
        if selected and (len(selected) != parent["child_count"] or {r["second_source"] for r in selected} != set(range(parent["child_count"]))):
            errors.append("Incomplete children counted as a full parent cover")
        if parent["state"] == "exhausted" and not parent.get("proof_committed") and not selected:
            errors.append("Completed parent has no selected proof cover")
    proof = result["outcome"]["result"]["proof"]
    if exhausted != proof["rootPartitionsExhausted"]:
        errors.append("Returned exhaustion count differs from its owner's root records")
    if result["mode"] != "all_min_nl" and any(r.get("second_source") is not None for r in roots):
        errors.append("Second-output partitioning escaped All min N/L")
    if any(r["state"] == "failed" for r in roots):
        errors.append("Worker failure in root diagnostics")
    return errors, dict(proof_owner=owner, roots=len(roots), owner_roots=len(owned),
                        owner_exhausted=exhausted, refined_roots=sum(r.get("second_source") is not None for r in roots),
                        adaptive_roots=len(adaptive), uncommitted_exhausted=sum(r["state"] == "exhausted" and not r.get("proof_committed", True) for r in roots),
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
