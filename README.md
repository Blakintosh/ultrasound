# Ultrasound

Less noise, more sound. A part of the gscode ecosystem.

Ultrasound is a drop-in replacement for the Black Ops III Mod Tools' snd_convert. Built from the ground up in Rust, Ultrasound fixes long-standing snd_convert bugs and delivers massive speed improvements over its predecessor. 

### Additional features

* Supports converting non-48k WAV files, including with correct resampling.
* Support for direct FLAC format.
* Support for OGG format.
* Optional support for lossy compression levels that reduce `.sabl` / `.sabs` bank sizes.

## Installation

Navigate to the [Releases](https://github.com/Blakintosh/ultrasound/releases) tab to download the latest release. From here, the `README.md` inside the release will guide you through the installation process.

## Issues

The current version of Ultrasound is an early release and may not yet achieve complete feature parity with the original snd_convert. If you encounter any issues where sound files do not parse with Ultrasound (but do parse with snd_convert), please file an issue report.

## Performance

All of the following performance metrics has been produced locally on a map level with ~1GB of total sound assets.

#### Cold: full rebuild of SABL & SABS files.

```
Baseline (snd_convert): ~52.1s
Ultrasound: ~6.5s (~8x faster)
```

#### Warm: no asset changes.

```
Baseline (snd_convert): ~19.9s
Ultrasound: ~1.1s (~18x faster)
```

Your mileage may vary.

## Disclaimer

Ultrasound is an independent community implementation of a sound processing pipeline compatible with the Black Ops III Mod Tools. It is in no way affiliated with Call of Duty, Treyarch, or Activision.

## Licence

MIT License

Copyright (c) 2026 Blakintosh

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
