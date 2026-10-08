"""Exercise background self-update against a fake release feed, entirely inside a temporary sandbox.

The release executable is copied to <sandbox>/sound/snd_convert.exe. LOCALAPPDATA and
GSCODE_UPDATE_FEED point into the sandbox, so neither the real state folder, the network nor any game
install is touched.
"""
import argparse
import ctypes
from ctypes import wintypes
import http.server
import json
import os
import re
from pathlib import Path
import subprocess
import tempfile
import threading
import time
import zipfile

from audit import Project, digest, require

FILES = ("snd_convert.exe", "libFLAC.dll")
VERSION = re.search(r'^version = "([^"]+)"', Path(__file__).resolve().parents[1].joinpath("Cargo.toml").read_text(),
                    re.MULTILINE)[1]
RELEASE_PAGE = "https://example.invalid/ultrasound/releases/tag/v9.9.9"
INSTALLER = "https://example.invalid/ultrasound/releases/download/v9.9.9/ultrasound-setup.exe"


class SlowFeed:
    """Serves the release JSON over HTTP after a delay, so the updater is provably still running."""

    def __init__(self):
        self.body = b"{}"
        self.delay = 0.0
        self.started = threading.Event()
        self.finished = threading.Event()
        feed = self

        class Handler(http.server.BaseHTTPRequestHandler):
            def do_GET(self):
                feed.started.set()
                time.sleep(feed.delay)
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(feed.body)))
                self.end_headers()
                self.wfile.write(feed.body)
                feed.finished.set()

            def log_message(self, *args):
                pass

        self.server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        threading.Thread(target=self.server.serve_forever, daemon=True).start()
        self.url = f"http://127.0.0.1:{self.server.server_address[1]}/latest.json"

    def arm(self, body, delay):
        self.body, self.delay = body, delay
        self.started.clear()
        self.finished.clear()


class Sandbox:
    def __init__(self, root, exe):
        self.root = root
        self.sound = root / "game/sound"
        self.sound.mkdir(parents=True)
        self.original = {}
        for name, source in (("snd_convert.exe", exe), ("libFLAC.dll", exe.with_name("libFLAC.dll"))):
            data = source.read_bytes()
            (self.sound / name).write_bytes(data)
            self.original[name] = data
        self.local = root / "localappdata"
        self.feed_dir = root / "feed"
        self.feed_dir.mkdir()
        self.runs = []
        self.project = Project(root / "game", self.sound / "snd_convert.exe", self.runs)
        os.environ["LOCALAPPDATA"] = str(self.local)
        self.http = SlowFeed()

    # Release fixtures ---------------------------------------------------------------------------

    def bundle(self, version, marker):
        """A contract bundle whose files are the current build plus a trailing marker (PE overlay)."""
        path = self.feed_dir / f"ultrasound-{version}-win-x64.zip"
        with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as zf:
            for name in FILES:
                zf.writestr(f"sound/{name}", self.original[name] + marker)
        return path

    def release(self, version, marker, digest_value=None):
        zip_path = self.bundle(version, marker)
        return json.dumps({
            "tag_name": f"v{version}",
            "html_url": f"https://example.invalid/ultrasound/releases/tag/v{version}",
            "draft": False, "prerelease": False, "body": "test",
            "assets": [{"name": zip_path.name, "browser_download_url": zip_path.as_uri(),
                        "digest": digest_value or "sha256:" + digest(zip_path.read_bytes())},
                       {"name": "ultrasound-setup.exe", "browser_download_url": INSTALLER, "digest": None}],
        }).encode()

    def file_feed(self, body):
        path = self.feed_dir / "latest.json"
        path.write_bytes(body)
        os.environ["GSCODE_UPDATE_FEED"] = path.as_uri()

    # State --------------------------------------------------------------------------------------

    def state_path(self):
        found = list((self.local / "gscode/ultrasound").glob("*/state.json"))
        require(len(found) <= 1, f"expected one install state, found {found}")
        return found[0] if found else None

    def state(self):
        path = self.state_path()
        return json.loads(path.read_text()) if path else {}

    def prime(self, last_check):
        """Make the next successful run start a check (last_check=0) or skip it (now)."""
        path = self.state_path()
        state = self.state()
        state.pop("last_result", None)
        state["last_check"] = last_check
        if path:
            path.write_text(json.dumps(state))

    def wait_result(self, timeout=90):
        deadline = time.time() + timeout
        while time.time() < deadline:
            state = self.state()
            if "last_result" in state:
                # The updater saves state last; give it a moment to exit and release the folder.
                time.sleep(0.5)
                return state
            time.sleep(0.1)
        raise AssertionError("background update did not record a result")

    # Runs ---------------------------------------------------------------------------------------

    def run(self, label):
        return self.project.run(label, languages=("all",))

    def notices(self, result):
        return [line for line in result.stdout.splitlines()
                if line.startswith(("Ultrasound ", "Couldn't install Ultrasound ")) and line != f"Ultrasound {VERSION}"]

    def files(self):
        return {p.name: p.read_bytes() for p in sorted(self.sound.iterdir())}

    def require_untouched(self, label):
        require(self.files() == self.original, f"{label}: installed files changed: {sorted(self.files())}")


