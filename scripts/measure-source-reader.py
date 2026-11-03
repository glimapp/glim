#!/usr/bin/env python3
"""Compare opt-in Glim source measurements in real macOS windows.

Build each revision with native Cargo, --release --features reader-measurements,
and preserve its binary before building the next revision. This runner does not
build, install, alter preferences, or save edits to the generated fixtures.
"""
import argparse
import hashlib
import json
import math
import platform
import statistics
import subprocess
import sys
import tempfile
from datetime import datetime, timezone
from pathlib import Path


def digest(path):
    result = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            result.update(chunk)
    return result.hexdigest()


def command(*args):
    return subprocess.check_output(args, text=True).strip()


def distribution(values):
    ordered = sorted(values)
    return {
        "samples": len(ordered),
        "median_ms": statistics.median(ordered),
        "p95_ms": ordered[math.ceil(len(ordered) * 0.95) - 1],
        "max_ms": ordered[-1],
    }


def summarize(runs):
    groups = {}
    for run in runs:
        key = (run["variant"], run["lines"])
        groups.setdefault(key, []).append(run["result"])
    summary = []
    for (variant, lines), reports in groups.items():
        item = {"variant": variant, "lines": lines, "runs": len(reports)}
        item["first_readable_submission"] = distribution([
            report["open"]["first_readable_viewport"]["to_frame_submission_ms"]
            for report in reports
        ])
        for phase in ("input", "scroll"):
            samples = [sample for report in reports for sample in report[phase]]
            item[phase] = {
                metric: distribution([sample[metric] for sample in samples])
                for metric in ("dispatch_ms", "draw_ms", "to_frame_submission_ms")
            }
        summary.append(item)
    return summary


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--baseline-binary", type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--lines", type=int, nargs="+", default=[1000, 10000, 50000])
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    if platform.system() != "Darwin":
        parser.error("real-window source measurements require macOS")
    if args.output.resolve().is_relative_to(root):
        parser.error("keep measurement reports outside the repository")
    if args.repeats < 1 or any(lines < 100 for lines in args.lines):
        parser.error("use positive repeats and at least 100 lines for scrolling")
    binaries = {"candidate": args.binary.resolve(strict=True)}
    if args.baseline_binary:
        binaries["baseline"] = args.baseline_binary.resolve(strict=True)
    for binary in binaries.values():
        help_result = subprocess.run([str(binary), "--help"], capture_output=True, text=True, timeout=30, check=True)
        if "--measure-source-reader" not in help_result.stdout:
            parser.error(f"{binary} lacks the reader-measurements feature")
    report = {
        "schema": 1,
        "created_at": datetime.now(timezone.utc).isoformat(),
        "environment": {
            "macos": command("sw_vers", "-productVersion"),
            "build": command("sw_vers", "-buildVersion"),
            "machine": platform.machine(),
            "cpu": command("sysctl", "-n", "machdep.cpu.brand_string"),
            "memory_bytes": int(command("sysctl", "-n", "hw.memsize")),
        },
        "binaries": {
            variant: {"path": str(path), "sha256": digest(path)}
            for variant, path in binaries.items()
        },
        "runner_sha256": digest(Path(__file__)),
        "protocol": {
            "input": "24 insert/backspace pairs rotating start/middle/end; exact text and dirty-state verification outside timing",
            "scroll": "12 downward and 12 upward 80-pixel wheel events; require viewport movement",
            "ordering": "alternate variant order on each repeat; one fresh process per fixture/variant",
            "cache": "OS file cache is uncontrolled; these are not cold-disk measurements",
            "timing": "scripted dispatch through painted source viewport and platform submission, not hardware latency or display scanout",
            "build": "supply binaries from the same native release profile with reader-measurements enabled",
        },
        "runs": [],
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="glim-source-measurement-") as directory:
        for lines in args.lines:
            fixture = Path(directory) / f"source-{lines}.txt"
            fixture.write_text("".join(
                f"Line {index:06d}: 中文内容, plain text, punctuation and a reproducible editor workload.\n"
                for index in range(lines)
            ), encoding="utf-8")
            fixture_hash = digest(fixture)
            for repeat in range(args.repeats):
                order = list(binaries)
                if repeat % 2 == 0:
                    order.reverse()
                for variant in order:
                    print(f"{variant}: {lines} lines, run {repeat + 1}", file=sys.stderr, flush=True)
                    process = subprocess.run(
                        [str(binaries[variant]), "--measure-source-reader", str(fixture)],
                        capture_output=True, text=True, timeout=240, check=True,
                    )
                    result = json.loads(process.stdout)
                    if "error" in result:
                        raise RuntimeError(result["error"])
                    if digest(fixture) != fixture_hash:
                        raise RuntimeError("measurement modified its fixture")
                    report["runs"].append({
                        "variant": variant, "lines": lines, "repeat": repeat,
                        "fixture_sha256": fixture_hash, "result": result,
                    })
                    report["summary"] = summarize(report["runs"])
                    args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    print(args.output)


if __name__ == "__main__":
    main()
