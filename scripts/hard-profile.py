"""Prepare and verify fixed N/L/profile benchmarks, never global solve proofs."""
import argparse
from collections import defaultdict
import csv
import hashlib
import json
from pathlib import Path
import random
import re
import shutil
import subprocess


def read(path):
    return json.loads(Path(path).read_text(encoding="utf-8-sig"))


def write(path, value):
    Path(path).write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def check_variant(directory):
    directory = Path(directory)
    hashes = read(directory / "hashes.json")
    for name, expected in hashes.items():
        target = (directory / name).resolve()
        if not target.is_relative_to(directory.resolve()) or digest(target) != expected:
            raise ValueError(f"Changed variant file: {name}")
    return hashes


def affinity_mask(value):
    if value is None:
        return None
    if not isinstance(value, str) or not re.fullmatch(r"[0-9a-fA-F]{1,16}", value):
        raise ValueError("Processor affinity must be a hexadecimal mask string")
    mask = int(value, 16)
    if not 0 < mask < 2**63:
        raise ValueError("Processor affinity mask must be nonzero and fit signed IntPtr")
    return format(mask, "x")


def validate_search_budget(manifest, schedule):
    maximum = manifest.get("max_search_seconds", 40 * 60)
    if type(maximum) is not int or not 0 < maximum <= 3 * 60 * 60:
        raise ValueError("Search budget must be an integer from 1 to 10800 seconds")
    if not schedule or sum(j["request"]["timeout_s"] for j in schedule) > maximum:
        raise ValueError("Empty plan or search caps exceed the declared budget")


def validate_affinity(job, metric):
    if job.get("affinity_before_resume") and metric.get("affinity_before_resume", "").lower() != "true":
        raise ValueError("Missing before-resume affinity evidence")
    requested = affinity_mask(job.get("processor_affinity"))
    if requested is None:
        if metric.get("affinity_requested"):
            raise ValueError("Unexpected processor affinity control")
        return
    if metric.get("affinity_requested") != requested or metric.get("affinity_observed") != requested:
        raise ValueError("Missing or changed processor affinity evidence")
    applied = float(metric["affinity_applied_s"])
    if not 0 <= applied <= float(metric["process_wall_s"]):
        raise ValueError("Invalid processor affinity application time")


