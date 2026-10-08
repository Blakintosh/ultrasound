# Testing changes toward v1

The compatibility reference is `5ebfe3a` (v0.3.1). Work starts on
`codex/v1-regression-baseline`; the package version stays unchanged until release
behavior changes. The expected snapshot now includes the documented v1 corrections
below; its generated inputs are unchanged. Do not treat the reference as proof
that every original behavior is correct.

## Run the audit

On Windows, install Rust with the MSVC toolchain, its C/C++ build tools, and Python
3.10 or newer. The repository supplies libFLAC. No Python packages are required.

```powershell
python scripts/audit.py --known-bugs
```

This runs Rust tests with default and all features, including native NTFS
validation tests, checks the known
unit failures individually, builds the default release executable, and runs it
against generated inputs in an isolated temporary game directory. Environment
variables are set on child processes only. It does not use your installed game.

`target/audit/report.json` records commands, exit codes, logs, timings, compiler,
commit, working-tree status, executable/DLL hashes and fixture hashes. CI runs
the same command and uploads this report even on failure. CI has been configured;
a local run does not prove the hosted workflow has run successfully. The native
validation tests require an active NTFS journal on the temporary directory's
volume. They fail if they cannot exercise actual checksum reuse.

## Journal validation

Local NTFS inputs can reuse their last checksum when their volume, journal,
file identity, file USN, size and conversion recipe match. A retained validation
checkpoint is also required. Checkpoints record when content was validated, so
an old source file can benefit even after its original change record expires.
The source is opened without write/delete sharing while checking or reading it.
An open writer or writable mapping causes full hashing without saving a stamp.

Unknown journal state, expired checkpoints, unsupported storage, missing cache
records and malformed cache JSON all cause full hashing. Validation records live
beside cached banks as `*.usn.json`; deleting them costs a fresh hash pass. Output
banks still undergo full byte comparison before deployment. Tables and FileSpec
directories are still evaluated on every build.

Set `ULTRASOUND_FULL_HASH` to any value to force source hashing. This is also the
appropriate mode for checking content after suspected silent corruption or
writes outside normal filesystem operations. Journal validation tracks filesystem
changes; it does not provide an independent disk-integrity scan. Neither mode
creates a project-wide snapshot while other programs are editing the project.

Run the dedicated parity tests after building release:

```powershell
python scripts/test_journal.py
python scripts/test_journal.py --reference target/audit/pre-journal/ultrasound.exe
```

Without a reference argument, the script compares the candidate's journal path
with its forced-hashing path. With a reference, it compares against that binary.
Both compare independently parsed bank metadata, encoded payload hashes and
sidecars, and require real cache hits. CI runs the first command after the audit.

The cases include timestamp-preserving edits, older replacements, hard links,
open writers, repeated mapped writes after the original file handle closes,
unusable checkpoints, changed journal/file identities, malformed records,
deletion/recreation, alias redirection, and folder additions/renames/removals.
Journal resets and retention gaps are simulated in cached state/unit tests;
the tests never delete or resize the machine's journal. Folder tests preserve
the current B007 behavior, not a completed variation feature.

For repeated timings with cache-hit assertions:

```powershell
python scripts/benchmark.py --reference target/audit/pre-journal/ultrasound.exe --candidate target/release/ultrasound.exe --assets 128 --seconds 20 --repeats 7 --require-journal-hits --out target/audit/journal-performance-final.json
```

Run benchmarks separately from compilation and other test suites. The reference
path above is a locally retained binary, not a tracked repository file.

## Self-update

Run after building release:

```powershell
python scripts/test_update.py
```

It copies the release executable and `libFLAC.dll` into a temporary `sound`
folder as `snd_convert.exe`, points `LOCALAPPDATA` and `GSCODE_UPDATE_FEED` into
the sandbox, and serves fake newer releases from `file://` and a local HTTP feed.
No network, real state folder or game install is used. Bundles are the current
build with a trailing marker, so swapped files are identifiable and still run.

Cases: the hidden `--update` flag stays out of `--help`; `--version` prints
`Ultrasound X.Y.Z`; a copy whose `sound` folder has neither `BlackOps3.exe` nor
`share` beside it never checks; an unreachable feed is silent; a second check
within a day is skipped; a digest mismatch installs nothing and repeats the
"Couldn't install" line with its reason and the installer link; a DLL held open
without delete sharing rolls back the already-swapped exe; a piped build reaches
EOF while the updater is still waiting on a slow feed, after which both files are
swapped, `.old` copies are cleaned by the next build and the "is installed" line
prints once; and an update still installs from inside a job that refuses breakaway.
A kill-on-close job that allows breakaway must not stop the updater; that case
is skipped when an enclosing job (some CI runners and agent shells) forbids
breakaway. `target/audit/update.json` records the runs.

Development builds named `ultrasound.exe` never check for updates, so the audit
and benchmarks are unaffected.

## Coverage and limits

