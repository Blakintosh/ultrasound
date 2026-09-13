"""Compare journal validation with full hashing using real NTFS files and CLI builds."""
import argparse
import json
import mmap
import os
from pathlib import Path
import re
import tempfile

from audit import Project, digest, require


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate", type=Path, default=Path("target/release/ultrasound.exe"))
    parser.add_argument("--reference", type=Path, help="defaults to candidate with full hashing forced")
    parser.add_argument("--out", type=Path, default=Path("target/audit/journal-parity.json"))
    args = parser.parse_args()
    candidate = args.candidate.resolve()
    reference = (args.reference or args.candidate).resolve()
    report = {"binaries": {"candidate": digest(candidate.read_bytes()), "reference": digest(reference.read_bytes())},
              "cases": [], "runs": []}
    with tempfile.TemporaryDirectory(prefix="ultrasound-journal-parity-") as directory:
        project = Project(Path(directory), candidate, report["runs"])
        source = project.root / "sound_assets/tone.wav"

        def hits(result):
            return sum(map(int, re.findall(r"source validation: (\d+) journal hits", result.stdout)))

        def compare(label, success=True, expect_hits=None):
            project.exe = candidate
            result = project.run(label + ": journal", success=success, languages=("all",))
            fast = project.snapshot()
            if expect_hits is not None:
                require(hits(result) == expect_hits, f"{label}: expected {expect_hits} hits, got {hits(result)}")
            project.exe = reference
            previous = os.environ.get("ULTRASOUND_FULL_HASH")
            os.environ["ULTRASOUND_FULL_HASH"] = "1"
            try:
                project.run(label + ": full hash", success=success, languages=("all",))
            finally:
                if previous is None:
                    del os.environ["ULTRASOUND_FULL_HASH"]
                else:
                    os.environ["ULTRASOUND_FULL_HASH"] = previous
            require(project.snapshot() == fast, label + ": journal output differs from full hashing")
            report["cases"].append({"case": label, "journal_hits": hits(result), "parity": True})
            return fast

        def change():
            with source.open("r+b") as stream:
                stream.seek(100)
                value = stream.read(1)[0]
                stream.seek(100)
                stream.write(bytes([value ^ 0x40]))

        compare("cold", expect_hits=0)
        baseline = compare("unchanged", expect_hits=6)
        times = source.stat()
        change()
        os.utime(source, ns=(times.st_atime_ns, times.st_mtime_ns))
        changed = compare("same-size edit with restored timestamp", expect_hits=5)
        require(changed != baseline, "source edit must alter bank output")

        original = source.read_bytes()
        source.write_bytes(original)
        require(compare("identical bytes rewritten", expect_hits=5) == changed, "identical rewrite changed output")

        times = source.stat()
        replacement = source.with_name("replacement.wav")
        replacement.write_bytes(source.read_bytes())
        source.rename(source.with_name("old.wav"))
        replacement.rename(source)
        change()
        os.utime(source, ns=(times.st_atime_ns, times.st_mtime_ns - 10_000_000_000))
        compare("replacement with older timestamp", expect_hits=5)

        link = source.with_name("hardlink.wav")
        os.link(source, link)
        compare("hard link created")
        with link.open("r+b") as stream:
            stream.seek(100)
            stream.write(b"\x33")
        compare("edit through hard link", expect_hits=5)

        with source.open("r+b") as stream:
            stream.seek(100)
            stream.write(b"\x55")
            stream.flush()
            compare("writer still open", expect_hits=5)
        compare("writer closed", expect_hits=5)
        compare("after writer recovery", expect_hits=6)

        with source.open("r+b") as stream:
            mapping = mmap.mmap(stream.fileno(), 0, access=mmap.ACCESS_WRITE)
        try:
            mapping[100] = 0x22
            mapping.flush()
            compare("writable mapping after file handle closed", expect_hits=5)
            mapping[100] = 0x66
            mapping.flush()
            compare("second write through same mapping", expect_hits=5)
        finally:
            mapping.close()
        compare("mapping closed", expect_hits=5)
        compare("after mapping recovery", expect_hits=6)

        for field in ("journal", "usn", "file", "volume"):
            for path in (project.root / "pc").rglob("*.usn.json"):
                records = json.loads(path.read_text())
                for record in records.values():
                    record["stamp"][field] = "wrong-volume" if field == "volume" else 0
                path.write_text(json.dumps(records))
            compare("stale cache " + field, expect_hits=0)
            compare("recovered cache " + field, expect_hits=6)

        for path in (project.root / "pc").rglob("*.usn.json"):
            path.write_bytes(b"{truncated")
        compare("malformed cache", expect_hits=0)

        for checkpoint in (0, 2**63 - 1):
            for path in (project.root / "pc").rglob("*.usn.json"):
                records = json.loads(path.read_text())
                for record in records.values():
                    record["checkpoint"] = checkpoint
                path.write_text(json.dumps(records))
            compare(f"unusable checkpoint {checkpoint}", expect_hits=0)
            compare(f"checkpoint recovery {checkpoint}", expect_hits=6)

        saved = source.read_bytes()
        source.unlink()
        compare("deleted source", success=False)
        source.write_bytes(saved)
        compare("recreated source", expect_hits=5)

        other = source.with_name("other.wav")
        other.write_bytes(saved)
        os.utime(other, ns=(1_000_000_000, 1_000_000_000))
        project.rows[0]["FileSpec"] = other.name
        project.aliases()
        compare("alias redirected to older file", expect_hits=5)
        project.rows[0]["FileSpec"] = source.name
        project.aliases()
        compare("alias restored", expect_hits=5)

        folder = source.parent / "variations"
        folder.mkdir()
        (folder / "one.wav").write_bytes(saved)
        project.rows.append({"Name": "variation", "Template": "base", "FileSpec": "variations"})
        project.aliases()
        first = compare("folder filespec added")
        (folder / "two.wav").write_bytes(saved)
        second = compare("folder gains a file")
        require(first != second, "folder addition must change output")
        (folder / "two.wav").rename(folder / "three.wav")
        third = compare("folder member renamed")
        require(second != third, "folder rename must change output")
        (folder / "three.wav").unlink()
        require(compare("folder member removed") == first, "folder removal must restore output")

        os.environ["ULTRASOUND_FULL_HASH"] = "1"
        try:
            compare("explicit full hashing", expect_hits=0)
        finally:
            del os.environ["ULTRASOUND_FULL_HASH"]

    report["status"] = "passed"
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(report, indent=2))
    print(f"{len(report['cases'])} parity cases passed: {args.out}")


if __name__ == "__main__":
    main()
