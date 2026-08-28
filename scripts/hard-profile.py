"""Prepare and verify fixed N/L/profile benchmarks, never global solve proofs."""
import argparse
from collections import defaultdict
import csv
import hashlib
import json
from pathlib import Path
import random
import shutil
import subprocess


def read(path):
    return json.loads(Path(path).read_text(encoding="utf-8-sig"))


def write(path, value):
    Path(path).write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def prepare(manifest_path, root):
    root = Path(root).resolve()
    manifest_path = Path(manifest_path).resolve()
    manifest = read(manifest_path)
    inputs = root / "input"
    inputs.mkdir()
    (inputs / "cases").mkdir()
    binary = root / "bin/profile_obligation.exe"
    schedule = []
    for original in manifest["jobs"]:
        job = dict(original)
        expand = job.pop("expand_profiles", False)
        job_id = job.pop("id")
        if not job_id or any(c not in "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_" for c in job_id):
            raise ValueError("Invalid job ID")
        source = (manifest_path.parent / job["case_file"]).resolve()
        if read(source)["problem"]["maxLinkRate"] != "1200":
            raise ValueError("Every case must use capacity 1200")
        # Include the digest so equal basenames cannot silently overwrite cases.
        case_file = "cases/" + digest(source)[:16] + ".json"
        shutil.copy2(source, inputs / case_file)
        job["case_file"] = case_file
        probe = inputs / (job_id + ".probe.json")
        write(probe, job)
        listed = subprocess.run([str(binary), "--list", str(probe)], check=True,
                                capture_output=True, text=True, timeout=30)
        expected = json.loads(listed.stdout)
        selections = expected["profiles"] if expand else [job.get("profile")]
        for index, profile in enumerate(selections):
            request = dict(job, profile=profile)
            identity = f"{job_id}-profile-{index}" if expand else job_id
            request_file = inputs / (identity + ".request.json")
            if request_file.exists():
                raise ValueError("Duplicate job identity")
            write(request_file, request)
            schedule.append(dict(id=identity, request_file=str(request_file),
                                 request=request, problem=expected["problem"],
                                 expected_profiles=[profile] if expand else expected["profiles"]))
        probe.unlink()
    if not schedule or sum(j["request"]["timeout_s"] for j in schedule) > 25 * 60:
        raise ValueError("Empty plan or search caps exceed 25 minutes")
    random.Random(manifest.get("seed", 280826)).shuffle(schedule)
    write(root / "schedule.json", schedule)
    write(root / "manifest.json", manifest)
    write(root / "binary.json", dict(path=str(binary), sha256=digest(binary)))
    print(f"Prepared {len(schedule)} jobs; {sum(j['request']['timeout_s'] for j in schedule)/60:.1f} minutes of search caps.")


def profile_id(profile):
    return tuple(profile[k] for k in ("splitter2", "splitter3", "merger2", "merger3"))


def validate_result(job, result):
    if result.get("kind") != "fixed_obligation" or result.get("scope") != "selected_profiles" or result.get("schema_version") != 1:
        raise ValueError("Wrong result scope/schema")
    if result.get("request") != job["request"] or result.get("problem") != job["problem"]:
        raise ValueError("Changed request or exact problem")
    if result.get("expected_profiles") != job["expected_profiles"] or result.get("validated") is not True:
        raise ValueError("Missing profile identity or validation")
    profiles = result["profiles"]
    if [p["profile"] for p in profiles] != job["expected_profiles"] or not profiles:
        raise ValueError("Missing, reordered or duplicate profile results")
    if result["status"] not in ("exhausted", "incomplete") or (result["status"] == "exhausted") != all(p["exhausted"] for p in profiles):
        raise ValueError("False exhaustion claim")
    if job["request"].get("must_exhaust") and result["status"] != "exhausted":
        raise ValueError("Control workload did not exhaust")
    for profile in profiles:
        if profile["roots"] < 1 or not 0 <= profile["roots_exhausted"] <= profile["roots"]:
            raise ValueError("Invalid root accounting")
        if profile["exhausted"] != (profile["roots"] == profile["roots_exhausted"]):
            raise ValueError("Profile and root exhaustion disagree")
        if profile["exhausted"]:
            if profile["incomplete_reason"] is not None:
                raise ValueError("Exhausted profile has an incomplete reason")
        elif profile["incomplete_reason"] != {"kind": "cancelled"} or result.get("deadline_fired") is not True:
            raise ValueError("Unexpected incomplete profile")
        keys = []
        for solution in profile["solutions"]:
            if (solution["nodeCount"], solution["linkCount"]) != (job["request"]["node_count"], job["request"]["link_count"]):
                raise ValueError("Witness belongs to different N/L")
            keys.append(bytes(solution["canonicalGraphKey"]).hex())
        if keys != sorted(set(keys)) or (job["request"]["mode"] == "best" and len(keys) > 1):
            raise ValueError("Malformed witness set")
    trace = result.get("activity", {})
    if trace.get("active"):
        raise ValueError("Activity left open after solver return")
    for record in trace.get("records", []):
        if record["end_ns"] < record["start_ns"]:
            raise ValueError("Reversed phase interval")
    return profiles