| Area | Required portable checks | Additional evidence still needed |
| --- | --- | --- |
| WAV | Exact PCM decode; mono/stereo conversion; 24 to 48 kHz conversion | More export formats and audible resampling quality |
| FLAC/OGG | Generated codec tests, mono/stereo, frame/channel checks; FLAC sample comparison | Full CLI input coverage for FLAC/OGG, listening |
| Aliases | Template inheritance, generated targets, sustain/release, looping/storage transitions | Full configuration semantics and game playback |
| Locales | Shared, English-only voice, empty French banks | A representative multilingual map |
| Banks | Independent Python layout reader, bounds/name/payload checks, metadata and FLAC STREAMINFO agreement | Downstream linker and engine compatibility |
| Incremental builds | Source/template/compression edits; add/remove; clean-build equivalence | Large maps and overlapping concurrent invocations |
| Recovery | Missing bank/sidecar; invalid source followed by correction | Interrupted writes and disk-full behavior |
| Sidecars | Exact normalized synthetic outputs; existing portable writer tests | External ambient/reverb/music/duck/scriptid fixtures and map build |
| Performance | Release subprocess timings; five warm repetitions | Repeated cold/edit measurements on a large fixed corpus, peak RAM and listening |

The synthetic corpus is generated by `scripts/audit.py`; no copyrighted audio is
needed. `tests/fixtures/baseline.json` pins its input hashes and normalized outputs.
The reader checks the emitted bank independently of the Rust reader. It compares
encoded payload hashes and metadata, not decoded audio from the full CLI. The
separate codec tests compare decoded samples. These checks complement each other;
neither replaces an engine test.

Absolute temporary paths are replaced with `<GAME>`. Asset-list ordering is
normalized because incremental and clean builds may pack assets in different
orders. Names, metadata, payloads and other sidecar contents remain compared.
The updated reference requires lossless encoding when no lossy level is selected.
The original snapshot remains available in commit `96026a6`. The snapshot changes
were limited to recipe checksums, encoded payloads and their memory totals; alias
contents and the existing long-clip metadata remained unchanged.

Timing reports describe this tiny fixture and filesystem-warm runs. They are not
speedup claims, and CI has no noisy wall-clock threshold. Measure a large corpus
before selecting an optimization.

## External fixture tests

Existing tests that read `test_data` have explicit `#[ignore]` reasons. A normal
`cargo test` reports them as ignored. To require them:

```powershell
python scripts/audit.py --known-bugs --external
```

Restore your existing `test_data` corpus at the repository root. It is not fetched
or redistributed by the audit. The current corpus contains:

- `core_patch.all.sabl`
- `zm_kar_roundend_normal.flac`, `zm_kar_roundstart_normal.ogg`
- `template_rottsky.csv`, `zm_karelia_sfx.csv`, `zm_karelia.szc`
- `ambient_zm_karelia.csv`, `common_reverb.csv`, `zm_karelia_reverb.csv`
- `blak_health_low.duk`, `zm_karelia.mus`
- `baseline_outputs/zm_karelia/` with alias, ambient, reverb, scriptid,
  ducklist, musiclist, memory, assetcount and ambientgeometry references.

Missing required files fail the explicit external run. The report fingerprints
the corpus. These tests parse and compare stored fixtures; they do not rebuild
the original map or launch the game. The exploratory `lossy_prepass_shrinks_flac`
experiment stays outside acceptance runs and can be invoked explicitly with
`cargo test --release lossy_prepass_shrinks_flac -- --ignored --nocapture`.

## Regression status

The formerly failing unit probes now run as required tests. The remaining known
failure is B007, folder FileSpecs. `--known-bugs` verifies that its documented
limitation still reproduces; it does not count it as a feature acceptance pass.

| ID | Status | Required behavior |
| --- | --- | --- |
| B001 | Fixed | Zero WAV block alignment returns an error without panicking |
| B002 | Fixed | Valid padded odd-sized RIFF chunks parse |
| B003 | Fixed | Short and long envelopes use the same dB SPL units |
| B004 | Fixed | Tiny resampling inputs preserve frames without underflow |
| B005 | Fixed | Only fully identical alias rows are deduplicated |
| B006 | Fixed | Corrupt empty/populated caches rebuild assets and header identity |
| B007 | Open | Folder enumeration encodes two assets but references only the first |
| B008 | Fixed | The default encoder preserves every input sample bit |
| B009 | Fixed for handled failures | Failed conversion publishes no changed files; publication errors roll back |

The conversion fingerprint includes an algorithm revision and encoder effort.
Existing cached assets therefore reconvert once after these audio fixes, and
switching `flac_hifi` invalidates cached encodings. That feature now changes only
encoder effort (5 instead of 8); explicit lossy levels still apply their masks
and silence gates before encoding.

