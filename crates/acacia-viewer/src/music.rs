//! Background music: a track of the dimension's set, then a pause before the next, as the game
//! does it. Events and pauses are the Bedrock pack's `music_definitions.json` (`game`,
//! `creative`, `hell`, `end`); biome sets are not used.

/// A dimension's music event and the shortest and longest pause after a track, in seconds.
const SETS: [(&str, [u32; 2]); 4] =
    [("music.game", [600, 1200]), ("music.game.creative", [60, 180]), ("music.game.nether_wastes", [60, 180]), ("music.game.end", [60, 180])];
/// Java's first track comes 100 ticks after joining.
const FIRST: u32 = 100;
/// A track's length is not known here: the pause is counted from this long after its start.
const TRACK: u32 = 240 * 20;

pub struct Music {
    /// Ticks until the next track.
    wait: u32,
    roll: u32,
}

impl Default for Music {
    fn default() -> Self {
        Music { wait: FIRST, roll: 0x9E37_79B9 }
    }
}

impl Music {
    /// Called once a tick: the event to play now, if a track is due. `dimension` is 0 for the
    /// overworld, 1 the Nether, 2 the End.
    pub fn tick(&mut self, dimension: i32, creative: bool) -> Option<&'static str> {
        self.wait = self.wait.saturating_sub(1);
        if self.wait > 0 {
            return None;
        }
        let (event, [least, most]) = SETS[match (dimension, creative) {
            (1, _) => 2,
            (2, _) => 3,
            (_, true) => 1,
            _ => 0,
        }];
        // Xorshift: the pause only has to vary.
        self.roll ^= self.roll << 13;
        self.roll ^= self.roll >> 17;
        self.roll ^= self.roll << 5;
        self.wait = TRACK + 20 * (least + self.roll % (most - least + 1));
        Some(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_track_starts_soon_after_joining_and_the_next_after_its_pause() {
        let mut music = Music::default();
        let first = (1..=FIRST).find_map(|tick| music.tick(0, false).map(|event| (tick, event)));
        assert_eq!(first, Some((FIRST, "music.game")));
        let pause = (1..).find(|_| music.tick(1, false).is_some()).unwrap();
        assert!((TRACK + 600 * 20..=TRACK + 1200 * 20).contains(&pause), "{pause}");
        assert_eq!(music.tick(2, true), None);
        assert_eq!((1..).find_map(|_| music.tick(2, true)), Some("music.game.end"));
    }
}
