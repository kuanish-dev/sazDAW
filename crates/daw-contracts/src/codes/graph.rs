//! Sound graph and mixer: nodes, edges, effects, parameters, sends, channel utility controls, the virtual stage. Codes
//! `GR-KNN`.

crate::codes! {
    NODE_NOT_FOUND = "GR-001", "Node {id} not found";
    UNKNOWN_EFFECT = "GR-002", "Unknown effect \"{kind}\"";
    EDGE_TO_ITSELF = "GR-101", "A node cannot be connected to itself";
    MASTER_HAS_NO_OUTPUT = "GR-102", "The master has no outputs";
    SOURCE_HAS_NO_INPUT = "GR-103", "A sound source accepts no inputs";
    SIDECHAIN_NEEDS_EFFECT = "GR-104", "Only an effect has a sidechain input";
    EDGE_MAKES_LOOP = "GR-105", "This connection would make a loop: the sound would return to the node it came from";
    PRE_FADER_NEEDS_STRIP = "GR-106", "Only tracks and buses have pre-fader sends";
    POSITION_NOT_A_NUMBER = "GR-110", "Node position {value} is not a finite number";
    LEVEL_NOT_A_NUMBER = "GR-111", "Connection level {value} is not a finite number";
    VOLUME_NOT_A_NUMBER = "GR-112", "Volume {value} is not a finite number";
    PAN_NOT_A_NUMBER = "GR-113", "Pan {value} is not a finite number";
    PARAM_NOT_A_NUMBER = "GR-114", "Effect parameter value {value} is not a finite number";
    NO_VOLUME = "GR-115", "Node {id} has no volume: only sources, buses and the master have one";
    NO_PAN = "GR-116", "Node {id} has no pan: only sources and buses have one";
    NO_MUTE = "GR-117", "Node {id} cannot be muted: only sources and buses can";
    NO_SOLO = "GR-118", "Node {id} cannot be soloed: only sources and buses can";
    NOT_REMOVABLE = "GR-119", "Node {id} cannot be deleted: only effects and buses can (a source goes with its track)";
    NO_SUCH_EDGE = "GR-120", "There is no connection from node {from} to node {to}";
    UNKNOWN_EFFECT_PARAM = "GR-121", "Effect \"{kind}\" has no parameter \"{param}\"";
    NO_SIDECHAIN_INPUT = "GR-122", "Effect \"{kind}\" has no sidechain input";
    NO_STAGE_PLACE = "GR-123", "Node {id} cannot stand on the stage: only tracks and buses can";
    NOT_ON_STAGE = "GR-124", "Node {id} does not stand on the stage";
    STAGE_NOT_A_NUMBER = "GR-125", "Stage value {value} is not a finite number";
    NO_UTILITY = "GR-126", "Node {id} has no polarity, width, mono or delay: only sources and buses have them";
    WIDTH_NOT_A_NUMBER = "GR-127", "Width {value} is not a finite number";
    DELAY_NOT_A_NUMBER = "GR-128", "Channel delay {value} is not a finite number";
    NODE_NOT_EFFECT = "GR-160", "Node {id} is not an effect";
    TOO_MANY_NODES = "GR-210", "The graph already has the maximum of {max} nodes";
    TOO_MANY_EDGES = "GR-211", "The graph already has the maximum of {max} connections";
    TOO_MANY_ON_STAGE = "GR-212", "The stage already holds the maximum of {max} channels";
    EFFECT_NOT_CREATED = "GR-410", "Effect \"{name}\" ({kind}) was not created: {details}";
    EFFECT_NOT_REMOVED = "GR-411", "The effect of node {id} was not removed from the engine: {details}";
}