Publication is staged per zone and locale. It protects deployed outputs from
conversion failures, and attempts rollback on publication errors. If rollback
fails, its backups are retained and their location is reported. This is not a
crash-atomic multi-file filesystem transaction. Power loss, concurrent builds of
the same bank, and a later locale failing after an earlier locale published still
need a broader recovery protocol. Cache files may have advanced after a failed
build; the next successful run repairs deployment from them.

Other inspection findings, including bank bounds handling, long names and unused
CLI/configuration options, remain outside these fixes. Every new fix needs a
specific acceptance test before production changes. Do not weaken an assertion
to keep the audit green.

## Updating a reference

Never regenerate a baseline as the first response to a failing test. Inspect the
input/output difference and explain why it is required. Preserve unaffected
assets. Add the intended-behavior test before changing production code.

After review, the explicit recording command is:

```powershell
python scripts/audit.py --record-baseline --known-bugs
```

Review `tests/fixtures/baseline.json` in the diff, update its reference provenance
when appropriate, then run the audit again without `--record-baseline`. The
recording command changes the snapshot before later tests run; a failed command
does not approve that change. CI never records snapshots.

## Performance and cache migration

Warm builds compare full file contents before publishing. Unchanged deployed
banks and sidecars keep their timestamps. Missing files and same-size deployment
corruption are still repaired. Changed sources are read and hashed once, and the
same buffer is passed to conversion. FLAC receives at most 16,384 frames of i32
PCM at a time instead of allocating an i32 copy of the entire clip. Tests require
the resulting FLAC bitstream to match a single-block encoding exactly.

Measure two saved release executables with the same audio recipe:

```powershell
python scripts/benchmark.py --reference target/audit/pre-performance/ultrasound.exe --candidate target/release/ultrasound.exe --assets 128 --seconds 20 --repeats 5
```

Keep `libFLAC.dll` beside each executable. The benchmark alternates execution
order, records clean/warm/single-edit timings and sampled peak working set, and
requires equivalent output manifests for every comparison. These are generated
high-entropy inputs, not a map or an OS-cold disk benchmark. See
[the v1 progress report](v1-progress.md) for the measured results.

To check a one-time cache migration from an older binary, and optionally a switch
to an alternate encoder recipe and back:

```powershell
python scripts/audit.py --known-bugs --external --migration-reference target/audit/reference/ultrasound.exe --alternate-executable target/audit/hifi/ultrasound.exe
```

The saved references are local artifacts, not repository dependencies. Omit
`--alternate-executable` if no alternate build is available. The migration check
requires the older binary to have a different recipe; a same-version reference
fails instead of pretending to cover an upgrade. The final default outputs must
match the pinned baseline, and alternate outputs must match their clean build.

## Real-world acceptance

Use a disposable copy of a known-working project. Select one typical map and one
larger map with localized voice, streamed music and ambient data. First verify
that the reference binary builds and plays them correctly.

Copy `tests/fixtures/corpus.example.json` outside the repository and replace its
placeholders. List every input dependency, including templates, localized audio,
music/duck/reverb tables and map geometry where used. The capture tool cannot
discover the dependency closure. List deployed banks and sidecars for every
tested locale. Record the exact converter arguments; the tool does not run them.

After a successful reference build, capture it:

```powershell
python scripts/capture_corpus.py --root D:/GameCopy --manifest D:/audit/map.json --exe D:/audit/reference/ultrasound.exe --out D:/audit/reference.json
```

Build the same inputs with the candidate, then compare:

```powershell
python scripts/capture_corpus.py --root D:/GameCopy --manifest D:/audit/map.json --exe target/release/ultrasound.exe --out D:/audit/candidate.json --compare D:/audit/reference.json
```

The capture reads selected files, hashes them in chunks, and writes a report.
Every pattern must match. Changed input or output fingerprints produce a nonzero
exit and a list of changed files. Binary differences require semantic/audio
review; they do not automatically mean playback differs. Keep the reference
binary, DLL and outputs outside the directories the candidate overwrites.

For each accepted change, record map/corpus identity, binary hash, invocation,
build result, repeated timings, and playback observations. Check sound variation,
loop starts/stops, sustain/release transitions, localized voice, streamed music,
ambient transitions and ducking. Mark each untested item explicitly. No automated
report may label playback as passed on your behalf.

## Folder FileSpecs

B007 captures the current limitation, not the desired feature contract. Before
implementation, decide whether each file becomes a separate alias variation,
how ordering/probability works, whether enumeration recurses, how unsupported
files and empty folders behave, and how sustain/release files pair. Add fixtures
with audibly distinct clips. Acceptance requires every intended variation to be
reachable in-game, not merely present in the bank.

## Change review

Keep fixes and features separate from this harness branch. For each change,
record its reproducer or acceptance test, affected compatibility areas, audit
report, real-world checks performed, and remaining gaps. For performance work,
compare repeated measurements of the same inputs and report output size and
quality tradeoffs. Turn this procedure into a skill after several changes have
shown which checks are useful.
