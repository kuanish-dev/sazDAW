//! Pure choice rules: which device to take by name and whether a buffer size is allowed.
//! Kept apart from cpal so they are tested without a sound card.

use daw_contracts::ContractError;
use daw_contracts::codes::audio;

/// Position of the device with exactly this name (system names are compared as they are).
pub(crate) fn index_of_name(names: &[Option<String>], wanted: &str) -> Option<usize> {
    names.iter().position(|name| name.as_deref() == Some(wanted))
}

/// Buffer size for the stream: `None` lets the system decide; a number must be within the device range.
/// If the device reports no range, the number is tried as is: cpal itself refuses, and that error reaches the user.
pub(crate) fn buffer_request(requested: Option<u32>, range: Option<(u32, u32)>) -> Result<Option<u32>, ContractError> {
    let Some(frames) = requested else { return Ok(None) };
    if frames == 0 {
        return Err(audio::BUFFER_SIZE_ZERO.err());
    }
    match range {
        Some((min, max)) if frames < min || frames > max => {
            Err(audio::BUFFER_SIZE_OUT_OF_RANGE.with("frames", frames).with("min", min).with("max", max))
        }
        _ => Ok(Some(frames)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_is_found_by_exact_name_only() {
        let names = vec![Some("Speakers".to_string()), None, Some("USB".to_string())];
        assert_eq!(index_of_name(&names, "USB"), Some(2));
        assert_eq!(index_of_name(&names, "usb"), None);
        assert_eq!(index_of_name(&names, "Missing"), None);
        assert_eq!(index_of_name(&[], "USB"), None);
    }

    #[test]
    fn no_request_means_system_default() {
        assert_eq!(buffer_request(None, Some((16, 4096))), Ok(None));
        assert_eq!(buffer_request(None, None), Ok(None));
    }

    #[test]
    fn buffer_inside_the_range_is_accepted_including_edges() {
        assert_eq!(buffer_request(Some(256), Some((16, 4096))), Ok(Some(256)));
        assert_eq!(buffer_request(Some(16), Some((16, 4096))), Ok(Some(16)));
        assert_eq!(buffer_request(Some(4096), Some((16, 4096))), Ok(Some(4096)));
    }

    #[test]
    fn buffer_outside_the_range_is_refused_with_the_range_in_the_params() {
        let low = buffer_request(Some(8), Some((16, 4096))).unwrap_err();
        assert!(low.is(audio::BUFFER_SIZE_OUT_OF_RANGE), "{low}");
        assert!(low.message().contains("16") && low.message().contains("4096"), "{low}");
        assert!(buffer_request(Some(8192), Some((16, 4096))).is_err());
        assert!(matches!(buffer_request(Some(0), None), Err(e) if e.is(audio::BUFFER_SIZE_ZERO)));
    }

    #[test]
    fn unknown_range_lets_the_device_decide() {
        assert_eq!(buffer_request(Some(300), None), Ok(Some(300)));
    }
}
