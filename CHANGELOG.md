### Unreleased

* Added background auto-update. After a successful build, at most once a day, Ultrasound checks for a newer release, verifies its SHA-256 digest and swaps in the new `snd_convert.exe` and `libFLAC.dll` without holding up the build. The next build logs the update, or that a new version is available if it couldn't be installed.

### Version 0.3 (latest)

* Added emitting .sz files used by Linker, as well as ambientgeometry.json files.

### Version 0.2

* Added FLAC file input support.
* Added OGG file input support.
* Added opt-in lossy compression: 
    * Specify `DefaultAudioCompression`: `none, low, medium, high, extreme` in your map/mod's SZC file (or within an ALIAS entry of the SZC) to use alias compression for the project/alias file. 
    * You can also use `CompressionLevel` column within an alias file to do this for an individual alias.
    * In testing as a % to baseline, the compression levels approximately mapped to: Low - 74%, Medium - 52%, High - 36%, Extreme - 24%. Your mileage may vary.
* Fixed a bug with how Ultrasound computed invalidation, which meant some alias changes were not picked up.
* Fixed a bug with how the loudness of sounds were calculated, which is used during processing.

### Version 0.1

* Initial release.