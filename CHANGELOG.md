# Changelog

## 1.1.0

* Shipped sounds for stock-carried aliases. When the Linker's stock carry (Dragon) has copied sound aliases that only a
  shipped bank defines into your map, it lists the sounds they play in `zone_source\<zone>.stocksound.json`. Ultrasound
  copies those sounds, as they are (no decoding, no encoding), out of the game's `zone\snd` banks into your map's
  `.sabl` / `.sabs`, and prints `stock sounds [all]: N from M shipped bank(s), K not found`. A sound your own sources
  also name stays yours. Without the file nothing changes and nothing is printed.
* `snd_convert --capabilities` prints one line of JSON naming the tool, its version and the stock sound request
  contract it understands, so the Linker can check before it asks for stock sounds. It does nothing else.
* Ultrasound lists itself in the Mod Tools components registry (`ultrasound.json`), so other tools can find it and its
  version. Set `ULTRASOUND_NO_REGISTRY=1` to leave the registry alone.

## 1.0.0

Your first build after updating reconverts every sound once, because the conversion recipe changed. Builds
after that are incremental again.

### Changes you'll notice

* Much faster builds when little has changed. On local NTFS drives Ultrasound uses the drive's change journal to
  tell that a source file is unchanged without reading it again. Set `ULTRASOUND_FULL_HASH` to check every file
  instead.
* A failed build leaves your deployed banks and `.sz` files as they were. A bank or `.sz` file whose contents
  haven't changed is no longer rewritten, so its timestamp stays put.
* Ultrasound updates itself in the background, at most once a day after a successful build, and installs with
  `ultrasound-setup.exe`, like the other gscode tools.
* `snd_convert --version` prints the version.
* The README and this changelog named the compression setting `DefaultAliasCompression`. It is
  `DefaultAudioCompression`; the old name never did anything, and a build now warns when it sees it.

### Fixes

* Aliases that differ only in playback or conversion settings are no longer merged into one.
* Short clips' loudness envelopes use the same units as long clips'.
* A damaged cached bank is rebuilt instead of failing the build.
* WAV files: zero block alignment is an error instead of a crash; padded odd-sized chunks, very short clips that
  need resampling, and data that ends partway through a frame all convert.
* FileSpecs match `.WAV`, `.Flac` and other capitalisations, and folder FileSpecs work in a game folder whose
  path has spaces in it.
* A malformed FLAC file is reported instead of being read out of bounds.
* Errors name the file, alias or map they're about.

### Known limits

* A FileSpec that names a folder encodes every file in it, but the alias plays only the first.

## 0.3.1

* Fixed blank lines in alias files and stale file targets.

## 0.3.0

* Added emitting the `.sz` files used by the Linker, and `ambientgeometry.json`.

## 0.2.0

* Added FLAC file input support.
* Added OGG file input support.
* Added opt-in lossy compression:
    * Specify `DefaultAudioCompression`: `none, low, medium, high, extreme` in your map/mod's SZC file (or within
      an ALIAS entry of the SZC) to use alias compression for the project/alias file.
    * You can also use `CompressionLevel` column within an alias file to do this for an individual alias.
    * In testing as a % to baseline, the compression levels approximately mapped to: Low - 74%, Medium - 52%,
      High - 36%, Extreme - 24%. Your mileage may vary.
* Fixed a bug with how Ultrasound computed invalidation, which meant some alias changes were not picked up.
* Fixed a bug with how the loudness of sounds were calculated, which is used during processing.

## 0.1.0

* Initial release.
