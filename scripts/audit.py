"""Run the v1 compatibility checks. Uses only Python's standard library."""
import argparse
import csv
import hashlib
import io
import json
import os
from pathlib import Path
import platform
import statistics
import struct
import subprocess
import tempfile
import time
import wave

REPO = Path(__file__).resolve().parents[1]
BASELINE = REPO / "tests/fixtures/baseline.json"
BUGS = {
    "B001": "zero_block_alignment_returns_error",
    "B002": "padded_odd_riff_chunk_is_accepted",
    "B003": "short_envelope_uses_same_units_as_long_envelope",
    "B004": "one_frame_resample_does_not_panic",
    "B005": "distinct_alias_settings_survive_deduplication",
    "B008": "default_encoder_preserves_unprocessed_samples",
}


def require(condition, message):
    if not condition:
        raise AssertionError(message)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def write(path, text):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8", newline="")


def write_csv(path, fields, rows):
    buf = io.StringIO(newline="")
    writer = csv.DictWriter(buf, fieldnames=fields)
    writer.writeheader()
    writer.writerows(rows)
    write(path, buf.getvalue())


def audio(path, rate=48000, channels=1, amplitude=12000):
    path.parent.mkdir(parents=True, exist_ok=True)
    with wave.open(str(path), "wb") as out:
        out.setparams((channels, 2, rate, 0, "NONE", "not compressed"))
        pcm = [((i % 97) - 48) * amplitude // 48 for i in range(rate // 10)]
        out.writeframes(b"".join(struct.pack("<h", s if c == 0 else -s)
                                 for s in pcm for c in range(channels)))


class Project:
    def __init__(self, root, exe, report):
        self.root, self.exe, self.report = root, exe, report
        self.sound = root / "share/raw/sound"
        self.rows = [
            {"Name": "tone", "Template": "base", "FileSpec": "tone.wav"},
            {"Name": "music", "Template": "base", "FileSpec": "music.wav", "Storage": "STREAMED"},
            {"Name": "asr", "Template": "base", "FileSpec": "attack.wav",
             "FileSpecSustain": "sustain.wav", "FileSpecRelease": "tail.wav"},
            {"Name": "voice", "Template": "base", "FileSpec": "voice.wav"},
        ]
        write(self.sound / "globals/platform.csv",
              "Platform,ConvertedAssetVersion,ConvertThreadCount,CompressionScale\npc,14,2,1\n")
        write(self.sound / "globals/locale.csv",
              "Name,SearchName,DeployName,CacheName,IsShared,CompressionScale\n"
              "all,all,all,all,true,1\nen,en,en,en,false,1\nfr,fr,fr,fr,false,1\n")
        self.template()
        self.aliases()
        write(root / "project/sound/zoneconfig/audit.szc", json.dumps({
            "Name": "audit", "Sources": [{"Type": "ALIAS", "Name": "audit", "Filename": "audit.csv"}],
        }))
        for name in ["tone", "attack", "sustain", "tail", "voice"]:
            audio(root / f"sound_assets/{name}.wav")
        audio(root / "sound_assets/music.wav", rate=24000, channels=2)
        audio(root / "soundloc_assets/en/voice.wav", amplitude=8000)

    def template(self, volume=77):
        write(self.sound / "templates/base.csv",
              f"Name,Storage,Looping,Compression,VolMin,VolMax\nbase,LOADED,NONLOOPING,100,{volume},{volume}\n")

    def aliases(self):
        write_csv(self.sound / "aliases/audit.csv",
                  ["Name", "Template", "FileSpec", "FileSpecSustain", "FileSpecRelease",
                   "Storage", "Looping", "CompressionLevel"], self.rows)

    def bank(self, lang="all", ext="sabl", cache=False):
        directory = f"sound/zone/CachedBanks/{lang}" if cache else f"zone/snd/{lang}"
        return self.root / f"pc/{directory}/audit.{lang}.{ext}"

    def run(self, label, success=True, languages=("all", "en", "fr")):
        command = [str(self.exe), "pc", "pc", "unused", "sound_zone", "project", "audit", *languages]
        env = dict(os.environ, TA_GAME_PATH=str(self.root), TA_TOOLS_PATH=str(self.root / "tools"))
        start = time.perf_counter()
        result = subprocess.run(command, env=env, cwd=self.root, capture_output=True, text=True, timeout=120)
        self.report.append({"label": label, "command": command, "seconds": time.perf_counter() - start,
                            "returncode": result.returncode, "stdout": result.stdout, "stderr": result.stderr})
        require((result.returncode == 0) == success,
                f"{label}: exit {result.returncode}\n{result.stdout}\n{result.stderr}")
        return result

    def snapshot(self):
        result = {}
        for path in sorted((self.root / "pc").rglob("*")):
            if not path.is_file() or "CachedBanks" in path.parts:
                continue
            key = path.relative_to(self.root / "pc").as_posix()
            if path.suffix in (".sabl", ".sabs"):
                result[key] = read_bank(path)
            else:
                raw = path.read_bytes()
                require(b"\n" not in raw.replace(b"\r\n", b""), f"{path}: sidecar lost CRLF formatting")
                body = raw.decode("utf-8").replace("\r\n", "\n").replace(str(self.root), "<GAME>")
                if path.name.endswith(".assets.sz"):
                    body = "\n".join(sorted(body.splitlines()))
                result[key] = body
        return result

    def clean_equivalent(self, label):
        incremental = self.snapshot()
        # Only generated bank copies inside this temporary project are removed.
        for path in (self.root / "pc").rglob("*"):
            if path.suffix in (".sabl", ".sabs") and path.is_file():
                path.unlink()
        self.run(label + ": clean comparison")
        require(self.snapshot() == incremental, label + ": incremental output differs from a clean build")


def read_bank(path):
    """Independent parser for the fixed little-endian bank layout, not Rust's reader."""
    data = path.read_bytes()
    require(len(data) >= 2048, f"{path}: truncated bank")
    magic, version, entry_size, checksum_size, dep_size, count, deps, build = struct.unpack_from("<8I", data)
    require((magic, version, entry_size, checksum_size, dep_size, deps, build) ==
            (592991538, 15, 36, 16, 64, 8, 2), f"{path}: invalid bank header")
    size, entries, converted = struct.unpack_from("<3q", data, 32)
    source, names = struct.unpack_from("<2q", data, 584)
    require(size == len(data), f"{path}: size mismatch")
    for offset, width in [(entries, 36), (converted, 16), (source, 16), (names, 128)]:
        require(2048 <= offset <= len(data) and offset + count * width <= len(data),
                f"{path}: metadata out of bounds")
    assets = {}
    previous_end = 2048
    for i in range(count):
        entry = data[entries + i * 36:entries + (i + 1) * 36]
        name_hash, length, frames, order, offset = struct.unpack_from("<4IQ", entry)
        rate, channels, looping, fmt = entry[24:28]
        name = data[names + i * 128:names + (i + 1) * 128].split(b"\0", 1)[0].decode("ascii")
        require(name and name not in assets, f"{path}: missing/duplicate name")
        h = 5381
        for char in name.lower():
            h = (ord(char) + h * 65599) & 0xffffffff
        require(name_hash == (h or 1), f"{path}: name hash mismatch")
        require(previous_end <= offset and offset + length <= entries, f"{path}: invalid asset extent")
        previous_end = offset + length
        payload = data[offset:offset + length]
        expected_md5 = data[converted + i * 16:converted + (i + 1) * 16]
        require(hashlib.md5(payload).digest() == expected_md5, f"{path}: payload checksum mismatch")
        require(fmt == 8 and rate == 6 and channels in (1, 2), f"{path}: unexpected audio format")
        if frames:
            require(payload[:4] == b"fLaC" and len(payload) >= 42, f"{path}: invalid FLAC")
            info = int.from_bytes(payload[18:26], "big")
            require((info >> 44, ((info >> 41) & 7) + 1, info & ((1 << 36) - 1)) ==
                    (48000, channels, frames), f"{path}: FLAC/bank metadata disagree")
        assets[name] = {"frames": frames, "channels": channels, "looping": looping,
                        "envelope": entry[28:].hex(), "payload_sha256": digest(payload),
                        "source_checksum": data[source + i * 16:source + (i + 1) * 16].hex()}
    return {"identity": data[600:678].hex(), "assets": dict(sorted(assets.items()))}


def pipeline(exe, report, record=False):
    with tempfile.TemporaryDirectory(prefix="ultrasound-audit-") as directory:
        project = Project(Path(directory), exe, report["runs"])
        report["inputs"] = {p.relative_to(project.root).as_posix(): digest(p.read_bytes())
                            for p in sorted(project.root.rglob("*")) if p.is_file()}
        project.run("cold")
        baseline = project.snapshot()
        report["cold_outputs"] = baseline
        original_bank = project.bank().read_bytes()
        probe_path = project.root / "inspector-probe.sabl"
        for label, offset, replacement, expected_error in [
            ("oversized entry table", 20, struct.pack("<I", 0xffffffff), "metadata out of bounds"),
            ("altered payload", 2048, bytes([original_bank[2048] ^ 1]), "payload checksum mismatch"),
        ]:
            damaged = bytearray(original_bank)
            damaged[offset:offset + len(replacement)] = replacement
            probe_path.write_bytes(damaged)
            try:
                read_bank(probe_path)
            except AssertionError as error:
                require(expected_error in str(error), f"inspector failed for wrong reason: {label}: {error}")
            else:
                raise AssertionError(f"independent inspector accepted {label}")
        loaded = baseline["zone/snd/all/audit.all.sabl"]["assets"]
        require(len(loaded) == 5, "cold: expected five loaded assets")
        require(loaded["sustain.LL100.pc.snd"]["looping"] == 1, "sustain must loop")
        require(len(baseline["zone/snd/en/audit.en.sabl"]["assets"]) == 1, "locale isolation")
        require(not baseline["zone/snd/fr/audit.fr.sabl"]["assets"], "empty locale")
        alias_rows = list(csv.DictReader(io.StringIO(baseline["sound/zone/audit.all.alias.sz"])))
        require(len(alias_rows) == 4 and all(r["VolMin"] == "77" for r in alias_rows), "template inheritance")
        require(next(r for r in alias_rows if r["Name"] == "asr")["FileTargetRelease"] == "tail.LN100.pc.snd",
                "release alias must reference its asset")
        if record:
            reference = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=REPO, text=True).strip()
            write(BASELINE, json.dumps({"reference_commit": reference, "inputs": report["inputs"],
                                       "outputs": baseline}, indent=2, sort_keys=True) + "\n")
        else:
            expected = json.loads(BASELINE.read_text(encoding="utf-8"))
            require(expected["inputs"] == report["inputs"], "fixture inputs changed; review before updating baseline")
            changed_keys = [key for key in sorted(set(expected["outputs"]) | set(baseline))
                            if expected["outputs"].get(key) != baseline.get(key)]
            require(not changed_keys, f"cold outputs differ from reviewed baseline: {changed_keys}")
        for repeat in range(5):
            project.run(f"warm {repeat + 1}")
            require(project.snapshot() == baseline, "warm build changed output contents")
        warm = [r["seconds"] for r in report["runs"] if r.get("label", "").startswith("warm ")]
        report["warm_seconds"] = {"median": statistics.median(warm), "min": min(warm), "max": max(warm)}
        for lang in ("all", "en", "fr"):
            for ext in ("sabl", "sabs"):
                require(project.bank(lang, ext).read_bytes() == project.bank(lang, ext, True).read_bytes(),
                        "cache/deploy mismatch")
        project.bank().unlink()
        project.run("missing deployed bank")
        require(project.snapshot() == baseline, "missing deployment was not restored")
        (project.root / "pc/sound/zone/audit.all.assetcount.sz").unlink()
        project.run("missing sidecar")
        require(project.snapshot() == baseline, "missing sidecar was not restored")
        audio(project.root / "sound_assets/tone.wav", amplitude=4000)
        project.run("one source edit")
        changed = project.snapshot()
        for name, asset in loaded.items():
            now = changed["zone/snd/all/audit.all.sabl"]["assets"][name]
            require((now != asset) == (name == "tone.LN100.pc.snd"), "source edit affected wrong assets")
        require(changed["zone/snd/all/audit.all.sabs"] == baseline["zone/snd/all/audit.all.sabs"], "streamed bank changed")
        project.clean_equivalent("source edit")
        project.template(65)
        project.run("template edit")
        after_template = project.snapshot()
        require(after_template["zone/snd/all/audit.all.sabl"] == changed["zone/snd/all/audit.all.sabl"], "volume changed audio")
        require(after_template["sound/zone/audit.all.alias.sz"] != changed["sound/zone/audit.all.alias.sz"], "template did not propagate")
        project.clean_equivalent("template edit")
        project.rows[0]["CompressionLevel"] = "High"
        project.aliases()
        project.run("compression edit")
        after_compression = project.snapshot()
        require(after_compression["zone/snd/all/audit.all.sabl"]["assets"]["tone.LN100.pc.snd"] !=
                after_template["zone/snd/all/audit.all.sabl"]["assets"]["tone.LN100.pc.snd"], "compression did not invalidate")
        project.clean_equivalent("compression edit")
        project.rows[0]["Looping"] = "LOOPING"
        project.aliases()
        project.run("looping edit")
        loop_assets = read_bank(project.bank())["assets"]
        require("tone.LN100.pc.snd" not in loop_assets and loop_assets["tone.LL100.pc.snd"]["looping"] == 1,
                "looping edit retained stale target or wrong metadata")
        project.clean_equivalent("looping edit")
        project.rows[0].pop("Looping")
        project.rows[0]["Storage"] = "STREAMED"
        project.aliases()
        project.run("storage edit")
        require("tone.LN100.pc.snd" not in read_bank(project.bank())["assets"] and
                "tone.SN100.pc.snd" in read_bank(project.bank(ext="sabs"))["assets"], "storage migration failed")
        project.clean_equivalent("storage edit")
        project.rows[0].pop("Storage")
        project.aliases()
        project.run("restore storage")
        require(project.snapshot() == after_compression, "restoring storage changed asset semantics")
        audio(project.root / "sound_assets/added.wav")
        project.rows.append({"Name": "added", "FileSpec": "added.wav", "Template": "base"})
        project.aliases()
        project.run("asset added")
        require(len(read_bank(project.bank())["assets"]) == 6, "addition missing")
        project.clean_equivalent("asset added")
        project.rows.pop()
        project.aliases()
        project.run("asset removed")
        require(project.snapshot() == after_compression, "removal did not restore previous asset set")
        project.clean_equivalent("asset removed")
        tone = project.root / "sound_assets/tone.wav"
        valid = tone.read_bytes()
        before_error = project.snapshot()
        tone.write_bytes(b"not a wav")
        failed = project.run("invalid source", success=False)
        require("tone.wav" in failed.stderr, "error did not identify source")
        require(project.snapshot() == before_error, "invalid source changed deployed outputs")
        tone.write_bytes(valid)
        project.run("corrected source")
        require(project.snapshot() == before_error, "failed build did not recover")
        project.clean_equivalent("corrected source")
        report["pipeline"] = "passed"


