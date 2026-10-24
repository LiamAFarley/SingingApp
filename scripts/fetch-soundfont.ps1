# Downloads the default General MIDI SoundFont (GeneralUser GS by S. Christian Collins)
# into crates/trainer-app/resources/soundfonts so the installer bundles it.
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"
$dir = Join-Path $PSScriptRoot "..\crates\trainer-app\resources\soundfonts"
$dest = Join-Path $dir "GeneralUser-GS.sf2"
if (Test-Path $dest) {
    Write-Host "Already present: $dest"
    exit 0
}
$url = "https://raw.githubusercontent.com/mrbumpy409/GeneralUser-GS/main/GeneralUser-GS.sf2"
Write-Host "Downloading $url"
Invoke-WebRequest -Uri $url -OutFile $dest -UseBasicParsing
$size = (Get-Item $dest).Length
if ($size -lt 20MB) {
    Remove-Item $dest
    throw "Download looks wrong ($size bytes)"
}
Write-Host ("Saved {0} ({1:N1} MB)" -f $dest, ($size / 1MB))
