//! The course's legs and the commands that build them, as offsets from the course origin (the bot's spawn feet
//! block). The lane runs along +X: walled 3 wide up to x = 47, then 11 wide to x = 95 for the parkour legs,
//! whose obstacles span z -5..=3 and leave a flat corridor at z 4..=5 for profiles that walk around them.

pub struct Leg {
    pub name: &'static str,
    /// Goal offset from the course origin.
    pub goal: [i32; 3],
    /// Blocks to set, as (offset, block) with `{stairs_e}`/`{ladder_w}` placeholders.
    pub build: &'static [Fill],
}

impl Leg {
    /// In the parkour section: built with it.
    pub fn far(&self) -> bool {
        self.goal[0] >= FAR_X
    }
}

/// Pits keep the bottom floor layer (y -4): below it is the void.
pub const LEGS: &[Leg] = &[
    Leg { name: "flat", goal: [3, 0, 0], build: &[] },
    Leg { name: "step", goal: [8, 0, 0], build: &[([5, 0, -1], [5, 0, 1], "stone")] },
    Leg {
        name: "stair2",
        goal: [14, 0, 0],
        build: &[([10, 0, -1], [10, 0, 1], "stone"), ([11, 0, -1], [12, 1, 1], "stone")],
    },
    Leg {
        name: "slabs",
        goal: [20, 0, 0],
        build: &[
            ([15, 0, -1], [15, 0, 1], "{slab}"),
            ([16, 0, -1], [17, 0, 1], "stone"),
            ([17, 1, -1], [17, 1, 1], "{slab}"),
        ],
    },
    Leg {
        name: "stairs",
        goal: [26, 0, 0],
        build: &[
            ([21, 0, -1], [21, 0, 1], "{stairs_e}"),
            ([22, 0, -1], [23, 0, 1], "stone"),
            ([22, 1, -1], [22, 1, 1], "{stairs_e}"),
            ([23, 1, -1], [23, 1, 1], "stone"),
        ],
    },
    Leg { name: "gap", goal: [32, 0, 0], build: &[([28, -3, -1], [29, -1, 1], "air")] },
    Leg {
        name: "ladder",
        goal: [38, 0, 0],
        build: &[
            ([34, 0, -1], [34, 3, 1], "stone"),
            ([35, 0, -1], [35, 1, 1], "stone"),
            ([33, 0, 0], [33, 3, 0], "{ladder_w}"),
        ],
    },
    Leg {
        name: "pool",
        goal: [47, 0, 0],
        build: &[([40, -2, -1], [44, -1, 1], "water")],
    },
    Leg { name: "float", goal: [42, -1, 0], build: &[] },
    Leg { name: "exit", goal: [47, 0, 0], build: &[] },
    Leg { name: "gap3", goal: [55, 0, 0], build: &[([50, -3, -5], [52, -1, 3], "air")] },
    // A 4-gap needs a 2-block run-up from rest (docs/pathfind-plan.md step 4); `Human` refuses it.
    Leg { name: "gap4", goal: [64, 0, 0], build: &[([58, -3, -5], [61, -1, 3], "air")] },
    Leg {
        name: "up1",
        goal: [70, 1, 0],
        build: &[([67, -3, -5], [68, -1, 3], "air"), ([69, 0, -5], [71, 0, 3], "stone")],
    },
    Leg {
        name: "down2",
        goal: [75, -1, 0],
        build: &[([72, -3, -5], [73, -1, 3], "air"), ([74, -1, -5], [77, -1, 3], "air")],
    },
    // A one-block pillar at (78, 0) and an island at (81, -2): only a diagonal leap joins them.
    Leg {
        name: "diag",
        goal: [86, 0, 0],
        build: &[
            ([78, -3, -5], [83, -1, 3], "air"),
            ([78, -3, 0], [78, -1, 0], "stone"),
            ([81, -3, -2], [81, -1, -2], "stone"),
        ],
    },
    Leg {
        name: "stones",
        goal: [95, 0, 0],
        build: &[
            ([88, -3, -5], [94, -1, 3], "air"),
            ([89, -3, 0], [89, -1, 0], "stone"),
            ([91, -3, 1], [91, -1, 1], "stone"),
            ([93, -3, 0], [93, -1, 0], "stone"),
        ],
    },
    Leg { name: "sprint40", goal: [55, 0, 4], build: &[] },
];

/// The leg whose leap `--lift` disturbs in mid-air, the x span (offsets) to do it in, and the command: a
/// second of levitation no planned leap expects.
pub const LIFT_LEG: &str = "gap3";
pub const LIFT_X: (f32, f32) = (49.5, 51.0);
pub const LIFT: &str = "/effect @s levitation 1 1 true";

pub type Fill = ([i32; 3], [i32; 3], &'static str);

/// Where the wide parkour section starts. It is built when the bot gets there: a fill touching a chunk the
/// server has not loaded fails as a whole, and x 96 can be out of reach from the origin.
pub const FAR_X: i32 = 48;

/// The empty walled lane of one section (`far`: x ≥ `FAR_X`), before any leg's obstacles.
pub fn lane(far: bool) -> Vec<Fill> {
    if !far {
        return vec![
            ([-2, 0, -6], [47, 8, 6], "air"),
            ([-2, -4, -6], [47, -1, 6], "stone"),
            ([-2, 0, -2], [47, 5, -2], "glass"),
            ([-2, 0, 2], [47, 5, 2], "glass"),
            ([-2, 0, -2], [-2, 5, 2], "glass"),
        ];
    }
    vec![
        ([48, 0, -6], [96, 8, 6], "air"),
        ([48, -4, -6], [96, -1, 6], "stone"),
        ([48, 0, -6], [96, 5, -6], "glass"),
        ([48, 0, 6], [96, 5, 6], "glass"),
        ([48, 0, -6], [48, 5, -3], "glass"),
        ([48, 0, 3], [48, 5, 6], "glass"),
        ([96, 0, -6], [96, 5, 6], "glass"),
    ]
}

/// The block for `java` (Geyser) or Bedrock syntax.
pub fn resolve(block: &str, java: bool) -> String {
    match (block, java) {
        ("{slab}", _) => "smooth_stone_slab".into(),
        ("{stairs_e}", true) => "oak_stairs[facing=east]".into(),
        ("{stairs_e}", false) => "oak_stairs [\"weirdo_direction\"=0]".into(),
        ("{ladder_w}", true) => "ladder[facing=west]".into(),
        ("{ladder_w}", false) => "ladder [\"facing_direction\"=4]".into(),
        (b, _) => b.into(),
    }
}
