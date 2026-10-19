// Plucked strings (backend `daw-strings`): their kinds and where a new track's step input starts.

export const GUITAR_KIND = "guitar";
export const ELECTRIC_GUITAR_KIND = "electric_guitar";
export const UKULELE_KIND = "ukulele";
export const BASS_GUITAR_KIND = "bass_guitar";

/** Step input pitch of a new string track: a note in the middle of the instrument's range (E3, C4, A1). */
export const STRING_PITCHES: Record<string, number> = {
  [GUITAR_KIND]: 52, [ELECTRIC_GUITAR_KIND]: 52, [UKULELE_KIND]: 60, [BASS_GUITAR_KIND]: 33,
};