def job_object(kill_on_close, breakaway_ok):
    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)

    class Basic(ctypes.Structure):
        _fields_ = [("PerProcessUserTimeLimit", ctypes.c_int64), ("PerJobUserTimeLimit", ctypes.c_int64),
                    ("LimitFlags", wintypes.DWORD), ("MinimumWorkingSetSize", ctypes.c_size_t),
                    ("MaximumWorkingSetSize", ctypes.c_size_t), ("ActiveProcessLimit", wintypes.DWORD),
                    ("Affinity", ctypes.c_size_t), ("PriorityClass", wintypes.DWORD),
                    ("SchedulingClass", wintypes.DWORD)]

    class Extended(ctypes.Structure):
        _fields_ = [("Basic", Basic), ("Io", ctypes.c_uint64 * 6), ("ProcessMemoryLimit", ctypes.c_size_t),
                    ("JobMemoryLimit", ctypes.c_size_t), ("PeakProcessMemoryUsed", ctypes.c_size_t),
                    ("PeakJobMemoryUsed", ctypes.c_size_t)]

    kernel32.CreateJobObjectW.restype = wintypes.HANDLE
    job = kernel32.CreateJobObjectW(None, None)
    require(job, "CreateJobObjectW failed")
    info = Extended()
    info.Basic.LimitFlags = (0x2000 if kill_on_close else 0) | (0x800 if breakaway_ok else 0)
    require(kernel32.SetInformationJobObject(wintypes.HANDLE(job), 9, ctypes.byref(info), ctypes.sizeof(info)),
            "SetInformationJobObject failed")
    return kernel32, job


