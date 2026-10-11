# Viewer: what it does, against the native client

State on 2026-10-09. `acacia-viewer` is a playable client in two looks (Bedrock, Java). How each
piece works is in the crates' READMEs (acacia-render, acacia-ui); this is the checklist.

"Live" means seen in a testbox screenshot (`tools/testbox-shot.sh`) against a BDS; "tests" means
unit or pack tests only.

## Done

| Area | What | Checked |
|---|---|---|
| World | Both looks' blocks, biomes' colours, lighting, liquids, block-entity models | live; Java blocks by `lookbake java-check` |
| World | Vines, glow lichen, sculk veins and resin clumps on the faces they cling to; levers | live, both looks. The Bedrock look's lever is Java's model (the pack has no shape for it): at rest and thrown on a floor and a wall look the same in both |
| World | Piglin heads (Java's model: the pack ships no geometry for it) on floors and walls; tripwire as a string over the floor | live, both looks. In the Bedrock look tripwire is drawn both ways across its block whichever way it runs |
| World | Nether portal blocks: a translucent sheet across the frame, with the pack's swirl | live, both looks (a frame lit with fire) |
| Sky | Sun, moon, stars, sky disc, sunrise glow, fancy and fast clouds (Options) | live (fast clouds: Java look) |
| Weather | Rain, storm sky and clouds, lightning bolts and flash | live |
| Weather | Snow by biome, none in dry biomes | tests (the test worlds have no cold biome) |
| In fluids | Water, lava and powder snow fog per look; Java's underwater overlay | live (powder snow: not seen) |
| Particles | Server particles; torches, fires, furnaces, campfires, drips; block chips | live, both looks |
| Entities | Pack models and animations, skins, name tags, dropped items | live |
| Entities | Held items in both hands, worn armour, hurt flash | live |
| Entities | Leather dyed worn, held and in slots: undyed brown, or the stack's own dye | undyed: live; a stack's dye: tests (no command dyes armour) |
| Entities | Enchantment glint on held items, worn armour and slot icons | live in the Java look (armour: faint on netherite, not told apart on diamond); Bedrock look: faint, its rules assumed; dropped items and other entities' hands: tests |
| Entities | Death topple | tests |
| Entities | Round shadows on the ground (radius from the hitbox) | live |
| Entities | Translucent shells (slime, sulfur cube) | live (slime) |
| Entities | The shield as its model in either hand; first person draws both hands | live, both looks: an armour stand's main hand, first person at rest; Java look only: the player's two hands from behind. Raised while sneaking: live (Java look), in first person and from the front (Java's `shield_blocking` placement; animations read `query.blocking` and the held items' names) |
| Entities | The charged creeper's aura | live, both looks, beside a plain creeper |
| Entities | Boat paddles and a dry hull | live, both looks (an empty boat on a pool) |
| Entities | Paddles rowing by the boat's paddle times | live in the Java look (one frame, from behind); the cadence is assumed |
| Entities | Fishing lines (hook to its owner's rod) and leads (mob to its holder or fence knot), sagging as Java's | fishing line: live, first person (Java look); a lead from a cow to another player's hand: live (Java look); other players' fishing lines: tests; other holders' hand anchors are approximate |
| Entities | Players crouch while sneaking (the server's flag; the own player's input) and raise a shield with it, the arm in Java's blocking pose | live for another player (Java look, the `props` bot). BDS did not set the blocking flag on it: a sneaking shield holder counts as blocking |
| Entities | Capes: the skin's cape image on the game's cape sheet, hung at Java's rest angle, leaning with a sneaking wearer and lifting with its pace | live on a standing, sneaking player from behind and the side (Java look, `props`); lifted behind a walking one (`props … pace`; any direction lifts it, no sideways sway); a persona skin's cape image would arrive the same way but none of the pool skins has one |
| Entities | A shield's banner: the cloth and patterns from `shield_patterns`, on the model and the slot icon | live (Java look): a shield the `props … shield` bot crafted a loomed banner onto, raised in its hand; the slot icon looked at from both packs' textures (`cargo run -p acacia-render --example shield_icon`) |
| Maps | A filled map held in first person shows the picture the server sends, on the map sheet: in both hands lying flat until the holder looks down (Java's placement), or beside the view when the other hand is full | live (Java look): lying flat, upright and one-handed. Markers are arrowheads in the marker's colour (neither pack ships the game's marker images): the holder's seen live, at its place and pointing its way. The arms holding it are drawn (seen live under the upright map and beside the one-handed one). Not drawn: marker labels; other hands and slots show the item's icon |
| Item frames | Frames on walls, floors and ceilings with their item at half size, or a map's picture over the block's face (asked of the server once) | live (Java look): a sword, an apple, a block and a map on a wall, a sword on the floor and under a ceiling, filled by the `frames` bot. A turned item turns clockwise (seen live); which way a lying item points is unchecked |
| First person | The player's bare arm while the main hand is empty, placed and swung as Java's | live (Java look): at rest, and swinging while it mines (`ACACIA_ATTACK=1`) |
| Signs | Text on standing, wall and hanging signs, dye and glow | live, both looks |
| Banners | Standing and wall banners with their dye and patterns, the ominous banner; the flag sways | live, both looks (20 of 42 patterns seen; composition and sway by tests) |
| Moving | Walking, sprinting, sneaking, jumping, swimming (acacia-physics, server-checked) | bot traces |
| Moving | Creative flight (Space twice) | live, 0 corrections on a strict BDS |
| Moving | Dimension travel: by command, joining in another dimension | live, Nether and End, with their fog and sky |
| Moving | Dimension travel through Nether and End portals | bot drill `dimensions` (0 corrections); not seen in the viewer |
| Moving | Riding: boat, horse, pig, minecart; seat camera, sitting pose | live (pig pose: not seen) |
| Camera | View bobbing, sprint and flight FOV, hurt roll, death tilt, F5 views | live (death tilt: tests) |
| Acting | Mining with cracks and predicted break, placing, using, attacking | live |
| Acting | Eating, drinking and the bow in first person | bow pose live, not compared with the game; eating and drinking: the pose by test (bottom centre through the use), the rise and the emptied bottle seen live |
| HUD | Hotbar, hearts, food, armour, air, XP, item name, effects, boss bars, titles, action bar | live |
| HUD | Chat with translations (rawtext and keys), Tab list, sidebar, F3 | live |
| Screens | Inventory with the player's figure, chests, recipe book and crafting by click | live |
| Screens | Crafting by hand: the 2×2 grid and the crafting table's 3×3; a click takes the result onto the cursor, Shift into the inventory | table live; live |
| Screens | Anvil with its name box and level cost, smithing table, stonecutter, enchanting table | live (the cost: both looks, a rename). A cost above the player's levels is red, and 40 or more reads "Too Expensive!": the red line by its pixels in a testbox shot (a levelless rename, image not looked at), "Too Expensive!" by the bot's tests only; the result stays empty in both cases |
| Screens | A scroller on the stonecutter's and the trader's lists when they are longer than their window; the wheel moves them a row | live (Java look): the stonecutter's moved by the wheel, the trader's seen at rest |
| Screens | Enchanting options: the server's words in the galactic alphabet (Java's `ascii_sga.png`, also in the Bedrock look), and under them the level and the enchantment hinted at | live, both looks. A Java look baked before 2026-10-11 lacks the alphabet: run `lookbake java` again |
| Screens | Grindstone | tests only, never opened live |
| Screens | Cartography table, with a name box where the title would be (Bedrock's table renames; Java's has no box, so the place is ours) | live (Java look): a map put in and named, the result shown; taking it and the paper and pane recipes unseen. The box was moved 3 pixels down after the shot |
| Screens | Sign editor: the board with its four lines, Done or Esc writes; a hanging sign's smaller board and narrower lines | live (Java look) |
| Screens | Books: a right-click opens the held book's pages; a book and quill takes typing at a page's end and new pages, and is signed under a title | live |
| Screens | Loom: patterns as buttons (the pattern item's one, else the plain 32); the result shows the banner it makes, and so does a large preview in the frame above it (the result's icon at twice its size, not Java's banner model) | live (Java look) |
| Screens | Beacon: powers, second power and confirm as buttons (not either game's own screen); a power the pyramid under it is too small for stands in brackets and cannot be picked | live (a two-layer pyramid: four powers open, three in brackets; "(Strength)" touches its button's edge) |
| Screens | Creative inventory (E in creative mode): four tabs, a click takes one, Shift a stack; a stack carried onto the items is destroyed; a fifth tab ("Self") shows the player's own screen under the tab buttons; a sixth ("Find") lists every tab's items whose identifier holds the typed text | live (the search: "diamond s" left the sword, spear and shovel); the fifth tab by tests and by pixels sampled from a shot on testbox (the image was not looked at). The search reads identifiers, not the translated names, and has no text box art |
| Screens | Trading: the offers of a villager or wandering trader, a click trades once; a villager's level beside its name, later levels' offers crossed out | live (wandering trader; a villager: "Fisherman - Novice", Java look). No experience bar |
| Screens | Furnace family, hopper, dispenser/dropper, brewing stand | live (acacia-46) |
| Screens | Server forms, pause, options (two columns), death and respawn | live |
| Sound | Each look's own samples: Bedrock `.fsb` (FADPCM), Java `.ogg` | logs (testbox has no audio device) |

## Not done

- **Screens:** the beacon's own art; a trader's experience bar; tooltips (an enchanting
  option writes its hint under its galactic words instead); a book's
  art and a caret inside its text.
- **Dimensions:** a nether portal was walked through in the viewer (2026-10-10, creative, to a
  crimson forest); the way back and the end portal were checked in the bot's drill only. An
  arrival can take one correction when the server lags (docs/DESIGN.md "Dimension travel").
  The end portal's surface is the Bedrock pack's still image in both looks (seen live; the
  Java game draws it with a shader). A Java look baked before 2026-10-10 lacks it: run
  `lookbake java` again.
- **Entities:** a banner stack's patterns were seen on the model in another player's hand
  (2026-10-10, Java look); its slot icon and a shield's banner are by tests only. A camel was seen standing, saddled and ridden (2026-10-10), not
  sitting or dashing. The Bedrock attachable's own shield placement (Java's is used in both
  looks).
- **Sky:** The Nether's per-biome fog is in (unit-tested; only the crimson
  forest was seen live, before it landed).
- **Bedrock look:** weather, clouds, sky colours and first-person poses use Java's rules; Bedrock's
  own are unmeasured.
- **Sound:** music, ambience, and Java's event mapping where a Bedrock sound has no Java file.
- **Performance:** only measured on lavapipe (CPU, 25 to 40 fps at 1280×720 on testbox); no GPU
  frame times recorded. The dev machine has an Intel Arc but too little free memory to build the
  viewer beside the other sessions (2026-10-10).

## Testing it

- `tools/testbox-shot.sh out.png` with `NAME=<yours>` (same-name runs kick each other) and a BDS
  port. Env: `ACACIA_PLAY`, `ACACIA_LOOK`, `ACACIA_SETTINGS`, `ACACIA_SHOT_AFTER`.
- `ACACIA_COMMANDS="tp @s 20 -60 20;wait 3;summon zombie ~ ~ ~4"`: operator commands, `wait secs`
  between them (`src/net/setup.rs`). A summon right after a far teleport fails: wait first.
- `ACACIA_KEYS="5 +W; 9 -W; 12 +Use; 15 click 640 300; 16 type Hello"`: scripted input
  (`src/keyscript.rs`). Its clock starts at the player's first tick, `ACACIA_SHOT_AFTER`'s at
  launch (4 to 9 s earlier): leave that much between the last step and the shot.
- `/enchant @s ...` sent through `ACACIA_COMMANDS` had no effect (2026-10-09, no output either);
  written to the BDS console fifo (`echo "enchant <name> sharpness 1" > work/bds-forms/console`)
  it works. Armour is put on by holding it and `ACACIA_KEYS="18 +Use; 18.3 -Use"`.
- Patterned banners: no command writes them, so `cargo run -p acacia-bot --example banners -- <server> [x y z]`
  looms and places a row (it runs commands only on itself and within 40 blocks).
- Item frames: no command fills one, so `cargo run -p acacia-bot --example frames -- <server> [x y z]`
  hangs a row on a wall and fills it, the last with a map.
- A second player: `cargo run -p acacia-bot --example props -- <server> [x y z]` stands there
  in a cape (`ClientBuilder::skin`), facing north, leashes a cow, looms a patterned banner into its hand and sneaks with a shield
  for a minute.
- `RUST_LOG=chat=debug` logs each chat line's raw packet. A setup command's syntax error is not
  sent back: write the command to the BDS console fifo and read `bds.log` (the BDS on 19174 runs
  in `work/bds-render`).
- A villager with trades: `summon villager x y z 0 0 minecraft:spawn_farmer` (`villager_v2` is
  not a name this BDS takes).
- The action-test pack (BDS 19174) rebuilds its scene around wherever the player joins, which
  is where it last left: stay there, summon on a stage some 40 blocks off and shoot it with
  `ACACIA_AT`. A charged creeper left near the join point blows up at the next join.
- `testbox-shot.sh` keeps local mtimes: a file edited while the first build of a directory ran
  is older than that build and never rebuilt. `touch` it there (2026-10-10: three shots showed
  the old binary).
- A boat's paddle times are in the `riding` log line (`src/ride.rs`).
