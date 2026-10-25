/** Answer vocabularies for the custom-level forms, in the ids the exercises use. */

export interface Opt {
  id: string;
  label: string;
}

export const SIMPLE_INTERVALS: Opt[] = [
  { id: "P1", label: "Unison" },
  { id: "m2", label: "m2" },
  { id: "M2", label: "M2" },
  { id: "m3", label: "m3" },
  { id: "M3", label: "M3" },
  { id: "P4", label: "P4" },
  { id: "TT", label: "Tritone" },
  { id: "P5", label: "P5" },
  { id: "m6", label: "m6" },
  { id: "M6", label: "M6" },
  { id: "m7", label: "m7" },
  { id: "M7", label: "M7" },
  { id: "P8", label: "Octave" },
];

export const COMPOUND_INTERVALS: Opt[] = [
  { id: "m9", label: "m9" },
  { id: "M9", label: "M9" },
  { id: "m10", label: "m10" },
  { id: "M10", label: "M10" },
  { id: "P11", label: "P11" },
  { id: "A11", label: "A11" },
  { id: "P12", label: "P12" },
  { id: "m13", label: "m13" },
  { id: "M13", label: "M13" },
  { id: "m14", label: "m14" },
  { id: "M14", label: "M14" },
  { id: "P15", label: "2 octaves" },
];

export const TRIADS: Opt[] = [
  { id: "major", label: "Major" },
  { id: "minor", label: "Minor" },
  { id: "diminished", label: "Diminished" },
  { id: "augmented", label: "Augmented" },
  { id: "major_b5", label: "Major ♭5" },
  { id: "sus2", label: "Sus 2" },
  { id: "sus4", label: "Sus 4" },
];

export const SEVENTHS: Opt[] = [
  { id: "dominant7", label: "7th" },
  { id: "major7", label: "Major 7th" },
  { id: "minor7", label: "Minor 7th" },
  { id: "minor_major7", label: "Minor-major 7th" },
  { id: "half_diminished7", label: "Half-diminished" },
  { id: "diminished7", label: "Diminished 7th" },
  { id: "major6", label: "Major 6th" },
  { id: "minor6", label: "Minor 6th" },
  { id: "dominant7_b5", label: "7th ♭5" },
  { id: "dominant7_s5", label: "7th ♯5" },
  { id: "major7_b5", label: "Major 7th ♭5" },
  { id: "major7_s5", label: "Major 7th ♯5" },
  { id: "minor_major7_b5", label: "Minor-major 7th ♭5" },
  { id: "dominant7_sus4", label: "7th sus4" },
  { id: "dominant7_sus2", label: "7th sus2" },
  { id: "major7_sus4", label: "Major 7th sus4" },
  { id: "major7_sus2", label: "Major 7th sus2" },
];

export const SCALES: Opt[] = [
  { id: "major", label: "Major" },
  { id: "dorian", label: "Dorian" },
  { id: "phrygian", label: "Phrygian" },
  { id: "lydian", label: "Lydian" },
  { id: "mixolydian", label: "Mixolydian" },
  { id: "natural_minor", label: "Natural minor" },
  { id: "locrian", label: "Locrian" },
  { id: "harmonic_minor", label: "Harmonic minor" },
  { id: "melodic_minor", label: "Melodic minor" },
  { id: "whole_tone", label: "Whole tone" },
  { id: "major_pentatonic", label: "Major pentatonic" },
  { id: "minor_pentatonic", label: "Minor pentatonic" },
  { id: "blues", label: "Blues" },
  { id: "chromatic", label: "Chromatic" },
  { id: "dorian_b2", label: "Phrygian ♯6" },
  { id: "lydian_augmented", label: "Lydian ♯5" },
  { id: "lydian_dominant", label: "Lydian ♭7" },
  { id: "mixolydian_b6", label: "Mixolydian ♭13" },
  { id: "locrian_s2", label: "Locrian ♯2" },
  { id: "altered", label: "Altered" },
  { id: "diminished_whole_half", label: "Diminished (whole–half)" },
  { id: "diminished_half_whole", label: "Diminished (half–whole)" },
  { id: "locrian_s6", label: "Locrian ♯6" },
  { id: "ionian_s5", label: "Ionian ♯5" },
  { id: "dorian_s4", label: "Dorian ♯4" },
  { id: "phrygian_dominant", label: "Phrygian dominant" },
  { id: "lydian_s2", label: "Lydian ♯2" },
  { id: "ultra_locrian", label: "Altered ♭♭7" },
];

export const KEYS: Opt[] = [
  { id: "C", label: "C" },
  { id: "C#", label: "C♯/D♭" },
  { id: "D", label: "D" },
  { id: "D#", label: "E♭" },
  { id: "E", label: "E" },
  { id: "F", label: "F" },
  { id: "F#", label: "F♯/G♭" },
  { id: "G", label: "G" },
  { id: "G#", label: "A♭" },
  { id: "A", label: "A" },
  { id: "A#", label: "B♭" },
  { id: "B", label: "B" },
];

/** Degrees numbered against the major scale, with the upper tonic as 8. */
export const DIATONIC_DEGREES = ["1", "2", "3", "4", "5", "6", "7", "8"];
export const MINOR_DEGREES = ["1", "2", "b3", "4", "5", "b6", "b7", "8"];
export const CHROMATIC_DEGREES = ["1", "b2", "2", "b3", "3", "4", "#4", "5", "b6", "6", "b7", "7", "8"];

/** "b3" → "♭3". */
export const degreeLabel = (d: string) => d.replace("b", "♭").replace("#", "♯");