def outer_job_allows_breakaway():
    """Breakaway from our test job also needs every enclosing job (e.g. a CI runner's) to allow it."""
    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    in_job = wintypes.BOOL()
    kernel32.IsProcessInJob(wintypes.HANDLE(kernel32.GetCurrentProcess()), None, ctypes.byref(in_job))
    if not in_job.value:
        return True
    buffer = (ctypes.c_byte * 144)()
    if not kernel32.QueryInformationJobObject(None, 9, buffer, ctypes.sizeof(buffer), None):
        return False
    flags = int.from_bytes(bytes(buffer[16:20]), "little")
    return bool(flags & 0x1800)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exe", type=Path, default=Path("target/release/ultrasound.exe"))
    parser.add_argument("--out", type=Path, default=Path("target/audit/update.json"))
    args = parser.parse_args()
    exe = args.exe.resolve(strict=True)
    report = {"executable_sha256": digest(exe.read_bytes()), "cases": []}
    saved_env = {k: os.environ.get(k) for k in ("LOCALAPPDATA", "GSCODE_UPDATE_FEED")}

    def case(name, **details):
        report["cases"].append({"case": name, **details})
        print("passed:", name, flush=True)

    try:
        with tempfile.TemporaryDirectory(prefix="ultrasound-update-", ignore_cleanup_errors=True) as directory:
            box = Sandbox(Path(directory), exe)
            report["runs"] = box.runs

            help_text = subprocess.run([str(box.sound / "snd_convert.exe"), "--help"],
                                       capture_output=True, text=True, timeout=30).stdout
            require("--update" not in help_text and "--verbose" in help_text, "--update must stay out of --help")
            case("--update hidden from help")

            version_text = subprocess.run([str(box.sound / "snd_convert.exe"), "--version"],
                                          capture_output=True, text=True, timeout=30).stdout.strip()
            require(version_text == f"Ultrasound {VERSION}", f"--version: {version_text!r}")
            case("--version names the product", line=version_text)

            # A copy outside a Black Ops III folder (no BlackOps3.exe or share beside its sound folder)
            # never checks, even with a newer release published.
            loose = box.root / "elsewhere/sound"
            loose.mkdir(parents=True)
            for name in FILES:
                (loose / name).write_bytes(box.original[name])
            box.file_feed(box.release("9.9.9", b"LOOSE"))
            Project(box.root / "game", loose / "snd_convert.exe", box.runs).run("loose copy: build", languages=("all",))
            time.sleep(3)
            require(box.state_path() is None, "a copy outside Black Ops III recorded update state")
            require(sorted(p.name for p in loose.iterdir()) == sorted(FILES), "a loose copy changed its files")
            case("a copy outside Black Ops III never updates")

            # Offline: nothing listening; the check fails quietly and is recorded only in state.
            os.environ["GSCODE_UPDATE_FEED"] = "http://127.0.0.1:9/latest.json"
            first = box.run("offline: build")
            require(not box.notices(first), "offline: unexpected notice")
            state = box.wait_result()
            require(state["last_result"].startswith("release check failed"), f"offline: {state}")
            require(not box.notices(box.run("offline: next build")), "offline must stay silent")
            box.require_untouched("offline")
            require(not any(box.root.joinpath("game").rglob("state.json")), "state must not live in the game folder")
            case("offline is silent", result=state["last_result"])

            # Throttle: a check ran less than a day ago, so a newer release is not even looked at.
            box.file_feed(box.release("9.9.9", b"THROTTLE"))
            box.prime(int(time.time()) - 60)
            box.run("throttle: build")
            time.sleep(3)
            require("last_result" not in box.state(), "throttle: a check ran within a day of the last")
            box.require_untouched("throttle")
            case("checks at most once a day")

            # Digest mismatch: nothing installed; the release is announced on every run until resolved.
            box.file_feed(box.release("9.9.9", b"MISMATCH", digest_value="sha256:" + "0" * 64))
            box.prime(0)
            box.run("mismatch: build")
            state = box.wait_result()
            require("digest mismatch" in state["last_result"], f"mismatch: {state}")
            box.require_untouched("mismatch")
            mismatch = ("Couldn't install Ultrasound 9.9.9. The download didn't match its checksum, so it wasn't kept. "
                        f"Download the installer: {INSTALLER}")
            for attempt in (1, 2):
                notices = box.notices(box.run(f"mismatch: next build {attempt}"))
                require(notices == [mismatch], f"mismatch run {attempt}: {notices}")
            case("digest mismatch leaves files and repeats the notice", line=mismatch)

            # Locked file: the DLL cannot be renamed, so the exe already swapped in is rolled back.
            box.file_feed(box.release("9.9.9", b"LOCKED"))
            box.prime(0)
            with open(box.sound / "libFLAC.dll", "rb"):
                box.run("locked: build")
                state = box.wait_result()
            require("previous files restored" in state["last_result"], f"locked: {state}")
            box.require_untouched("locked")
            locked = ("Couldn't install Ultrasound 9.9.9. A file was in use or couldn't be replaced. "
                      f"Nothing was changed. Download the installer: {INSTALLER}")
            notices = box.notices(box.run("locked: next build"))
            require(notices == [locked], f"locked: {notices}")
            case("locked file rolls back and announces", result=state["last_result"])

            # Success, with the linker's view: stdout/stderr through pipes reach EOF while the updater
            # is still waiting on the (deliberately slow) feed.
            marker = b"ULTRASOUND-UPDATE-TEST-9.9.9"
            box.http.arm(box.release("9.9.9", marker), delay=8)
            os.environ["GSCODE_UPDATE_FEED"] = box.http.url
            box.prime(0)
            command = [str(box.project.exe), "pc", "pc", "unused", "sound_zone", "project", "audit", "all"]
            env = dict(os.environ, TA_GAME_PATH=str(box.project.root), TA_TOOLS_PATH=str(box.project.root / "tools"))
            start = time.perf_counter()
            process = subprocess.Popen(command, env=env, cwd=box.project.root,
                                       stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            out, err = process.communicate(timeout=60)
            eof = time.perf_counter() - start
            require(process.returncode == 0, f"piped build failed: {out!r} {err!r}")
            require(box.http.started.wait(10), "the updater never requested the feed")
            require(not box.http.finished.is_set(), f"EOF only after the updater finished ({eof:.2f}s)")
            require(eof < 6, f"pipes reached EOF after {eof:.2f}s")
            state = box.wait_result()
            require(state["last_result"] == "installed 9.9.9", f"install: {state}")
            files = box.files()
            for name in FILES:
                require(files[name] == box.original[name] + marker, f"{name} was not replaced")
                require(files[name + ".old"] == box.original[name], f"{name}.old is not the previous file")
            require(sorted(files) == sorted([*FILES, *(f + ".old" for f in FILES)]), f"stray files: {sorted(files)}")
            case("piped run reaches EOF while the update runs", eof_seconds=round(eof, 3))

            updated = f"Ultrasound 9.9.9 is installed. You had {VERSION}. Release notes: {RELEASE_PAGE}"
            notices = box.notices(box.run("installed: next build"))
            require(notices == [updated], f"updated notice: {notices}")
            require(sorted(box.files()) == sorted(FILES), f".old files not cleaned: {sorted(box.files())}")
            require(not box.notices(box.run("installed: build after")), "updated notice must show once")
            case("update swaps files, cleans .old, reports once", line=updated)

            # Job objects: a job that refuses breakaway still gets its update (started inside the job);
            # a kill-on-close job that allows breakaway does not take the updater down with it.
            kernel32, _ = job_object(False, False)
            scenarios = [("job refusing breakaway", False, False, b"JOB-NO-BREAKAWAY")]
            if outer_job_allows_breakaway():
                scenarios.append(("kill-on-close job allowing breakaway", True, True, b"JOB-BREAKAWAY"))
            else:
                report["skipped"] = "kill-on-close breakaway: an enclosing job forbids breakaway"
                print("skipped: kill-on-close breakaway (an enclosing job forbids breakaway)", flush=True)
            for label, kill, breakaway, job_marker in scenarios:
                current = box.files()
                box.http.arm(box.release("9.9.9", job_marker), delay=3)
                box.prime(0)
                _, job = job_object(kill, breakaway)
                process = subprocess.Popen(command, env=env, cwd=box.project.root,
                                           stdout=subprocess.PIPE, stderr=subprocess.PIPE)
                require(kernel32.AssignProcessToJobObject(ctypes.c_void_p(job), ctypes.c_void_p(int(process._handle))),
                        f"{label}: AssignProcessToJobObject failed")
                out, err = process.communicate(timeout=60)
                require(process.returncode == 0, f"{label}: build failed {out!r} {err!r}")
                require(box.http.started.wait(10), f"{label}: the updater never started")
                kernel32.CloseHandle(ctypes.c_void_p(job))
                state = box.wait_result()
                require(state["last_result"] == "installed 9.9.9", f"{label}: {state}")
                require(box.files()["snd_convert.exe"].endswith(job_marker), f"{label}: not installed")
                require(box.files()["snd_convert.exe.old"] == current["snd_convert.exe"], f"{label}: wrong .old")
                box.run(f"{label}: cleanup build")
                case(label)
    finally:
        for key, value in saved_env.items():
            if value is None:
                os.environ.pop(key, None)
            else:
                os.environ[key] = value
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_text(json.dumps(report, indent=2, default=str))
    print(f"{len(report['cases'])} update cases passed: {args.out}")


if __name__ == "__main__":
    main()
