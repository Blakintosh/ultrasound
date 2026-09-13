"""Fingerprint explicitly selected real-world inputs/outputs without running a build."""
import argparse
import hashlib
import json
from pathlib import Path
import platform


def file_hash(path):
    result = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            result.update(block)
    return result.hexdigest()


def capture(root, patterns):
    files = {}
    if not patterns:
        raise ValueError("Each input/output list must contain at least one pattern")
    for pattern in patterns:
        if Path(pattern).is_absolute() or ".." in Path(pattern).parts:
            raise ValueError(f"Pattern must stay relative to game root: {pattern}")
        matches = [p for p in root.glob(pattern) if p.is_file()]
        if not matches:
            raise ValueError(f"No files matched required pattern: {pattern}")
        for path in matches:
            path.resolve().relative_to(root)
            files[path.relative_to(root).as_posix()] = {"bytes": path.stat().st_size, "sha256": file_hash(path)}
    return dict(sorted(files.items()))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--exe", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--compare", type=Path)
    args = parser.parse_args()
    root = args.root.resolve(strict=True)
    spec = json.loads(args.manifest.read_text(encoding="utf-8"))
    if not spec.get("invocation") or not spec.get("name"):
        raise ValueError("Manifest requires a corpus name and the exact invocation used")
    result = {"name": spec["name"], "invocation": spec["invocation"], "root": str(root),
              "machine": platform.platform(), "executable_sha256": file_hash(args.exe),
              "inputs": capture(root, spec["inputs"]), "outputs": capture(root, spec["outputs"]),
              "playback": "not assessed by this capture tool"}
    changed = {}
    if args.compare:
        old = json.loads(args.compare.read_text(encoding="utf-8"))
        if old["name"] != result["name"] or old["invocation"] != result["invocation"]:
            raise ValueError("Cannot compare different corpus names or invocations")
        for section in ("inputs", "outputs"):
            changed[section] = [name for name in sorted(set(old[section]) | set(result[section]))
                                if old[section].get(name) != result[section].get(name)]
        result["comparison"] = changed
    out = args.out.resolve()
    protected = [root / p for section in ("inputs", "outputs") for p in result[section]]
    protected.extend([args.exe.resolve(), args.manifest.resolve()])
    if args.compare:
        protected.append(args.compare.resolve())
    if out in [p.resolve() for p in protected]:
        raise ValueError("Capture destination would overwrite an input, output, or reference")
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(f"Captured {len(result['inputs'])} inputs and {len(result['outputs'])} outputs to {out}")
    if any(changed.values()):
        print(json.dumps(changed, indent=2))
        raise SystemExit(1)


if __name__ == "__main__":
    main()
