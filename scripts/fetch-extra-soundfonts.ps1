#requires -Version 5.1
<#
.SYNOPSIS
  Downloads the extra free SoundFont banks used for timbre diversity and installs them in the
  app's data folder.

.DESCRIPTION
  The app loads every .sf2 in <data>\soundfonts at startup. This script fetches the curated set
  documented in docs/SOUNDFONTS.md, re-keys the octave-low drawbar organ with tools/rekey_sf2.py,
  verifies each file's SHA-256, and copies it into place. Existing files are left alone unless
  -Force is given.

  Run from the repository (installs into SingingApp\data\soundfonts):
      scripts/fetch-extra-soundfonts.ps1

  For an installed app, point it at the app's data folder:
      scripts/fetch-extra-soundfonts.ps1 -Dest "$env:LOCALAPPDATA\Programs\Singing Trainer\data\soundfonts"

  Requires 7-Zip (7z) and tar, both already used by the project's other fetch scripts.
#>
[CmdletBinding()]
param(
    [string]$Dest = (Join-Path (Split-Path -Parent $PSScriptRoot) 'data\soundfonts'),
    [switch]$Force
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

# Final file name, download URL, whether it is packed, and where it came from.
$items = @(
    @{ Name = 'UprightPiano.sf2'
       Url  = 'https://freepats.zenvoid.org/Piano/UprightPianoKW/UprightPianoKW-small-SF2-20190703.7z'
       Packed = $true;  Source = 'FreePats Upright Piano KW'; License = 'CC0 1.0' }
    @{ Name = 'TenorSax.sf2'
       Url  = 'https://freepats.zenvoid.org/Reed/TenorSaxophone/TenorSaxophone-small-SF2-20200717.tar.bz2'
       Packed = $true;  Source = 'FreePats Tenor Saxophone (VCSL samples)'; License = 'CC0 1.0' }
    @{ Name = 'ClassicalGuitar.sf2'
       Url  = 'https://freepats.zenvoid.org/Guitar/SpanishClassicalGuitar/SpanishClassicalGuitar-SF2-20190618.7z'
       Packed = $true;  Source = 'FreePats Spanish classical guitar'; License = 'CC0 1.0' }
    @{ Name = 'ConcertHarp.sf2'
       Url  = 'https://freepats.zenvoid.org/OrchestralStrings/ConcertHarp/ConcertHarp-small-SF2-20200702.tar.xz'
       Packed = $true;  Source = 'FreePats Concert Harp (VCSL samples)'; License = 'CC0 1.0' }
    @{ Name = 'DrawbarOrgan.sf2'
       Url  = 'https://freepats.zenvoid.org/Organ/DrawbarOrganEmulation/DrawbarOrganEmulation-SF2-20190712.tar.xz'
       Packed = $true;  Rekey = -12; Source = 'FreePats Drawbar Organ (re-keyed +12)'; License = 'CC0 1.0' }
    @{ Name = 'Violin.sf2'
       Url  = 'https://raw.githubusercontent.com/open-soundfonts/Saint_James_Orchestra_soundfonts/master/SJO_Violin_Arco.sf2'
       Packed = $false; Source = 'Saint James Orchestra, Violin Arco'; License = 'GPL-3.0' }
    @{ Name = 'Viola.sf2'
       Url  = 'https://raw.githubusercontent.com/open-soundfonts/Saint_James_Orchestra_soundfonts/master/SJO_Viola.sf2'
       Packed = $false; Source = 'Saint James Orchestra, Viola'; License = 'GPL-3.0' }
    @{ Name = 'Cello.sf2'
       Url  = 'https://raw.githubusercontent.com/open-soundfonts/Saint_James_Orchestra_soundfonts/master/SJO_Cello.sf2'
       Packed = $false; Source = 'Saint James Orchestra, Cello'; License = 'GPL-3.0' }
    @{ Name = 'Flute.sf2'
       Url  = 'https://raw.githubusercontent.com/open-soundfonts/Saint_James_Orchestra_soundfonts/master/SJO_Flute.sf2'
       Packed = $false; Source = 'Saint James Orchestra, Flute'; License = 'GPL-3.0' }
    @{ Name = 'FrenchHorn.sf2'
       Url  = 'https://raw.githubusercontent.com/open-soundfonts/Saint_James_Orchestra_soundfonts/master/SJO_French_Horn.sf2'
       Packed = $false; Source = 'Saint James Orchestra, French Horn'; License = 'GPL-3.0' }
    @{ Name = 'Trombone.sf2'
       Url  = 'https://raw.githubusercontent.com/open-soundfonts/Saint_James_Orchestra_soundfonts/master/SJO_Trombone.sf2'
       Packed = $false; Source = 'Saint James Orchestra, Trombone'; License = 'GPL-3.0' }
    @{ Name = 'SoloVoices.sf2'
       Url  = 'https://www.zanderjaz.com/soundfonts/choirs/PhilsVoicesolo.sf2'
       Packed = $false; Source = "Phil's Voice Solo (Zanderjaz archive)"; License = 'not stated - personal use only' }
    @{ Name = 'OperaFemale.sf2'
       Url  = 'https://www.zanderjaz.com/soundfonts/choirs/OperaSingerFemale1.sf2'
       Packed = $false; Source = 'Opera Singer Female 1 (Zanderjaz archive)'; License = 'not stated - personal use only' }
    @{ Name = 'SoftVoice.sf2'
       Url  = 'https://www.zanderjaz.com/soundfonts/choirs/SoftVoice.sf2'
       Packed = $false; Source = 'Soft Voice (Zanderjaz archive)'; License = 'not stated - personal use only' }
    @{ Name = 'MaleOohs.sf2'
       Url  = "https://www.zanderjaz.com/soundfonts/choirs/Dave%27sMaleOohs.sf2"
       Packed = $false; Source = "Dave's Male Oohs (Zanderjaz archive)"; License = 'not stated - personal use only' }
    @{ Name = 'HeavenChoir.sf2'
       Url  = 'https://www.zanderjaz.com/soundfonts/choirs/HeavenChoir.SF2'
       Packed = $false; Source = 'Heaven Choir (Zanderjaz archive)'; License = 'not stated - personal use only' }
)

# SHA-256 of the exact files verified with the app's renderer. A mismatch means the upstream
# file changed; inspect it before trusting it in a pitch test.
$hashes = @{
    'UprightPiano.sf2'   = 'CF2A98EB38A32C4954B4B6E2CAAE4112D62DD8E892ECEEFDD7942B0E7D01AC2F'
    'TenorSax.sf2'       = 'D7B9A7EF796484FA25AF7ECBDEB2B23B228568D404007A4AB3259768D3481A5D'
    'ClassicalGuitar.sf2'= '0D4C08EA8C1C8924B3829084B0C8778F6D6B906F331E44FDE51BC159F76B03B9'
    'ConcertHarp.sf2'    = '1796607B3C7DC9CE0D98473C33068EA4C20AB3DFDA38A44F157EE10BFED6CCB4'
    'DrawbarOrgan.sf2'   = '6F5643BEF5B32BD2DE4CE52E21B3DB6B6759D097826E804CA54779B55D703BB6'
    'Violin.sf2'         = '38A9AF78352ED8F716C93E2E14DC8367B33501D718E165220CD97E3308F961A1'
    'Viola.sf2'          = '93C3654650072BE48F6239F4FF9BD025E06B9C3A19C27C2C9AB4D8CFB824432E'
    'Cello.sf2'          = '87C6A0397EF120A2A860BE6315F4766FC1238D7B2C16F394D727675A88EC2C7A'
    'Flute.sf2'          = '7A313674B466FDA29ED6C6E7F7A9E8F01B3830882CB811CE9AD3A176951BE6FF'
    'FrenchHorn.sf2'     = '9B5E354BF95CF214BF60AFEA99971309C6F7F7DC6C3D83CCE93910913AD024E9'
    'Trombone.sf2'       = '554F922B456AD51C84FF3F6AD9DBC9ACA968D6CA7D8FA7A68DCCF1E39F575F31'
    'SoloVoices.sf2'     = '2187622C136772DF6EF1BF6E55AB461F07A3B6B0C12E569A43C8816A5ECCC674'
    'OperaFemale.sf2'    = 'FAC333E9135CFD77C51633587CCF525A774197288BEBD97CC5EF785726253651'
    'SoftVoice.sf2'      = '4B8311253F31452609C21AC092F816CC8BDE29DB25103DCAFAAEF672FBA0BA54'
    'MaleOohs.sf2'       = 'FC6E417B96E6BC7D824814CF10397551A9714B0E5C45479A93E6E9C7A95DAD10'
    'HeavenChoir.sf2'    = '23F9739B7D73F380279838CBAB4FC997CE700D12FBC511C2C51B92B2562B1F52'
}

function Get-FirstSf2([string]$dir) {
    $sf2 = Get-ChildItem -Path $dir -Recurse -Filter '*.sf2' | Select-Object -First 1
    if (-not $sf2) { throw "No .sf2 found in $dir" }
    return $sf2
}

if (-not (Get-Command 7z -ErrorAction SilentlyContinue)) {
    throw '7-Zip (7z) is required to unpack the FreePats archives.'
}

New-Item -ItemType Directory -Force -Path $Dest | Out-Null
$tmp = Join-Path ([IO.Path]::GetTempPath()) ('singing-trainer-soundfonts-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $tmp | Out-Null

try {
    foreach ($item in $items) {
        $target = Join-Path $Dest $item.Name
        $expected = $hashes[$item.Name]
        if (Test-Path $target) {
            $actual = (Get-FileHash $target -Algorithm SHA256).Hash
            if ($actual -eq $expected) {
                Write-Host ("have    {0}" -f $item.Name)
                continue
            }
            if (-not $Force) {
                Write-Warning ("{0} exists but does not match the verified hash; use -Force to replace it." -f $item.Name)
                continue
            }
        }

        $download = Join-Path $tmp ([guid]::NewGuid().ToString('N') + '_' + [IO.Path]::GetFileName(([uri]$item.Url).AbsolutePath))
        Write-Host ("fetch   {0}  ({1})" -f $item.Name, $item.License)
        Invoke-WebRequest -Uri $item.Url -OutFile $download -UserAgent 'Mozilla/5.0' -TimeoutSec 180

        if ($item.Packed) {
            $unpack = Join-Path $tmp ([guid]::NewGuid().ToString('N'))
            New-Item -ItemType Directory -Force -Path $unpack | Out-Null
            if ($download -match '\.(7z|zip)$') {
                & 7z x -y "-o$unpack" $download | Out-Null
            } else {
                & tar -xf $download -C $unpack
            }
            $source = (Get-FirstSf2 $unpack).FullName
        } else {
            $source = $download
        }

        if ($item.Rekey) {
            if (-not (Get-Command python -ErrorAction SilentlyContinue)) {
                throw 'Python is required to re-key DrawbarOrgan.sf2 (tools/rekey_sf2.py).'
            }
            $tool = Join-Path (Split-Path -Parent $PSScriptRoot) 'tools\rekey_sf2.py'
            $rekeyed = Join-Path $tmp ([guid]::NewGuid().ToString('N') + '.sf2')
            & python $tool $source $rekeyed --root-shift $item.Rekey
            if ($LASTEXITCODE -ne 0) { throw "re-keying $($item.Name) failed" }
            $source = $rekeyed
        }

        $actual = (Get-FileHash $source -Algorithm SHA256).Hash
        if ($actual -ne $expected) {
            throw ("Hash mismatch for {0}.`n  expected {1}`n  got      {2}`nThe upstream file may have changed." -f $item.Name, $expected, $actual)
        }
        Copy-Item -Force $source $target
        Write-Host ("install {0}  {1:N1} MB" -f $item.Name, ((Get-Item $target).Length / 1MB))
    }
} finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}

$installed = Get-ChildItem $Dest -Filter '*.sf2'
$total = ($installed | Measure-Object Length -Sum).Sum / 1MB
Write-Host ''
Write-Host ("{0} banks in {1} ({2:N1} MB). Restart the app so it reloads them." -f $installed.Count, $Dest, $total)
Write-Host 'Timbre ids and measured pitch ranges are in docs/SOUNDFONTS.md.'
