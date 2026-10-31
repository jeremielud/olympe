# Olympe

*English · [Français](README.fr.md)*

A free, open-source engine written in Rust for *Zeus: Master of Olympus* and its
expansion *Poseidon* (Impressions Games, 2000–2001). It reads the original game's data
and runs it on today's computers: any screen resolution, free zoom, and an interface
scaled up on high-DPI displays. It runs on Linux, and the code is written to be
cross-platform.

**Olympe contains no game data.** You need your own copy of the game: an existing
installation, the CDs, or their images.

## Project status

Olympe is a rewrite in progress, not yet a complete replacement for the game.

### Progress: about 75 %

Estimate per area (October 2026), weighted by what a player runs into:

| Area | Progress |
|---|---|
| Installing and reading the data (maps, images, texts, sounds, videos) | 100 % |
| City display (terrain, buildings, walkers, minimap, zoom) | 90 % |
| Adventures, episodes, colonies and goals (152 of 152 episodes tracked) | 85 % |
| Music, sound effects, voices and narrated stories | 85 % |
| Economy and housing (employment, food, goods, evolution, taxes, risks) | 85 % |
| Interface (menus, info panels, overlays, journal, advisors, panels) | 80 % |
| Trade and world cities | 70 % |
| Mythology (gods, heroes, monsters, quests) | 70 % |
| Army and combat (walls, towers, gatehouses, distant campaigns and raids) | 70 % |
| Languages (English, Esperanto) and cheat menu | 75 % (the whole game interface is translated, English using the official game's terms; still French: command-line options and debugging output; no cheat menu yet) |

Still missing, notably: the image and specific rules of the Atlantean frigate (in an
Atlantean city, the wharf and ship carry their Atlantean names but keep the trireme's
image and rules), and the cheat menu.

Checked by playing: the first tutorial, the colony tutorial and the first episode of
*Perseus and Medusa* can be won with a city built by script (`essais/`).

What works:

- installing the data from an installed game, the contents of a CD, or a CD image;
- the adventure selection screen with all 40 adventures and their opening stories;
- map display, zoom, minimap, animated sea;
- building: roads, common and elite housing, agoras and shops, farms, workshops,
  storehouse, granary, culture, palace, stadium, beautification; dragging with a house
  selected fills the rectangle with adjoining houses, which flow around roads and
  obstacles (preview and cost while dragging, placed on release), and flowering
  meadows are tinted green while a farm is selected;
- simulation: immigrants, employment, walker routes, food and goods, neighbourhood
  appeal, housing evolution (the game's culture points: 15 for philosophy, 20 for
  athletics, 30 more with a stadium, 25 for drama, measured from its save files), taxes,
  fires and collapses (rates and thresholds from `Zeus.exe`); the workforce is shared as
  in the game: when labour is short, each building gets the same share of its jobs and
  works that much more slowly;
- the original music and ambient sounds;
- episode goals, read from the adventure (`O` key): population, treasury, housing,
  yearly profit and production are tracked, and the goods to set aside for the next
  city are reserved with the `R` key (food from the granary; fleece, oil, wine, armour,
  bronze, wood, marble and sculptures from the storehouse); the episode ends with its
  victory story once all are met;
- sanctuaries of the gods chosen by the episode (Mythology category), laid out as in
  the game; carriers bring marble, wood and sculptures from the storehouse to the site
  by road (provisional quantities), and sanctuary goals are tracked; once finished,
  those of Athena, Dionysus and Hephaestus provide olives, grapes and bronze, and each
  year the god comes in person to grant a blessing (Olympe's choice);
- Poseidon's monuments (pyramids, monuments, minor sanctuaries, altar and temple of
  Olympus…), offered when a goal requires them: laid out and raised step by step as in
  the game as marble, wood and sculptures arrive from the storehouse (provisional
  quantities); slopes, corners, steps and marble decorations as in the game, statues,
  altar and temple of the sanctuaries;
- Poseidon's black marble and orichalc: workshop and refinery placed anywhere
  (shortcut), goods stored in the storehouse;
- each episode's allowed buildings, read from the adventure (the table of the editor's
  "Allowed buildings" panel, at 0x902d30 in `Zeus.exe`): what an episode does not allow
  does not appear in the menus; farms, herding and raw materials still depend on the
  resources;
