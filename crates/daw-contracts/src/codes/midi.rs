//! MIDI devices, MIDI files and MIDI mappings. Codes `MD-KNN`.

crate::codes! {
    DEVICE_NOT_FOUND = "MD-070", "MIDI device \"{name}\" not found";
    NO_MAPPING = "MD-110", "The parameter has no MIDI controller assigned";
    BAD_FILE = "MD-170", "Not a valid MIDI file: {details}";
    FILE_HAS_NO_NOTES = "MD-171", "The MIDI file has no notes";
    BAD_FILE_TEMPO = "MD-172", "The MIDI file has an invalid tempo or time signature: {details}";
    UNAVAILABLE = "MD-470", "MIDI is unavailable: {details}";
    DEVICE_NOT_CONNECTED = "MD-471", "Could not connect to MIDI device \"{name}\": {details}";
    FILE_NOT_READ = "MD-472", "Could not read the MIDI file {path}: {details}";
    FILE_NOT_WRITTEN = "MD-473", "Could not write the MIDI file {path}: {details}";
    SMPTE_UNSUPPORTED = "MD-570", "MIDI files with SMPTE time code are not supported";
}
