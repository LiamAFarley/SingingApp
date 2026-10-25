"""Write the speech-folder file list.

The app reads these from the app's data\\speech folder (WAV, FLAC or MP3).
Required files turn a speaking feature off when they are missing. Optional files
are alternate spellings and short prompts. Keep this list in step with
crates/trainer-core/src/speech.rs.
"""

from pathlib import Path

REQUIRED = []
for n in range(10):
    REQUIRED.append(f"digits/{n}.wav")
for c in range(ord("A"), ord("Z") + 1):
    REQUIRED.append(f"letters/{chr(c)}.wav")
for name in ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"]:
    REQUIRED.append(f"notes/{name}.wav")
for word in ["Red", "Blue", "Yellow", "Orange", "Brown", "Grey", "Green", "Magenta", "Teal", "Gold", "Indigo", "Pink"]:
    REQUIRED.append(f"labels/taneda_colors/{word}.wav")
for name in ["Sarah", "David", "Francine", "Jimmy", "Karen", "Leo", "Nina", "Oscar", "Paula", "Robert", "Rita", "Simon"]:
    REQUIRED.append(f"labels/gervain_names/{name}.wav")

OPTIONAL = [f"notes/{name}.wav" for name in ["Db", "Eb", "Gb", "Ab", "Bb"]]
OPTIONAL += ["prompts/sing.wav", "prompts/now.wav"]
OPTIONAL += [
    f"labels/eguchi_chords/{word}.wav"
    for word in [
        "Red", "Yellow", "Blue", "Black", "Green", "Orange", "Purple", "Pink", "Brown",
        "Gray", "Tan", "Light_green", "Light_purple", "Sky_blue",
    ]
]


def main() -> None:
    lines = ["# required"] + REQUIRED + ["", "# optional"] + OPTIONAL + [""]
    text = "\n".join(lines)
    out = Path(__file__).with_name("speech_files.txt")
    out.write_text(text, encoding="utf-8")
    print(text, end="")
    print(f"# wrote {out}", flush=True)


if __name__ == "__main__":
    main()
