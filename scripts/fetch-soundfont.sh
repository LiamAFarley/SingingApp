#!/usr/bin/env sh
# Downloads the default General MIDI SoundFont (GeneralUser GS by S. Christian Collins)
# into crates/trainer-app/resources/soundfonts so the build bundles it.
set -eu
dir="$(dirname "$0")/../crates/trainer-app/resources/soundfonts"
dest="$dir/GeneralUser-GS.sf2"
if [ -f "$dest" ]; then
  echo "Already present: $dest"
  exit 0
fi
url="https://raw.githubusercontent.com/mrbumpy409/GeneralUser-GS/main/GeneralUser-GS.sf2"
echo "Downloading $url"
curl -fL --retry 3 -o "$dest.part" "$url"
size=$(wc -c < "$dest.part")
if [ "$size" -lt 20000000 ]; then
  rm -f "$dest.part"
  echo "Download looks wrong ($size bytes)" >&2
  exit 1
fi
mv "$dest.part" "$dest"
echo "Saved $dest ($size bytes)"