def verify(root):
    root = Path(root).resolve()
    schedule = read(root / "schedule.json")
    binary = read(root / "binary.json")
    failures = []
    if digest(binary["path"]) != binary["sha256"]:
        failures.append("Binary hash mismatch")
    expected_ids = [job["id"] for job in schedule]
    if len(set(expected_ids)) != len(expected_ids):
        failures.append("Duplicate scheduled identity")
    with (root / "process-metrics.csv").open(encoding="utf-8-sig", newline="") as stream:
        metrics = list(csv.DictReader(stream))
    if sorted(row["id"] for row in metrics) != sorted(expected_ids):
        failures.append("Process metrics do not match the schedule")
    if any(row["watchdog_killed"].lower() != "false" or int(row["exit_code"]) != 0 for row in metrics):
        failures.append("Failed or killed process")
    summaries = []
    comparisons = defaultdict(list)
    for job in schedule:
        try:
            result = read(root / "results" / (job["id"] + ".json"))
            profiles = validate_result(job, result)
            for profile in profiles:
                key = (json.dumps(job["problem"], sort_keys=True), job["request"]["node_count"],
                       job["request"]["link_count"], profile_id(profile["profile"]))
                comparisons[key].append((job, profile))
            spans = defaultdict(list)
            for rec in result["activity"]["records"]:
                spans[rec["kind"]].append((rec["end_ns"] - rec["start_ns"]) / 1e9)
            summaries.append(dict(id=job["id"], request=job["request"], status=result["status"],
                wall_s=result["wall_s"], profiles=profiles, hotspots=result["hotspots"],
                trace_dropped=result["activity"]["dropped"],
                phase_s={k:dict(count=len(v), sum=sum(v), maximum=max(v)) for k,v in spans.items()}))
        except (OSError, KeyError, TypeError, ValueError) as error:
            failures.append(f"{job['id']}: {error}")
    for entries in comparisons.values():
        complete_all = [p for j,p in entries if p["exhausted"] and j["request"]["mode"] == "all"]
        if complete_all:
            reference = {bytes(s["canonicalGraphKey"]).hex():s for s in complete_all[0]["solutions"]}
            for job, profile in entries:
                actual = {bytes(s["canonicalGraphKey"]).hex():s for s in profile["solutions"]}
                if any(k not in reference or reference[k] != v for k,v in actual.items()):
                    failures.append(f"{job['id']}: witness differs from completed profile reference")
                if profile["exhausted"]:
                    expected = reference if job["request"]["mode"] == "all" else dict(list(sorted(reference.items()))[:1])
                    if actual != expected:
                        failures.append(f"{job['id']}: completed witness set/preferred mismatch")
        for mode in ("best", "all"):
            complete = [(j,p) for j,p in entries if p["exhausted"] and j["request"]["mode"] == mode]
            if complete and any(p["solutions"] != complete[0][1]["solutions"] for _,p in complete):
                failures.append("Completed same-mode profile results disagree")
    if (root / "failed-jobs.json").exists():
        failures.extend(str(j) for j in read(root / "failed-jobs.json"))
    write(root / "summary.json", dict(failures=failures, records=len(summaries), summary=summaries))
    if failures:
        raise ValueError("\n".join(failures))
    print(f"Verified {len(summaries)}/{len(schedule)} fixed workloads. Exhaustion is local to selected profiles.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("prepare", "verify"))
    parser.add_argument("root", type=Path)
    parser.add_argument("--manifest", type=Path)
    args = parser.parse_args()
    if args.action == "prepare":
        prepare(args.manifest, args.root)
    else:
        verify(args.root)
