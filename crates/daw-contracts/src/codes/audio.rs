//! Audio devices and streams, input monitoring, audio recording. Codes `AU-KNN`.

crate::codes! {
    ENGINE_QUEUE_FULL = "AU-280", "The audio engine command queue is full";
    NO_OUTPUT_DEVICE = "AU-040", "No audio output device available";
    OUTPUT_DEVICE_NOT_FOUND = "AU-041", "Output device \"{name}\" not found (disconnected?)";
    INPUT_DEVICE_NOT_FOUND = "AU-042", "Input device \"{name}\" not found (disconnected?)";
    NO_INPUT_DEVICE = "AU-043", "No audio input device available. Check the microphone permission in system settings";
    BUFFER_SIZE_ZERO = "AU-140", "The buffer size must be greater than zero";
    BUFFER_SIZE_OUT_OF_RANGE = "AU-240", "Buffer size {frames} is outside the device range {min}…{max} frames";
    CAPTURE_ALREADY_RUNNING = "AU-340", "Audio capture is already running";
    INPUT_CHANGE_WHILE_RECORDING = "AU-341", "Audio is being recorded: stop recording before changing the input";
    OUTPUT_NOT_STARTED = "AU-342", "Audio output is not started";
    OUTPUT_FAILED = "AU-440", "The audio output failed: {details}";
    INPUT_FAILED = "AU-441", "The audio input failed: {details}. Check the microphone permission in system settings";
    HOST_NOT_RESPONDING = "AU-442", "The audio thread is not responding; restart the application";
    STREAM_OVERLOAD = "AU-443", "Audio overload: the sound was interrupted";
    DEVICE_UNAVAILABLE = "AU-444", "The audio device is unavailable (disconnected?)";
    STREAM_CLOSED_BY_SYSTEM = "AU-445", "The system closed the audio stream";
    STREAM_FAILED = "AU-446", "The audio stream failed";
    UNSUPPORTED_SAMPLE_FORMAT = "AU-540", "Sample format {format} is not supported (only 32-bit float)";
    INPUT_WITHOUT_QUEUE = "AU-940", "The input is open but has no capture queue";
    NOT_AUDIO_TRACK = "AU-141", "Track \"{name}\" cannot record audio: it is an instrument track, an audio track is needed";
    TAKE_BEFORE_CLIP = "AU-241", "The take audio ends before the clip starts";
    ARMING_WHILE_RECORDING = "AU-343", "Audio is being recorded: stop recording before changing armed tracks or inputs";
    RECORDING_ALREADY_RUNNING = "AU-344", "Audio recording is already running";
    NO_ARMED_TRACKS = "AU-345", "No armed audio tracks: press ● on an audio track to arm it";
    PROJECT_NOT_SAVED = "AU-346", "Save the project first: recorded audio goes into its folder";
    NOT_RECORDING = "AU-347", "Audio recording is not running";
    EMPTY_TAKE = "AU-348", "Empty take: no frames came from the input";
    TAKE_SHORTER_THAN_LATENCY = "AU-349", "The recording is shorter than the input latency: nothing to save";
    RECORD_THREAD_FAILED = "AU-447", "Could not start the recording thread: {details}";
    TAKES_FAILED = "AU-448", "Some tracks were not recorded: {details}";
    TAKE_CLIP_NOT_CREATED = "AU-449", "The audio was saved to Samples, but the clip was not created: {details}";
    RECORD_THREAD_CRASHED = "AU-941", "The recording thread crashed; the audio is lost";
}
