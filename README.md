# Ultrasound

Less noise, more sound. A part of the gscode ecosystem.

Ultrasound replaces the Black Ops III Mod Tools' sound converter, `snd_convert.exe`. It builds the same sound
banks from the same alias, ambient, reverb, duck and music files, fixes long-standing bugs in the original, and
is many times faster, especially when little has changed since your last build.

It installs as `snd_convert.exe` in your `sound` folder, so the Linker, Blackbird and any shortcut you already
have use it without any change to your project.

## What it does

- Converts WAV at any sample rate (resampled to 48 kHz), and FLAC and OGG files directly.
- Rebuilds only the sounds whose source or settings changed. On local NTFS drives it can tell a file is
  unchanged without reading it again, so a build where nothing changed takes about two seconds, even for a map
  with ten thousand sounds.
- Builds maps Treyarch's converter rejects, such as aliases whose names or file paths are longer than it allows.
- Writes the `.sz` files the Linker reads, and `ambientgeometry.json`.
- Leaves your deployed banks untouched when a build fails, and rewrites a bank only when its contents change.
- Optionally shrinks `.sabl` / `.sabs` banks with lossy compression (see [Settings](#settings)).

## Requirements

- Windows 10 or 11, 64-bit
- Call of Duty: Black Ops III and the Black Ops III Mod Tools, installed through Steam

## Installing

Download `ultrasound-setup.exe` from the [Releases](https://github.com/Blakintosh/ultrasound/releases) page and
choose Install. It puts `snd_convert.exe` and `libFLAC.dll` into `<BO3>\sound`, and keeps Treyarch's copies in a
hidden `sound\.gscode-backups` folder.

To install by hand, unzip `ultrasound-X.Y.Z-win-x64.zip` into your `Call of Duty Black Ops III` folder; its
`sound` folder goes over the one already there. Keep a copy of Treyarch's `snd_convert.exe` first if you want
to go back to it.

If a Steam "Verify integrity of game files" puts Treyarch's converter back, open the setup and choose
Reinstall.

## Updates

Ultrasound keeps itself up to date. At most once a day, after a successful sound build, it checks for a new
release in the background, verifies the download against the release's SHA-256 checksum, and swaps in the new
`snd_convert.exe` and `libFLAC.dll`. Your build never waits for it. The next build's log says
`Ultrasound X.Y.Z is installed`, or `Couldn't install Ultrasound X.Y.Z` with the reason and a link to the
installer. Only a copy in a Black Ops III `sound` folder updates itself. Its update state lives in
`%LOCALAPPDATA%\gscode\ultrasound`, not in the game folder.

## Uninstalling

Open the setup and choose Uninstall. It puts Treyarch's converter back and removes everything it added.

To do it by hand, with the Mod Tools closed, move `snd_convert.exe` and `libFLAC.dll` from
`<BO3>\sound\.gscode-backups\` up into `<BO3>\sound\`, replacing the files there, then delete
`<BO3>\sound\.gscode-backups\`. The folder is hidden; in File Explorer choose View > Show > Hidden items. Or
verify the Mod Tools in Steam, which puts Treyarch's files back.

To remove Ultrasound's cached data as well, delete `%LOCALAPPDATA%\gscode\ultrasound`, and the `*.usn.json`
files beside your map's cached banks in `sound\zone\CachedBanks`.

## Settings

By default each sample keeps all but its lowest bit, as with Treyarch's converter; the difference is inaudible.
To trade more quality for smaller banks:

- Set `DefaultAudioCompression` to `None`, `Low`, `Medium`, `High` or `Extreme` in your map or mod's `.szc`
  file, for the whole zone, or on one of its `Sources` entries, for that alias file.
- Set the `CompressionLevel` column in an alias file for a single alias. It wins over both.

On one large map, Low, Medium, High and Extreme gave banks 78%, 57%, 42% and 26% of their default size. Your
mileage may vary; listen to the result.

Set the environment variable `ULTRASOUND_FULL_HASH` to any value to make Ultrasound read and check every source
file on every build, rather than trusting the NTFS journal. Builds are slower; use it if you suspect files have
changed without Windows knowing, such as after disk corruption.

## Performance

Measured on `zm_reactorfour`, a map with 7,898 sounds and 768 MB of banks, on one machine with 16 logical CPUs:

| Build | Treyarch's `snd_convert` | Ultrasound 1.0 |
| --- | ---: | ---: |
| Full rebuild | 54–73 s | 9–15 s |
| Nothing changed | 19–35 s | 2–3 s |

Treyarch's converter also failed the first attempt at a full rebuild in three of four tries, with "Missing source
checksum", and succeeded when run again. Your mileage may vary.

## Issues

Ultrasound aims to build everything Treyarch's converter builds. If a sound builds with Treyarch's
`snd_convert` but not with Ultrasound, or sounds different in game, please
[file an issue](https://github.com/Blakintosh/ultrasound/issues) with the alias and source file.

Known limit: a FileSpec that names a folder rather than a file encodes every file in it, but the alias plays
only the first.

## Building from source

You need Rust with the MSVC toolchain. The repository supplies libFLAC.

```
cargo build --release
```

`target\release` then holds `ultrasound.exe` and `libFLAC.dll`; it installs as `sound\snd_convert.exe`.
`release.ps1` builds the release zip and `ultrasound-setup.exe`. A build named `ultrasound.exe` never updates
itself. For the test suite and the compatibility audit, see [Testing changes](docs/testing.md).

## Disclaimer

Ultrasound is an independent community implementation of a sound processing pipeline compatible with the Black
Ops III Mod Tools. It is in no way affiliated with Call of Duty, Treyarch, or Activision.

## Licence

MIT. See [LICENSE](LICENSE).