def prepare(manifest_path, root):
    root = Path(root).resolve()
    manifest_path = Path(manifest_path).resolve()
    manifest = read(manifest_path)
    inputs = root / "input"
    inputs.mkdir()
    (inputs / "cases").mkdir()
    binary = root / "bin/profile_obligation.exe"
    variants = {}
    for name, origin in manifest.get("variants", {}).items():
        if not name or any(c not in "abcdefghijklmnopqrstuvwxyz0123456789_" for c in name):
            raise ValueError("Invalid variant name")
        source = (manifest_path.parent / origin).resolve()
        check_variant(source)
        destination = root / "variants" / name
        shutil.copytree(source, destination)
        check_variant(destination)
        variants[name] = str(destination)
    write(root / "variants.json", variants)
    schedule = []
    prefix_certificates = {}
    root_certificates = {}
    for original in manifest["jobs"]:
        job = dict(original)
        affinity = affinity_mask(job.pop("processor_affinity", None))
        variant = job.pop("variant", None)
        variant_root = Path(variants[variant]) if variant is not None else None
        selected_binary = variant_root / "bin/profile_obligation.exe" if variant_root else binary
        kind = job.pop("kind", "fixed_obligation")
        expand = job.pop("expand_profiles", False)
        job_id = job.pop("id")
        if not job_id or any(c not in "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_" for c in job_id):
            raise ValueError("Invalid job ID")
        if kind == "witness_replay":
            source = (manifest_path.parent / job.pop("witness_file")).resolve()
            saved = read(source)
            if saved.get("kind") != "fixed_obligation" or saved.get("validated") is not True:
                raise ValueError("Replay needs a validated fixed-work witness source")
            witness = next(s for p in saved["profiles"] for s in p["solutions"])
            replay_input = inputs / (job_id + ".witness.json")
            write(replay_input, {"problem": saved["problem"], "outcome": {"result": witness},
                                "source_scope": saved["scope"], "source_sha256": digest(source)})
            executable = variant_root / "bin/profile_witness.exe" if variant_root else root / "bin/profile_witness.exe"
            schedule.append(dict(id=job_id, kind=kind, variant=variant, binary=str(executable),
                                 processor_affinity=affinity, affinity_before_resume=True, request=job, request_file=str(replay_input), input_sha256=digest(replay_input)))
            continue
        if kind != "fixed_obligation":
            raise ValueError("Unknown workload kind")
        source = (manifest_path.parent / job["case_file"]).resolve()
        if read(source)["problem"]["maxLinkRate"] != "1200":
            raise ValueError("Every case must use capacity 1200")
        # Include the digest so equal basenames cannot silently overwrite cases.
        case_file = "cases/" + digest(source)[:16] + ".json"
        shutil.copy2(source, inputs / case_file)
        job["case_file"] = case_file
        probe = inputs / (job_id + ".probe.json")
        write(probe, job)
        listed = subprocess.run([str(selected_binary), "--list", str(probe)], check=True,
                                capture_output=True, text=True, timeout=30)
        expected = json.loads(listed.stdout)
        if job.get("prefix") is not None:
            if job.get("root") is not None:
                raise ValueError("Prefix and adaptive root selections are mutually exclusive")
            if expand or job.get("profile") is None or job.get("workers") != 1:
                raise ValueError("Prefix requires one selected profile and one worker")
            if not expected.get("prefix_identity"):
                raise ValueError("Binary does not support frozen prefix identities")
            identity = expected["prefix_identity"]
            if job.get("prefix_identity", identity) != identity:
                raise ValueError("Requested frozen prefix identity changed")
            recipe = json.dumps([expected["problem"], job["node_count"], job["link_count"],
                                 job["profile"], job["prefix"]], sort_keys=True)
            if prefix_certificates.setdefault(recipe, identity) != identity:
                raise ValueError("Same prefix recipe changed between jobs or variants")
            job["prefix_identity"] = expected["prefix_identity"]
        if job.get("root") is not None:
            if expand or job.get("profile") is None or job.get("stage") != "p1":
                raise ValueError("Adaptive root requires one selected profile and p1")
            if not expected.get("root_identity"):
                raise ValueError("Binary does not support frozen adaptive root identities")
            identity = expected["root_identity"]
            if job.get("root_identity", identity) != identity:
                raise ValueError("Requested frozen adaptive root identity changed")
            recipe = json.dumps([expected["problem"], job["node_count"], job["link_count"],
                                 job["profile"], job["workers"], job["root"]], sort_keys=True)
            if root_certificates.setdefault(recipe, identity) != identity:
                raise ValueError("Same adaptive root recipe changed between jobs or variants")
            job["root_identity"] = expected["root_identity"]
        selections = expected["profiles"] if expand else [job.get("profile")]
        for index, profile in enumerate(selections):
            request = dict(job, profile=profile)
            identity = f"{job_id}-profile-{index}" if expand else job_id
            request_file = inputs / (identity + ".request.json")
            if request_file.exists():
                raise ValueError("Duplicate job identity")
            write(request_file, request)
            schedule.append(dict(id=identity, kind=kind, variant=variant, binary=str(selected_binary), request_file=str(request_file),
                                 processor_affinity=affinity, affinity_before_resume=True, request=request, problem=expected["problem"],
                                 expected_profiles=[profile] if expand else expected["profiles"]))
        probe.unlink()
    if len({j["id"] for j in schedule}) != len(schedule):
        raise ValueError("Duplicate job identity")
    validate_search_budget(manifest, schedule)
    random.Random(manifest.get("seed", 280826)).shuffle(schedule)
    write(root / "schedule.json", schedule)
    write(root / "manifest.json", manifest)
    write(root / "binary.json", dict(path=str(binary), sha256=digest(binary)))
    print(f"Prepared {len(schedule)} jobs; {sum(j['request']['timeout_s'] for j in schedule)/60:.1f} minutes of search caps.")


def profile_id(profile):
    return tuple(profile[k] for k in ("splitter2", "splitter3", "merger2", "merger3"))


