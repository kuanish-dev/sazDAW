//! Project files: open, save, autosave, recent list, new project. Codes `PJ-KNN`.

crate::codes! {
    AUTOSAVE_NOT_FOUND = "PJ-070", "There is no autosave to restore";
    ALREADY_EXISTS = "PJ-110", "Project {path} already exists";
    FILE_DAMAGED = "PJ-170", "The project file is damaged: {details}";
    SAMPLE_NOT_A_FILE = "PJ-171", "{path} is not a file";
    SAMPLE_NOT_LOADED = "PJ-410", "Sample \"{name}\" ({path}) could not be loaded: {details}";
    FOLDER_NOT_CREATED = "PJ-470", "Could not create the folder {path}: {details}";
    FILE_NOT_WRITTEN = "PJ-471", "Could not write the project file {path}: {details}";
    FILE_NOT_READ = "PJ-472", "Could not read the project file {path}: {details}";
    SAMPLE_NOT_COPIED = "PJ-473", "Could not copy {path} into the project folder: {details}";
    NEWER_FORMAT = "PJ-570", "The project was made by a newer version of the app (format {format})";
}