def pipeline_bugs(exe, report):
    with tempfile.TemporaryDirectory(prefix="ultrasound-bugs-") as directory:
        project = Project(Path(directory), exe, report["runs"])
        project.run("B006 setup")
        corrupt = b"corrupt empty bank"
        project.bank("fr", cache=True).write_bytes(corrupt)
        project.run("B006 corrupt empty cache", languages=("fr",))
        require(project.bank("fr").read_bytes() == corrupt,
                "B006 no longer reproduces; promote corrupt-cache recovery to required tests")
        report["known_bugs"]["B006"] = "reproduced: corrupt empty bank deployed successfully"
        audio(project.root / "sound_assets/variations/one.wav")
        audio(project.root / "sound_assets/variations/two.wav", amplitude=6000)
        project.rows = [{"Name": "variation", "Template": "base", "FileSpec": "variations"}]
        project.aliases()
        project.run("B007 folder filespec", languages=("all",))
        assets = read_bank(project.bank())["assets"]
        rows = list(csv.DictReader((project.root / "pc/sound/zone/audit.all.alias.sz").read_text().splitlines()))
        require(len(assets) == 2 and len(rows) == 1 and rows[0]["FileTarget"] == "variations\\one.LN100.pc.snd",
                "B007 behavior changed; review folder semantics and update its acceptance tests")
        report["known_bugs"]["B007"] = "reproduced: two bank assets, only first referenced"
        old_bank = project.bank().read_bytes()
        alias_path = project.root / "pc/sound/zone/audit.all.alias.sz"
        old_alias = alias_path.read_bytes()
        project.template(65)
        (project.root / "sound_assets/variations/one.wav").write_bytes(b"invalid audio")
        failed = project.run("B009 failed conversion after alias edit", success=False, languages=("all",))
        require("one.wav" in failed.stderr and project.bank().read_bytes() == old_bank and
                alias_path.read_bytes() != old_alias, "B009 behavior changed; review atomic output acceptance")
        report["known_bugs"]["B009"] = "reproduced: failed build publishes new aliases beside old bank"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--external", action="store_true", help="require the private test_data corpus")
    parser.add_argument("--known-bugs", action="store_true", help="verify failures have the documented cause")
    parser.add_argument("--record-baseline", action="store_true", help="explicitly replace the reviewed default-feature snapshot")
    parser.add_argument("--report", type=Path, default=REPO / "target/audit/report.json")
    args = parser.parse_args()
    report = {"machine": platform.platform(), "runs": [], "known_bugs": {},
              "external": "not requested; external compatibility coverage skipped",
              "real_world_build_and_playback": "not run"}

    def command(argv):
        start = time.perf_counter()
        result = subprocess.run(argv, cwd=REPO, capture_output=True, text=True, timeout=600)
        report["runs"].append({"command": argv, "seconds": time.perf_counter() - start,
                                "returncode": result.returncode, "stdout": result.stdout, "stderr": result.stderr})
        return result

    def checked(argv):
        print("Running", " ".join(argv), flush=True)
        result = command(argv)
        require(result.returncode == 0, result.stdout + result.stderr)
        print(result.stdout.strip(), flush=True)

    try:
        report["commit"] = command(["git", "rev-parse", "HEAD"]).stdout.strip()
        report["working_tree"] = command(["git", "status", "--short"]).stdout
        report["rust"] = command(["rustc", "--version"]).stdout.strip()
        report["libflac_sha256"] = digest((REPO / "lib/libFLAC.dll").read_bytes())
        checked([os.sys.executable, "-m", "unittest", "discover", "-s", "tests", "-p", "test_*.py"])
        checked(["cargo", "test", "--locked"])
        checked(["cargo", "test", "--locked", "--all-features"])
        if args.known_bugs:
            for bug, name in BUGS.items():
                full_name = "regression_tests::known_bugs::" + name
                result = command(["cargo", "test", "--locked", full_name, "--", "--ignored", "--exact", "--nocapture"])
                require(result.returncode != 0 and f"{bug}:" in result.stderr and "1 failed" in result.stdout,
                        f"{bug}: expected failure changed or test did not run\n{result.stdout}\n{result.stderr}")
                report["known_bugs"][bug] = "reproduced at documented assertion"
                print(bug, "reproduced", flush=True)
        if args.external:
            report["external_inputs"] = {
                p.relative_to(REPO).as_posix(): digest(p.read_bytes())
                for p in sorted((REPO / "test_data").rglob("*")) if p.is_file()
            }
            checked(["cargo", "test", "--locked", "--", "--ignored", "--skip", "regression_tests::known_bugs",
                     "--skip", "lossy_prepass_shrinks_flac", "--test-threads=1"])
            report["external"] = "passed (fixture compatibility only; no game build/playback)"
        checked(["cargo", "build", "--locked", "--release"])
        exe = REPO / "target/release/ultrasound.exe"
        report["executable_sha256"] = digest(exe.read_bytes())
        pipeline(exe, report, args.record_baseline)
        if args.known_bugs:
            pipeline_bugs(exe, report)
        report["status"] = "passed"
        print("Audit passed. External coverage:", report["external"])
    except Exception as error:
        report["status"] = "failed"
        report["error"] = str(error)
        raise
    finally:
        write(args.report.resolve(), json.dumps(report, indent=2) + "\n")
        print("Report:", args.report.resolve(), flush=True)


if __name__ == "__main__":
    main()