def validate_result(job, result):
    prefix = job["request"].get("prefix") is not None
    root = job["request"].get("root") is not None
    if prefix and root:
        raise ValueError("Ambiguous fixed-work selection")
    expected_scope = "selected_prefix" if prefix else "selected_root" if root else "selected_profiles"
    if result.get("kind") != "fixed_obligation" or result.get("scope") != expected_scope or result.get("schema_version") != 1:
        raise ValueError("Wrong result scope/schema")
    if prefix and (not job["request"].get("prefix_identity") or
                   result.get("prefix_identity") != job["request"]["prefix_identity"]):
        raise ValueError("Missing or changed prefix identity")
    if root and (not job["request"].get("root_identity") or
                 result.get("root_identity") != job["request"]["root_identity"]):
        raise ValueError("Missing or changed adaptive root identity")
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
        if (prefix or root) and profile["roots"] != 1:
            raise ValueError("A selected prefix or adaptive root accounts for one local root")
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
    variants = read(root / "variants.json") if (root / "variants.json").exists() else {}
    for directory in variants.values():
        check_variant(directory)
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
    metrics_by_id = {row["id"]: row for row in metrics}
    replays = defaultdict(list)
    comparisons = defaultdict(list)
    for job in schedule:
        try:
            validate_affinity(job, metrics_by_id[job["id"]])
            if job.get("variant") is not None:
                example = "profile_witness.exe" if job.get("kind") == "witness_replay" else "profile_obligation.exe"
                expected_binary = Path(variants[job["variant"]]) / "bin" / example
                if Path(job["binary"]).resolve() != expected_binary.resolve():
                    raise ValueError("Job executable does not match its variant")
            result = read(root / "results" / (job["id"] + ".json"))
            if job.get("kind") == "witness_replay":
                validate_replay(job, result)
                if result["completed"]:
                    source = read(job["request_file"])
                    replays[source["source_sha256"]].append(result)
                summaries.append(dict(id=job["id"], kind="witness_replay", variant=job["variant"], result=result))
                continue
            profiles = validate_result(job, result)
            for profile in profiles:
                key = (json.dumps(job["problem"], sort_keys=True), job["request"]["node_count"],
                        job["request"]["link_count"], profile_id(profile["profile"]),
                        json.dumps(job["request"].get("prefix_identity"), sort_keys=True),
                        json.dumps(job["request"].get("root_identity"), sort_keys=True))
                comparisons[key].append((job, profile))
            spans = defaultdict(list)
            for rec in result["activity"]["records"]:
                spans[rec["kind"]].append((rec["end_ns"] - rec["start_ns"]) / 1e9)
            summaries.append(dict(id=job["id"], variant=job.get("variant"), scope=result["scope"], request=job["request"], status=result["status"],
                processor_affinity=job.get("processor_affinity"),
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
    for completed in replays.values():
        for result in completed[1:]:
            if any(result["hotspots"][k] != completed[0]["hotspots"][k] for k in ("witness_leaves", "witness_branches")):
                failures.append("Replay permutation coverage changed")
    if (root / "failed-jobs.json").exists():
        failures.extend(str(j) for j in read(root / "failed-jobs.json"))
    write(root / "summary.json", dict(failures=failures, records=len(summaries), summary=summaries))
    if failures:
        raise ValueError("\n".join(failures))
    print(f"Verified {len(summaries)}/{len(schedule)} fixed workloads. Exhaustion is local to the recorded profile/prefix/root scope.")


def validate_replay(job, result):
    if digest(job["request_file"]) != job["input_sha256"]:
        raise ValueError("Changed replay input")
    cancel_ms = job["request"].get("cancel_after_ms", 0)
    if result.get("kind") != "witness_replay" or result.get("validated") is not True or result.get("cancel_after_ms") != cancel_ms:
        raise ValueError("Invalid replay identity or validation")
    if result.get("completed"):
        if result.get("key_equal") is not True:
            raise ValueError("Replay key mismatch")
    elif not cancel_ms or result.get("cancelled") is not True or result.get("key_equal") is not None:
        raise ValueError("Unexpected incomplete replay")
    if result.get("activity", {}).get("active"):
        raise ValueError("Replay left active work")


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
