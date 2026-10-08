"""Compare two release executables on identical generated audio; no game installation required."""
import argparse
import ctypes
import json
import os
import platform
from pathlib import Path
import random
import re
import statistics
import subprocess
import tempfile
import time
import wave

from audit import Project, digest, require, write


class MemoryCounters(ctypes.Structure):
    _fields_ = [("cb", ctypes.c_ulong), ("faults", ctypes.c_ulong)] + [
        (name, ctypes.c_size_t) for name in
        ("peak_working_set", "working_set", "peak_paged", "paged", "peak_nonpaged", "nonpaged", "pagefile", "peak_pagefile")
    ]


def measure(project, label):
    command = [str(project.exe), "pc", "pc", "unused", "sound_zone", "project", "audit", "all"]
    env = dict(os.environ, TA_GAME_PATH=str(project.root), TA_TOOLS_PATH=str(project.root / "tools"))
    peak = None
    start = time.perf_counter()
    with subprocess.Popen(command, cwd=project.root, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE) as process:
        handle = None
        if os.name == "nt":
            kernel = ctypes.WinDLL("kernel32", use_last_error=True)
            kernel.OpenProcess.argtypes = [ctypes.c_ulong, ctypes.c_int, ctypes.c_ulong]
            kernel.OpenProcess.restype = ctypes.c_void_p
            kernel.CloseHandle.argtypes = [ctypes.c_void_p]
            query = ctypes.WinDLL("psapi").GetProcessMemoryInfo
            query.argtypes = [ctypes.c_void_p, ctypes.POINTER(MemoryCounters), ctypes.c_ulong]
            handle = kernel.OpenProcess(0x410, False, process.pid)
        try:
            while process.poll() is None:
                if handle:
                    counters = MemoryCounters()
                    counters.cb = ctypes.sizeof(counters)
                    if query(handle, ctypes.byref(counters), counters.cb):
                        peak = max(peak or 0, counters.peak_working_set)
                if time.perf_counter() - start > 180:
                    process.kill()
                    raise TimeoutError("benchmark converter exceeded 180 seconds")
                time.sleep(0.005)
            stdout, stderr = process.communicate()
        finally:
            if handle:
                kernel.CloseHandle(handle)
    elapsed = time.perf_counter() - start
    require(process.returncode == 0, stderr.decode(errors="replace"))
    return {"scenario": label, "seconds": elapsed, "peak_working_set_bytes": peak,
            "stdout": stdout.decode(errors="replace")}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", type=Path, required=True)
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--assets", type=int, default=64)
    parser.add_argument("--seconds", type=int, default=10)
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--require-journal-hits", action="store_true", help="require candidate to reuse every unchanged source")
    parser.add_argument("--out", type=Path, default=Path("target/audit/benchmark.json"))
    args = parser.parse_args()
    require(args.assets > 0 and args.seconds > 0 and args.repeats > 0, "benchmark sizes must be positive")
    executables = {"reference": args.reference.resolve(strict=True), "candidate": args.candidate.resolve(strict=True)}
    result = {"kind": "generated high-entropy 48 kHz stereo WAVs; filesystem-warm, not a real map",
              "machine": platform.platform(), "python": platform.python_version(), "logical_cpus": os.cpu_count(),
              "assets": args.assets, "seconds_per_asset": args.seconds, "repeats": args.repeats,
              "binaries": {name: digest(path.read_bytes()) for name, path in executables.items()}, "runs": []}
    try:
        for executable in executables.values():
            subprocess.run([str(executable), "--help"], capture_output=True, check=True, timeout=10)
        for repetition in range(args.repeats):
            reference_outputs = {}
            order = ("reference", "candidate") if repetition % 2 == 0 else ("candidate", "reference")
            for name in order:
                with tempfile.TemporaryDirectory(prefix="ultrasound-benchmark-") as directory:
                    project = Project(Path(directory), executables[name], [])
                    for index in range(args.assets):
                        path = project.root / f"sound_assets/bench{index:04}.wav"
                        with wave.open(str(path), "wb") as out:
                            out.setparams((2, 2, 48000, 0, "NONE", "not compressed"))
                            out.writeframes(random.Random(index).randbytes(48000 * args.seconds * 4))
                        project.rows.append({"Name": f"bench{index:04}", "FileSpec": path.name,
                                             "Template": "base", "Storage": "LOADED" if index % 2 else "STREAMED"})
                    project.aliases()
                    for scenario in ("clean", "warm", "single_edit"):
                        if scenario == "single_edit":
                            path = project.root / "sound_assets/bench0000.wav"
                            with path.open("r+b") as stream:
                                stream.seek(1000)
                                value = stream.read(1)[0]
                                stream.seek(1000)
                                stream.write(bytes([value ^ 1]))
                        measured = measure(project, scenario)
                        if args.require_journal_hits and name == "candidate":
                            actual = sum(map(int, re.findall(r"\((\d+) sources unchanged, per the NTFS journal\)", measured["stdout"])))
                            expected = {"clean": 0, "warm": args.assets + 6, "single_edit": args.assets + 5}[scenario]
                            require(actual == expected, f"{scenario}: expected {expected} journal hits, got {actual}")
                            measured["journal_hits"] = actual
                        measured.update(binary=name, repetition=repetition + 1)
                        snapshot = project.snapshot()
                        if scenario in reference_outputs:
                            require(snapshot == reference_outputs[scenario], f"{scenario}: semantic outputs differ")
                        else:
                            reference_outputs[scenario] = snapshot
                        measured["deployed_bytes"] = sum(project.bank("all", ext).stat().st_size for ext in ("sabl", "sabs"))
                        result["runs"].append(measured)
                        print(f"{name} {repetition + 1}/{args.repeats} {scenario}: {measured['seconds']:.3f}s", flush=True)
        result["summary"] = {}
        for scenario in ("clean", "warm", "single_edit"):
            result["summary"][scenario] = {}
            for name in executables:
                runs = [r for r in result["runs"] if r["binary"] == name and r["scenario"] == scenario]
                result["summary"][scenario][name] = {
                    "median_seconds": statistics.median(r["seconds"] for r in runs),
                    "min_seconds": min(r["seconds"] for r in runs),
                    "max_seconds": max(r["seconds"] for r in runs),
                    "peak_working_set_bytes": max((r["peak_working_set_bytes"] or 0) for r in runs) or None,
                }
        result["status"] = "passed; all compared outputs equivalent"
    except Exception as error:
        result["status"] = "failed"
        result["error"] = str(error)
        raise
    finally:
        write(args.out.resolve(), json.dumps(result, indent=2) + "\n")
        print("Report:", args.out.resolve(), flush=True)


if __name__ == "__main__":
    main()
