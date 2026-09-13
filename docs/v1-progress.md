# V1 correctness and performance progress

The baseline checkpoint is `96026a6` on `codex/v1-regression-baseline`.
Correctness fixes are checkpointed in `0c4ff32`. Continued work is on
`codex/v1-correctness-and-performance`. These changes have not been released.

## Correctness

Eight of the nine verified issues are fixed and covered by required tests:

- WAV dimension validation, padded RIFF chunks and tiny resampling inputs.
- Consistent envelope units for short clips, including silence and full scale.
- Preservation of alias rows that differ in playback or conversion settings.
- Recovery of invalid cached bank headers, including empty banks and bank identity.
- Lossless default encoding; explicit lossy levels retain their existing processing.
- Staged publication, with no changed deployment after conversion failure and
  rollback on handled publication errors.

The recipe fingerprint now includes the conversion revision and encoder effort.
The audit checks upgrades from the original converter's cache, an alternate
encoder recipe, and a return to the default recipe. Existing assets reconvert
once after the correction; ordinary unchanged builds remain incremental.

The synthetic snapshot update was reviewed separately. Only source-recipe
checksums, encoded payloads and their memory totals changed. Existing aliases and
long-clip metadata matched the original reference. The original snapshot remains
in the baseline checkpoint.

Folder FileSpecs (B007) remain open. Their variation and sustain/release behavior
needs an explicit contract and an in-game acceptance test.

## Performance

The comparison uses the corrected converter before performance work as its
reference. This keeps the audio recipe identical on both sides.

Three changes passed without any further snapshot changes:

1. Skip publication of unchanged bank/sidecar contents, preserving timestamps.
2. Pass the hashed source buffer directly into conversion, removing its second
   read and checksum pass.
3. Feed FLAC bounded i32 PCM blocks instead of retaining a full extra PCM copy.

Local measurements used 128 generated high-entropy stereo WAVs of 20 seconds
each, plus the small baseline fixture. Deployed banks totaled 492,899,072 bytes.
Each executable ran five repetitions per scenario, with execution order
alternating. Source files were freshly generated and filesystem-warm. The host
reported Windows build 26200 and 16 logical CPUs.

| Scenario | Reference median | Candidate median | Reduction |
| --- | ---: | ---: | ---: |
| Clean build | 1.1243 s | 0.9842 s | 12.5% |
| Unchanged build | 0.2477 s | 0.1945 s | 21.5% |
| Single source edit | 0.3693 s | 0.3250 s | 12.0% |

Maximum observed clean-build peak working set fell from 663.7 MB to 559.0 MB.
Single-edit peak working set rose from 68.7 MB to 72.3 MB; hashing other sources
can now overlap with conversion. Warm-build peak working set was essentially
unchanged. These are observed process working sets, not guaranteed memory caps.

Every compared output manifest matched, including encoded payload hashes. The
local report at `target/audit/performance-final.json` contains individual samples,
memory readings and binary hashes. The measured reference SHA-256 is
`613494c4c06179a5a5c14cc29a32dedd7fc0b42db600024f7dec7c296579cf33`;
the candidate is
`665368432f38c48859693811ad601016d3c92ba98e4a4c0460ff76ccfee39a1b`.

These results support keeping the changes. They do not predict gains on a
particular map, slower disk, or different asset mix.

## NTFS source validation

The `codex/v1-journal-validation` branch adds checksum reuse for unchanged local
NTFS sources, with full hashing on uncertainty. It retains the existing checksum
and audio formats. The reference for this comparison is `c7abdea`, after the
earlier performance changes.

The fast path requires matching volume/journal/file identities, file USN, size,
recipe and the bank's stored checksum, plus a retained validation checkpoint.
Read handles exclude writers and writable mappings. Inputs that cannot be opened
that way use the previous full-content validation behavior. Missing/malformed
validation records and unavailable tracking also use full hashing. Checksums
remain available with `ULTRASOUND_FULL_HASH` set, including for integrity scans
that must detect corruption outside normal filesystem writes.

Validation passed:

- 58 Rust tests in each feature configuration: 48 existing tests and 10 new
  journal tests. All 24 external fixture tests and three Python capture tests
  also passed.
- 36 dedicated parity scenarios against `c7abdea`, then the same 36 against the
  candidate with full hashing forced: 144 CLI invocations. Real cache hits are
  required; a fallback cannot pass as a working optimization.
- The full compatibility audit, including recipe migration, publication failure
  recovery, deployment corruption repair, settings changes and B007 reproduction.
- 84 timed CLI builds across large and small generated workloads, with matching
  independently parsed output manifests and exact encoded payload hashes.

The large workload is the same 128 generated stereo WAVs of 20 seconds plus
baseline fixtures, producing 492,899,072 deployed bytes. Seven repetitions per
binary/scenario ran in alternating order, separately from compilation and tests.
Every warm candidate build reused 134 source checksums; every single-edit build
reused 133. Median wall times:

| Scenario | Full hashing | Journal validation | Change |
| --- | ---: | ---: | ---: |
| Clean build | 1.2578 s | 1.2988 s | 3.3% slower |
| Unchanged build | 0.2489 s | 0.1364 s | 45.2% less time |
| Single source edit | 0.3843 s | 0.3328 s | 13.4% less time |

Warm-build maximum observed process working set fell from 68.7 MB to 8.7 MB;
single-edit working set fell from 73.3 MB to 17.2 MB. Cold memory was similar.
Timing noise was substantial: warm candidate samples ranged from 0.1276 s to
2.0155 s, with six of seven below 0.15 s. All samples remain in the report;
none were discarded. These medians are local observations, not promised gains.

A second workload with eight one-second assets (1,584,896 deployed bytes) showed
no meaningful speed difference: about 75 ms cold, 41 ms warm and 57 ms for an edit.
The benefit depends on how much source data a build would otherwise hash.

Local evidence is in `target/audit/journal-audit-final.json`,
`target/audit/journal-parity.json`, `target/audit/journal-self-parity.json`,
`target/audit/journal-performance-final.json` and
`target/audit/journal-performance-small.json`. The reference executable SHA-256 is
`665368432f38c48859693811ad601016d3c92ba98e4a4c0460ff76ccfee39a1b`;
the candidate is
`9e196322adda353946557993371b8e9b000fcc2250d2d9b9752a8d2c48961688`.

Journal resets and retention gaps were simulated, without changing the host's
journal. Unsupported filesystems and network shares retain full hashing but
were not exercised on separate physical storage. This does not establish
whole-project consistency during concurrent editing or replace in-game tests.
The CI workflow includes the dedicated parity suite; hosted execution remains
unverified.

## Remaining acceptance work

The local audit covers portable tests in both feature configurations, the 24
external fixture tests, recovery and migration scenarios, and the remaining
folder reproducer. It does not launch the linker or game. Map builds, playback
checks and hosted CI execution remain unverified.

Publication is per zone/locale and is not crash-atomic across multiple files.
Concurrent writers and recovery from process termination still need a separate
protocol. Cached-payload corruption and hostile bank metadata need further
loader validation; the current recovery tests exercise invalid headers.

Further performance candidates include reuse across zones, bounded retention of
all converted payloads, and bank compaction. Measure them against this checkpoint
before changing the pipeline again.
