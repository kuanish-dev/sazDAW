//! VST3 strings: C `char8` arrays and UTF-16 (`String128`) both ways, plus `'static` strings for contracts.

use std::ffi::c_char;

use daw_contracts::L10n;

/// Makes a string `'static` by leaking it on purpose: the `ParamInfo`/`EffectFactory`/`InstrumentFactory` contract requires
/// `&'static str`. A plugin source (`PluginSource`) leaks its strings once and survives repeated scans;
/// only catalog kinds leak again — tens of bytes per plugin per manual scan, which is acceptable.
pub fn leak(text: String) -> &'static str {
    Box::leak(text.into_boxed_str())
}

/// A plugin's own text as a contract label: the plugin speaks one language, so it is the same in every language.
/// Leaks like [`leak`].
pub fn leak_label(text: String) -> L10n {
    L10n::same(leak(text))
}

/// Option names for `ParamInfo::choices`, leaked like [`leak`]; untranslated, see [`leak_label`].
pub fn leak_list(texts: &[String]) -> &'static [L10n] {
    Box::leak(texts.iter().map(|text| leak_label(text.clone())).collect::<Vec<_>>().into_boxed_slice())
}

/// Reads a C string from a fixed-size array (up to the first zero or the end of the array).
pub fn from_c_chars(src: &[c_char]) -> String {
    let bytes: Vec<u8> = src.iter().take_while(|&&c| c != 0).map(|&c| c as u8).collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Reads a UTF-16 string from a fixed-size array (up to the first zero).
pub fn from_utf16(src: &[u16]) -> String {
    let units: Vec<u16> = src.iter().copied().take_while(|&c| c != 0).collect();
    String::from_utf16_lossy(&units)
}

/// Writes a string into a UTF-16 array with a trailing zero; the excess is cut off.
pub fn to_utf16(src: &str, dst: &mut [u16]) {
    let room = dst.len().saturating_sub(1);
    let mut len = 0;
    for (unit, slot) in src.encode_utf16().zip(dst.iter_mut().take(room)) {
        *slot = unit;
        len += 1;
    }
    if let Some(end) = dst.get_mut(len) {
        *end = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c_string_stops_at_nul() {
        let mut raw = [0 as c_char; 8];
        for (slot, byte) in raw.iter_mut().zip(b"Fx|Dist") {
            *slot = *byte as c_char;
        }
        assert_eq!(from_c_chars(&raw), "Fx|Dist");
        raw[2] = 0;
        assert_eq!(from_c_chars(&raw), "Fx");
    }

    #[test]
    fn utf16_roundtrip_with_cyrillic_and_truncation() {
        let mut buffer = [7u16; 6];
        to_utf16("Привет мир", &mut buffer);
        assert_eq!(from_utf16(&buffer), "Приве");
        let mut wide = [7u16; 16];
        to_utf16("DAW", &mut wide);
        assert_eq!(from_utf16(&wide), "DAW");
    }
}
