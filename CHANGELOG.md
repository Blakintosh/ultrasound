### Version 0.2 (latest)

Features:

* Added FLAC file input support.
* Added OGG file input support.
* Added opt-in lossy compression: 
    * Specify `DefaultAliasCompression`: `none, low, medium, high, exteme` in your map/mod's SZC file (or within an ALIAS entry of the SZC) to use alias compression for the project/alias file. 
    * You can also use `CompressionLevel` column within an alias file to do this for an individual alias.
    * In testing as a % to baseline, the compression levels approximately mapped to: Low - 74%, Medium - 52%, High - 36%, Extreme - 24%. Your mileage may vary.
* Fixed a bug with how Ultrasound computed invalidation, which meant some alias changes were not picked up.
* Fixed a bug with how the loudness of sounds were calculated, which is used during processing.

### Version 0.1

* Initial release.