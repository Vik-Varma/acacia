//! Music tracks the Bedrock definitions name by number and Java's files by title. Both name
//! lists were read off the files on 2026-10-11 (BDS 1.26 `sound_definitions.json`, Java 26.3's
//! asset index); which number is which title follows Java's own renaming and was not checked
//! by ear.

/// Bedrock's name and Java's, both under `sounds/music/`.
const TRACKS: [(&str, &str); 28] = [
    ("game/calm1", "game/minecraft"),
    ("game/calm2", "game/clark"),
    ("game/calm3", "game/sweden"),
    ("game/hal1", "game/subwoofer_lullaby"),
    ("game/hal2", "game/living_mice"),
    ("game/hal3", "game/haggstrom"),
    ("game/hal4", "game/danny"),
    ("game/nuance1", "game/key"),
    ("game/nuance2", "game/oxygene"),
    ("game/piano1", "game/dry_hands"),
    ("game/piano2", "game/wet_hands"),
    ("game/piano3", "game/mice_on_venus"),
    ("game/creative/creative1", "game/creative/biome_fest"),
    ("game/creative/creative2", "game/creative/blind_spots"),
    ("game/creative/creative3", "game/creative/haunt_muskie"),
    ("game/creative/creative4", "game/creative/aria_math"),
    ("game/creative/creative5", "game/creative/dreiton"),
    ("game/creative/creative6", "game/creative/taswell"),
    ("game/nether/rubedo", "game/nether/nether_wastes/rubedo"),
    ("game/nether/nether1", "game/nether/concrete_halls"),
    ("game/nether/nether2", "game/nether/dead_voxel"),
    ("game/nether/nether3", "game/nether/warmth"),
    ("game/nether/nether4", "game/nether/ballad_of_the_cats"),
    ("game/end/end", "game/end/the_end"),
    ("menu/menu1", "menu/mutation"),
    ("menu/menu2", "menu/moog_city_2"),
    ("menu/menu3", "menu/beginning_2"),
    ("menu/menu4", "menu/floating_trees"),
];
const MUSIC: &str = "sounds/music/";

/// The paths a sample may lie at: the definition's own, then Java's name for the track.
pub(crate) fn candidates(path: &str) -> Vec<String> {
    let java = path.strip_prefix(MUSIC).and_then(|track| TRACKS.iter().find(|(bedrock, _)| *bedrock == track)).map(|(_, java)| format!("{MUSIC}{java}"));
    std::iter::once(path.to_owned()).chain(java).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_numbered_track_is_also_looked_for_by_its_title() {
        assert_eq!(candidates("sounds/music/game/calm3"), ["sounds/music/game/calm3", "sounds/music/game/sweden"]);
        assert_eq!(candidates("sounds/music/game/shores"), ["sounds/music/game/shores"]);
        assert_eq!(candidates("sounds/dig/stone1"), ["sounds/dig/stone1"]);
    }
}