- Atlantean cities (flag at byte 648 of each episode, read at 0x902bb0 in `Zeus.exe`
  and checked in the game: Poseidon's four Atlantean adventures): science replaces
  Greek culture — library, university and observatory, inventors' workshop and
  laboratory, museum, with Poseidon's images; science points measured in the game:
  astronomy 20, learning 15, research 25 (the museum counts like the stadium, not
  measured);
- distant campaigns (world cities panel, `C` key): a panel picks the companies that
  leave — rabble, hoplites, horsemen and triremes, each from zero to the number
  available — and the rest guard the city; the company sets out to conquer or raid a
  foreign city, marches for as long as the world route takes (length × pace / 200
  months, at least two), fights, then returns, as in `Zeus.exe`: a rabble or hoplite
  counts as one, a horseman as two, a trireme as six (it can sink, and its wharf waits
  for it without building another); a city's defence is √(years, at most thirty) ×
  difficulty factor × its military level, halved if its army is away, and recovers one
  point per month; losses on both sides depend on the strength gap (the game's tables),
  and fallen soldiers are removed from the houses that supplied them; a successful raid
  brings back goods the city sells (up to what hoplites, horsemen and triremes can
  carry) or drachmas depending on its wealth;
- Atlantean soldiers: archers, spearmen and chariots from `poseidonimps.sg3` (same
  rules as rabble, hoplites and horsemen, as in the game's walker rules), with their
  walking and fighting animations;
- Atlantean chariots: the chariot factory gets wood from the storehouse and horses
  from the horse ranch (one of each per chariot, Olympe's quantities), stores its
  chariots in the storehouse (Poseidon's bays), and the agora's chariot vendor brings
  them to elite housing, which requires them instead of horses; black marble and
  orichalc also have their proper bays;
- view rotation (Home and End keys), as in the game: the terrain is re-tiled for the
  orientation by a transcription of `Zeus.exe`'s routine (0x521520: water, cliffs and
  ramps from the heights, trees, rocks, quarries, beaches, marshes, scrub, meadows,
  patches of grass), which reproduces 99.9 % of the map images at orientation 0;
  buildings keep their single view and their footprint rotates, the palace uses its
  images for the other direction, and the minimap and walkers rotate with the view;
  raised terrain is drawn 30 pixels higher per level, as in the game (heights from
  block 23 of the maps), and the mouse picks the raised tile drawn under it;
- Panhellenic Games (episodes that schedule them, byte 808): each year the Olympian,
  Isthmian, Nemean or Pythian Games depending on the year, as in `Zeus.exe`; entrants
  leave in the sixth month and the result comes in the eighth, based on coverage and
  the number of venues for the discipline (a hero in the city helps, and so does a
  sanctuary of Apollo); a victory earns a commemorative monument and the favour of the
  world cities, and winning the Olympian Games means hosting the next ones, which
  requires a stadium;
- `olympe --bilan` checks that every episode of every installed adventure has all its
  goals tracked and achievable with its allowed buildings, room on the map for what has
  to be produced, and enough allowed culture for the housing required (152 of 152);
- hippodrome (Administration, Atlantis adventures): track sections laid end to end,
  straight or curved depending on their neighbours; a closed loop hosts races and earns
  money every month (Olympe's rates), and the goal counts one stadium per section
  (Olympe's assumption);
- walls (Military tab, drawn like roads, 5 drachmas per tile): each tile's image
  follows its neighbours according to the game's table; invaders walk around them and,
  if there is no way through, undermine the nearest one until it breaches (Olympe's
  strength). Walls also stop citizens: the gatehouse (type 131, two images depending on
  the road's direction), placed on a road, lets them through and bars the way to
  invaders;
- towers (Military tab, rules of type 132): when staffed, their sentries shoot at the
  nearest invader in range, with the range, ranged attack and rate of fire of the
  "tower sentry" in the walker rules;
- trireme wharf (Military tab), on the shore: it takes two loads of wood and two of
  armour from the storehouse and launches a trireme 32 days later (as in the game; it
  has no recognised wharf image, so a fishery stands in for it); trireme upkeep goals
  are tracked;
- heroes, quests and monsters: each hero's hall (Mythology category), with requirements
  read from the game, including an active hippodrome (Atlantean cities) and an active
  stadium or museum; a hero who comes to the city fights the monster assigned to them,
  then leaves on their quest, and the matching goals are tracked; monsters enter the
  city and destroy a building every two months until their hero, coming from their
  hall, kills them (sea monsters stay offshore); a released or invading monster arrives
  on schedule;
- hostile gods in person: on the date of their event they enter the city and ravage it
  for six months, only one if a defending god has a sanctuary there (as in the game,
  Zeus against all, Aphrodite against Ares, Hephaestus, Hermes and Dionysus, Hera
  against Zeus; six-month stay as in the game, but a shortened duration and Olympe's
  rate of destruction);
- soldiers supplied by housing (rabble, hoplites, horsemen, using the adventure
  editor's figures) for military goals, without triremes;
- world cities read from the map (status, military strength, open trade); the `C`
  panel, from which the army is sent (see distant campaigns), which tracks the "Rule"
  goals; as in the game, trade goes through a trading post for a city linked by land or
  through docks on the shore for a city linked by sea (world route type, byte 0x140),
  with eight-bay storehouses each assigned to a partner (the game lets you choose,
  Olympe takes the first one that has none); distant cities trade too: each month the
  city sells what a partner buys beyond a reserve and buys what it sells when reserves
  are low (goods, quantities and prices read from the map); the panel shows what each
  city sells and buys, and sets per good what trade may do with it (sell the surplus
  and buy, one of the two, or nothing); vassals and founded colonies pay the yearly
  tribute set in the editor; rival cities send their armies on schedule: soldiers enter
  at the event's invasion point, or land from a ship if it comes by sea, the city's
  hoplites march out of the palace to meet them, and they fight with the hit points,
  attacks, armour and ranges of the game's rules (`figure_model`), the enemy army being
  made up according to its unit frequencies (Greeks, Trojans, Persians, centaurs,
  Amazons, Egyptians, Mayans, Phoenicians, Oceanids or Atlanteans, depending on the
  city); if the invaders win, they burn buildings and leave with a fifth of the
  treasury; gifts from cities arrive on schedule, and those from the mother city bring
  the colony what it had set aside; the episode's events change the world on their
  date (a city becomes a rival, an ally or a vassal, rebels, gains or loses its army)
  and bring city requests (goods, food, drachmas, soldiers), which are fulfilled from
  the `C` panel: serving a city raises its favour, ignoring it angers it, until it
  becomes a rival (Olympe's rates); events also change prices, cities' supply and
  demand, and wages, and send a rival's army away (its defence is then halved);
  disasters strike the areas painted in the editor from their starting point: an
  earthquake opens the rift, lava covers the land, a tidal wave floods the coast for six
  months (forever if the adventure says so), a landslide covers the foot of the slope
  with rubble, and the buildings hit are destroyed;
- episodes follow one another in the adventure's order, with their narrated stories
  (the game's voices, recovered for the Zeus and Poseidon adventures and for those in a
  folder), their goals and their resources: leaving for a colony on its own map (the
  player's choice when the adventure offers more colonies than it has colony episodes),
  returning to the mother city, end of the adventure; the save file remembers the
  current episode and the chosen colonies;
- the game's interface: top bar with File, Options and Help menus; side panel with
  category tabs, category information (service coverage, free housing…), "See…"
  buttons, illustrated buttons for building groups, general map tab, message area;
  tooltips on hover; a building's info panel on right-click; pause with `P` and its
  banner;
- a second, modern interface, which the `I` key switches to and from: dark top bar with
  plaques, and a build panel listing every building with its thumbnail, price and
  description;
- overlays (`V` key): water, fire and collapse risk, appeal, crime, culture, health and
  taxes, with each building tinted from green to red;
- house info panel: what a house lacks to evolve; a blocked service says what it is
  waiting for (palace for tax collectors, school for philosophers and actors);
- advisors (`A` key, Tab to change page): population per housing level, employment per
  category, finances, services and stocks, mythology, army and trade, in text panels
  as in the game;
- the game's event letters (`model/zeus eventmsg.txt`): city requests (in a tone that
  matches their status), gifts, invasions, gods, heroes, finished sanctuaries, trade,
  wages and plague open like the stories, dated, with the name of the city and its
  leader (spelled out in Poseidon's maps);
- clicking a walker (water carrier, guard, philosopher, god…) makes them speak, with
  the game's voices;
- the introduction video, converted by the installer (with ffmpeg) into images and
  sound that Olympe plays itself, at startup (`--sans-intro` skips it);
- five difficulty levels (Beginner to Olympian, `D` key on the adventure screen or
  `--difficulte`), with their rule files and the starting treasury adjusted as in the
  game;
- adjustable tax rate (`+` and `-` keys), from "none" to "outrageous" with the game's
  rates, and city sentiment computed as in `Zeus.exe`, twice a month: taxes depending
  on difficulty (`tax_sentiment_model.txt`), wages, unemployment, each house's food and
  debts, damped outside the 41–60 band, and with no negative effect until the city
  exceeds 349 inhabitants; every day, sentiment drives migration as in the game: from
  12 newcomers above 70 to none between 36 and 49, then up to 3 departures a day below
  16 (not below 100 inhabitants), newcomers going first to empty houses and leavers
  leaving the humblest housing first; no more newcomers after three years of debt;
- crime in common housing as in the game (increment and base for each level, distance
  of sentiment from 40, passing guards): above 80 a house produces a thief, who robs an
  elite house or the treasury;
- permanent blessings of the sanctuaries, as in the game: Dionysus lowers crime,
  Aphrodite keeps all inhabitants and lowers by 10 the appeal housing needs, Hephaestus
  removes all fire risk, Poseidon makes fishing take two thirds of the time, Hera adds 3
  points to population growth, Hermes makes carriers a third faster, Athena gives the
  city's soldiers 4 points of armour against missiles, Atlas speeds up construction
  (the wharf fits out its ship in 16 days instead of 32), Apollo adds 20 to the city's
  score at the Panhellenic Games; Zeus, Demeter, Artemis, Ares and Hades act through
  their events (defence against hostile gods, monsters, invasions) rather than through
  a permanent blessing;
- city health as in the game (services advisor): each month it moves towards the share
  of inhabitants visited by physicians and who have eaten, plus a bonus for cities under
  1,000 inhabitants; below 75 an epidemic can break out (up to 3 chances in 100 a day
  below 10, at most one per month) and strike 40 % of houses, the least cared-for
  first, killing from a quarter of the inhabitants on very easy to all of them on very
  hard; each year, health makes part of the population be born (up to 6 % above 90) or
  die (up to 5 %);
- saving and loading a game.

All installed adventures can be played from start to finish: `olympe --bilan` checks
that each of the 152 episodes has its goals tracked and achievable, and each adventure
runs through to its closing story.

What is approximated or missing: archers' shots are drawn as lines, since the catalogs
have no arrow images; resource buildings are placed within eight tiles of their deposit
(trees, marble quarry, copper rocks, black marble, orichalc), unless the map has none
while a goal requires one; the top of the pyramids and the observatory and museum
placed on monuments are drawn as obelisks; the interface is Olympe's own, dressed in
the original images. The rates of producers and workshops, and the use of fleece, oil
and wine in houses, are the game's; those of the fishery (depending on the distance to
the nearest fishing spot placed in the editor), the dairy and the hunting lodge were
measured in the game, but Olympe shows no boats, goats or game animals. Other balance
settings (amounts of materials) are approximations rather than the game's values, as is
the thieves' loot.


## Installation

You need:

- [Rust](https://www.rust-lang.org/tools/install) 1.85 or newer;
- the **SDL3** and **Opus** libraries with their development files (`SDL3-devel` and
  `libopus-devel`, or `libsdl3-dev` and `libopus-dev`, depending on the system);
- **ffmpeg**, optional, to convert the game's music when installing the data. Without
  it the game works, but without music.

On Linux and other Unix systems:

```sh
./installer.sh
```

The script builds Olympe and installs the program in `~/.local/bin`, with a menu entry
and an icon. On any system you can also run:

```sh
cargo install --path . --bin olympe
```

## Installing the game data

```sh
olympe installer <source> [<source>…]
```

A source is any of:

- the folder where the game is installed;
- a folder holding the contents of a CD;
- a CD image (`.iso`, `.bin`, raw or not).

With the CDs, give the Zeus CD first, then the Poseidon CD:

```sh
olympe installer "zeus cd.iso" "poseidon cd.iso"
```

An official patch (for example the program `mise_a_jour_2_1_fr_poseidon.exe`) is given
as one more source, last, or on its own afterwards:

```sh
olympe installer mise_a_jour_2_1_fr_poseidon.exe
```

Olympe finds the data files it patches and updates them; the Windows program itself is
never run.

The data goes into the system's usual folder (`~/.local/share/olympe` on Linux,
`~/Library/Application Support/olympe` on macOS, `%APPDATA%\olympe` on Windows), or
into the one given with `--vers`. It takes about 700 MB and a minute.

## Playing

```sh
olympe                              # adventure selection screen
olympe "les aventures de jason"     # an adventure by name
olympe path/to/map.map              # a specific map
```

In game, the `H` key shows the controls. `olympe --aide` lists the options (the
command-line options and in-game texts are in French by default).

The language is chosen with `--langue en` or `--langue eo` (or with `L` on the
adventure screen); the choice is remembered in the `langue` file of the user's
configuration folder. The game's texts come from the language's subfolder (`en`) when
it exists; Esperanto has its own translations. Esperanto voices, when available, go in
`donnees/eo/audio/voice/campaign` and take precedence over those of the installation.
The English interface uses the terms of the official English release.

Two interfaces are available: the original game's (the default) and a modern one, with
a dark top bar with plaques and a build panel listing every building with its
thumbnail, price and description. The `I` key or the Options menu switches between
them, and the choice is remembered in the `interface` file of the configuration
folder; `--interface moderne` or `--interface origine` forces one for a single launch.

Games are saved with `F5` and loaded with `F9`; they are stored in the `sauvegardes`
subfolder of the data folder.

## Development

```sh
cargo test --release
```

Tests that use the game data look for a `donnees/` folder at the root of the repository
and do nothing if it is missing. To create it:

```sh
olympe installer <sources> --vers donnees
```

The code is commented in French. The original game's formats (images, maps,
adventures, texts, CD archives) are read by modules with no external dependency; only
display and sound go through SDL3 and Opus.

Debugging tools: `zeus-iso`, `zeus-cab`, `zeus-sprites`, `zeus-map`, `zeus-text`,
`zeus-jpeg`, `zeus-opus` (see `src/bin/`). `olympe --mesure 600` draws 600 frames
without vertical sync and reports the frame rate.

## Platforms

Olympe is developed and tested on Linux (Wayland, OpenGL rendering through SDL3).

On the development machine (integrated GPU), the game was tested with every
combination SDL3 offers: Wayland or X11 display, OpenGL, OpenGL ES 2, Vulkan, SDL's
"gpu" renderer, and software rendering without a GPU. All of them work; a city is
drawn in 1 to 2 ms per frame with acceleration, 6 ms with software rendering. To force
a choice: `SDL_VIDEODRIVER=x11` or `SDL_RENDER_DRIVER=software`, for example.

For Windows (`x86_64-pc-windows-gnu`) and macOS (`aarch64-apple-darwin`), the code
passes the compiler's checks (`cargo check --target …`): nothing in it is specific to
one system. Linking with SDL3 and Opus, and the game itself, have not been tried there
yet.

## License

Olympe is distributed under the GNU GPL, version 3 or later (see `LICENSE`).

*Zeus: Master of Olympus* and *Poseidon* are trademarks and works of their respective
owners. This project is neither affiliated with nor endorsed by them.
