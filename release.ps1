<#
  Builds an Ultrasound release, as gscode-installer's docs/release-contract.md describes:

    artifacts\ultrasound-X.Y.Z-win-x64.zip   the bundle: sound\snd_convert.exe and sound\libFLAC.dll
    artifacts\ultrasound-setup.exe           the installer, with that bundle inside

  .\release.ps1                      build both (refuses uncommitted changes; -AllowDirty for a test build)
  .\release.ps1 -Publish -NotesFile notes.md
                                     also create the GitHub release vX.Y.Z from them; fails if the tag exists

  Needs Rust, the .NET 10 SDK (for the installer) and, to publish, the GitHub CLI.
#>
param(
    [string] $Installer = 'J:\Github\gscode-installer',
    [switch] $AllowDirty,
    [switch] $Publish,
    [string] $NotesFile
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.IO.Compression.FileSystem
$root = $PSScriptRoot
$id = 'ultrasound'

if (-not $AllowDirty -and (git -C $root status --porcelain --untracked-files=no)) {
    throw 'Uncommitted changes. Commit them, or pass -AllowDirty for a test build.'
}
$version = (Select-String -Path "$root\Cargo.toml" -Pattern '^version\s*=\s*"([^"]+)"').Matches[0].Groups[1].Value
$tag = "v$version"
if ($Publish) {
    if (-not $NotesFile -or -not (Test-Path $NotesFile)) { throw '-Publish needs -NotesFile <release notes .md>' }
    if (git -C $root ls-remote --tags origin "refs/tags/$tag") { throw "$tag already exists on GitHub" }
}

# The cargo binary is ultrasound.exe; it ships as snd_convert.exe, beside the libFLAC.dll it links against.
cargo build --release --manifest-path "$root\Cargo.toml"
if ($LASTEXITCODE) { throw "cargo build failed ($LASTEXITCODE)" }

$name = "$id-$version-win-x64"
$artifacts = Join-Path $root 'artifacts'
$stage = Join-Path $artifacts $name
$zip = "$stage.zip"
Remove-Item $stage, $zip -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory "$stage\sound" -Force | Out-Null
Copy-Item "$root\target\release\ultrasound.exe" "$stage\sound\snd_convert.exe"
Copy-Item "$root\lib\libFLAC.dll" "$stage\sound\libFLAC.dll"
[IO.Compression.ZipFile]::CreateFromDirectory($stage, $zip)

$setup = & "$Installer\scripts\build-setup.ps1" -Bundle $zip
foreach ($f in $zip, $setup) { '{0}  {1:N1} MB' -f $f, ((Get-Item $f).Length / 1MB) }

if ($Publish) {
    gh release create $tag $zip $setup --target (git -C $root rev-parse HEAD) --title "Ultrasound $version" --notes-file $NotesFile
    if ($LASTEXITCODE) { throw "gh release create failed ($LASTEXITCODE)" }
}
