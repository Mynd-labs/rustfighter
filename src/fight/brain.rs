//! Fighter brain — the per-frame driver process, ported from
//! `docs/port/action-execution.md` §7 (addresses: `0x17665`, `0x176CA`,
//! `0x17929`, `0x179B7`, `0x176FC`, `0x10C5D`, `0x10D45`, `0x112B0`, `0x11172`).
//!
//! One frame of a fighter, in engine order:
//!
//! 1. freeze check + recursion into a throwing opponent (`0x17665`)
//! 2. brain steps (`0x176CA`): dizzy → KO → energy-zero → connect timer →
//!    hit reaction → input/AI action request → action start
//! 3. script commit + mode-4 tick + movement (`0x176FC`/`0x10C5D`/`0x10D45`)
//! 4. world→screen placement and the "done this frame" bit
//!
//! Fidelity notes (explicit, not invented):
//! * The per-character hooks (modes 0/2/4/6) are **no-ops** here; their
//!   documented side effects (KEN/RYU sounds, fireball spawns) are deferred
//!   until the handler table is ported.
//! * The motion/special recogniser (`0x14E1B`) is **not decoded**; the input
//!   decoder treats its result as "no special" (its data patterns are kept in
//!   [`crate::fight::tables::MOTION_PATTERNS`]).
//! * The step/wall-clamp physics follows the disassembly of `0x112B0`/`0x11172`
//!   including the ground clamp at Y=200; bonus-stage branches (levels 0x18 /
//!   0x1C) keep the same structure with their constants.

use crate::fight::ait::AitTable;
use crate::fight::chars;
use crate::fight::ani::AniTable;
use crate::fight::fighter::{flag_a, DrawObject, Fighter, ACTION_IDLE, FRAME_ACC_STEP};
use crate::fight::frm::FrmTable;
use crate::fight::hit::{self, DamageOutcome};
use crate::fight::rng::Rng;
use crate::fight::round::Round;
use crate::fight::tables;
use crate::gamedata::GameData;

/// All 12 characters' gameplay tables, loaded once.
pub struct FightAssets {
    /// Fireball sprite frames (`sprFIREBAL0/1`, the flicker pair).
    pub fireball: Vec<crate::sprite::Frame>,
    /// Sonic boom sprite frames.
    pub sonic: Vec<crate::sprite::Frame>,
    /// Tiger shot sprite frames.
    pub tiger: Vec<crate::sprite::Frame>,
    /// `frm<CHAR>` tables in engine character order.
    pub frm: Vec<FrmTable>,
    /// `ani<CHAR>` tables in engine character order.
    pub ani: Vec<AniTable>,
    /// `ait<CHAR>` tables in engine character order.
    pub ait: Vec<AitTable>,
}

impl FightAssets {
    /// Load and parse all 36 tables.
    pub fn load(gd: &GameData) -> Result<Self, String> {
        let mut frm = Vec::with_capacity(12);
        let mut ani = Vec::with_capacity(12);
        let mut ait = Vec::with_capacity(12);
        for stem in crate::gamedata::ENGINE_TABLE_STEMS {
            let fname = format!("frm{stem}");
            let aname = format!("ani{stem}");
            let tname = format!("ait{stem}");
            frm.push(
                FrmTable::parse(&fname, &gd.decoded(&fname).map_err(|e| format!("{fname}: {e}"))?)
                    .map_err(|e| e.to_string())?,
            );
            ani.push(
                AniTable::parse(&aname, &gd.decoded(&aname).map_err(|e| format!("{aname}: {e}"))?)
                    .map_err(|e| e.to_string())?,
            );
            ait.push(
                AitTable::parse(&tname, &gd.decoded(&tname).map_err(|e| format!("{tname}: {e}"))?)
                    .map_err(|e| e.to_string())?,
            );
        }
        let mut fireball = Vec::new();
        for name in crate::fight::chars::FIREBALL_SPRITES {
            if let Ok(frame) = gd.sprite(name) {
                fireball.push(frame);
            }
        }
        let mut sonic = Vec::new();
        for name in crate::fight::chars::SONIC_SPRITES {
            if let Ok(frame) = gd.sprite(name) {
                sonic.push(frame);
            }
        }
        let mut tiger = Vec::new();
        for name in crate::fight::chars::TIGER_SPRITES {
            if let Ok(frame) = gd.sprite(name) {
                tiger.push(frame);
            }
        }
        Ok(FightAssets { fireball, sonic, tiger, frm, ani, ait })
    }
}

/// A live projectile (fireball / sonic boom / tiger shot).
#[derive(Debug, Clone, Copy)]
pub struct Projectile {
    /// World X.
    pub x: i16,
    /// World Y (player space).
    pub y: i16,
    /// Horizontal speed.
    pub x_speed: i16,
    /// Reaction id applied on hit (the spawning action).
    pub action: u8,
    /// Owner player index.
    pub owner: usize,
    /// Flicker selector (0 / 2), toggled each tick (`0x16AA3`).
    pub flicker: u8,
    /// Sprite size used for the hit box.
    pub width: u8,
    /// Sprite height.
    pub height: u8,
    /// Sprite set / behaviour.
    pub kind: chars::ProjKind,
    /// Animation cursor into the sprite set.
    pub anim: usize,
}

/// The two fighters and the globals one fight frame touches.
pub struct FightState {
    /// Player structs (P1, P2).
    pub players: [Fighter; 2],
    /// Driver draw objects (P1, P2).
    pub objects: [DrawObject; 2],
    /// Global freeze (`[0x322F] == 0xFF`).
    pub freeze: bool,
    /// Level/opponent id (`[0x25BE]`).
    pub level: u16,
    /// Camera (`[0x3BA3]`) and pending camera (`[0x3BA7]`).
    pub camera_x: i16,
    /// Pending camera target.
    pub camera_pending: i16,
    /// Camera-dirty flag (`[0x3C62]`).
    pub camera_dirty: bool,
    /// Ground Y (`[0x25C8]`).
    pub ground_y: i16,
    /// Stage right edge (`[0x25CA]`).
    pub stage_right: i16,
    /// Maximum camera pan (`stage_width - 320`).
    pub stage_max_pan: i32,
    /// Pseudo-random generator.
    pub rng: Rng,
    /// 32-bit scores per player.
    pub score: [u32; 2],
    /// `[0x322E]` — KO latch.
    pub ko_flag: bool,
    /// Round state.
    pub round: Round,
    /// Live projectiles (max 8, engine slots).
    pub projectiles: Vec<Projectile>,
    /// Sound requests collected from the character hooks (drained by the host).
    pub sound_events: Vec<u8>,
    /// One-shot sound latches `[0x37A6]` / `[0x37A7]`.
    pub latches: (bool, bool),
    /// Debug action override: `(player, action)` replaces the chooser's
    /// request for that player (used by tests and the fight build's keys).
    pub debug_action: Option<(usize, u8)>,
    /// Per-player input history rings for the motion recogniser (`0x2F4B`).
    pub hist: [crate::fight::motion::Hist; 2],
}

impl FightState {
    /// Create a match state for two character indices and a level.
    pub fn new(char1: usize, char2: usize, level: u16) -> Self {
        let mut p1 = Fighter::spawn(0x50, false);
        let mut p2 = Fighter::spawn(0xD0, false);
        p1.flags_a &= !flag_a::CPU;
        p2.flags_a |= flag_a::CPU;
        let o1 = DrawObject::spawn(0, (char1 * 2) as u8, 0x50, 200);
        let o2 = DrawObject::spawn(1, (char2 * 2) as u8, 0xD0, 200);
        FightState {
            players: [p1, p2],
            objects: [o1, o2],
            freeze: false,
            level,
            camera_x: 0,
            camera_pending: 0,
            camera_dirty: false,
            ground_y: 0,
            stage_right: 0x110,
            stage_max_pan: 0,
            rng: Rng::default(),
            score: [0, 0],
            ko_flag: false,
            round: Round::new(),
            projectiles: Vec::new(),
            sound_events: Vec::new(),
            latches: (false, false),
            debug_action: None,
            hist: [Default::default(), Default::default()],
        }
    }

    /// Establish the initial stand script for both fighters — the engine's
    /// constructor commits the idle action, which is what sets the low
    /// `flags_a` bits (`0x10D74`) that later input decoding reads.
    pub fn prime(&mut self, assets: &FightAssets) {
        for i in 0..2 {
            let opp = 1 - i;
            self.commit_named_action(assets, i, opp, 0x47);
            // Run one commit chain so the script/header flags are applied.
            self.script_commit(assets, i, opp);
        }
    }

    /// Process one frame for both fighters in engine order (`0x17665` per
    /// driver; the object pass calls player 1's driver first, `0x13C07`).
    pub fn run_frame(&mut self, assets: &FightAssets, inputs: [u8; 2], ai: [bool; 2]) {
        // The "updated this frame" bit is cleared for both players first.
        for p in &mut self.players {
            p.flags_b &= !0x40;
        }
        for (i, input) in inputs.iter().enumerate() {
            self.hist[i].update(*input);
        }
        self.run_driver(assets, 0, inputs, ai);
        self.run_driver(assets, 1, inputs, ai);
        self.tick_projectiles();
        self.hit_pass(assets);
    }

    fn run_driver(&mut self, assets: &FightAssets, i: usize, inputs: [u8; 2], ai: [bool; 2]) {
        if self.freeze {
            return;
        }
        let opp = 1 - i;
        // 0x17687: I am frozen and the opponent has not run yet -> opponent first.
        if self.players[i].flags_b & 0x0004 != 0
            && self.players[opp].flags_b & 0x0040 == 0
        {
            self.run_driver(assets, opp, inputs, ai);
        }
        // 0x176A5
        if self.players[i].flags_b & 0x0040 != 0 {
            return;
        }
        if self.players[i].flags_a & flag_a::CPU != 0 {
            // The shipped build skips CPU fighters here (see the analysis §8);
            // the port also drives them through this same path when `ai[i]`.
        }
        let _ = ai;
        let committed = self.logic(assets, i, opp, inputs[i], ai[i]);
        if !committed {
            self.script_commit(assets, i, opp);
        }
        // 0x176B4: place the driver object.
        self.objects[i].x = self.players[i].x.wrapping_sub(self.camera_x);
        self.objects[i].y = self.ground_y.wrapping_add(self.players[i].y);
        self.players[i].flags_b |= 0x0040;
    }

    /// `0x176CA` — the ordered brain steps. Returns true when the action was
    /// already final (CF set; no script commit should run).
    fn logic(&mut self, assets: &FightAssets, i: usize, opp: usize, input: u8, ai: bool) -> bool {
        self.dizzy_step(i);
        self.ko_step(assets, i);
        if self.players[i].energy == 0 {
            self.energy_zero_step(assets, i);
        }
        self.connect_timer(i);
        self.reaction_step(assets, i, opp);
        let request = match self.debug_action {
            Some((player, action)) if player == i => action,
            _ => self.choose_action(assets, i, opp, input, ai),
        };
        self.start_action(assets, i, opp, request);
        false
    }

    /// `0x17737` — dizzy/stun handling.
    fn dizzy_step(&mut self, i: usize) {
        let obj = &mut self.objects[i];
        let p = &mut self.players[i];
        if p.flags_a & hit::KO_FLAG != 0 {
            return;
        }
        if p.b19 >= 0x32 && obj.flags38 & 0x10 != 0 {
            // Full stun: two variants selected by the current frame slot and
            // the reaction id (0x17737..0x1778x).
            if (obj.sub_timer == 8 || obj.sub_timer == 0x34 || obj.sub_timer == 0xF4)
                || (0x28..=0x33).contains(&p.reaction_anim)
            {
                p.b19 = 0x31;
                p.flags_b |= 0x0010;
                self.force_script(i, 0x30);
            } else {
                p.b19 = 0x33;
                self.force_script(i, 0x0D);
            }
        } else if p.b19 != 0 && p.b19 < 0x32 {
            p.b19 -= 1;
            let char = (obj.char2 / 2) as usize;
            let threshold = tables::DIZZY_THRESHOLD
                .get(char)
                .copied()
                .unwrap_or(0x20);
            if p.b19 < threshold {
                p.flags_b &= !0x0010;
                self.force_script(i, 6);
            }
        }
    }

    /// `0x177B3` — KO / round-lost handling.
    fn ko_step(&mut self, _assets: &FightAssets, i: usize) {
        if self.players[i].flags_a & 0x8000 == 0 {
            return;
        }
        let p = &mut self.players[i];
        p.flags_a |= hit::KO_FLAG;
        p.flags_b &= !(0x0010 | 0x0080 | 0x0020);
        // Script 0x0E (+1 when the battle record says the bout is over).
        let script = 0x0E;
        self.force_script(i, script);
    }

    /// `0x17803` — energy reached zero.
    fn energy_zero_step(&mut self, _assets: &FightAssets, i: usize) {
        if self.players[i].flags_a & hit::KO_FLAG != 0 {
            return;
        }
        self.players[i].flags_a |= hit::KO_FLAG;
        self.players[i].flags_b &= !0x0004;
        self.players[i].flags_b &= !0x0080;
        self.players[i].flags_b &= !0x0020;
        self.ko_flag = true;
        self.force_script(i, 0x0D);
    }

    /// `0x17830` — hit-connected timer.
    fn connect_timer(&mut self, i: usize) {
        let p = &mut self.players[i];
        if p.flags_a & 0x0080 != 0 && p.b20 != 0 {
            p.b20 -= 1;
            if p.b20 != 0 {
                p.flags_a &= !0x0080;
            }
        }
    }

    /// `0x17849` — deferred hit reaction.
    fn reaction_step(&mut self, assets: &FightAssets, i: usize, opp: usize) {
        if self.players[i].flags_a & hit::WAS_HIT == 0 {
            return;
        }
        self.players[i].flags_b |= 0x0800; // reacting
        self.players[i].flags_a &= !hit::WAS_HIT;
        let mut script: u8 = 0;
        if self.players[i].flags_b & 0x1000 != 0 {
            script = 0x3D;
        }
        let reaction = self.players[i].reaction_anim;
        // Trade/interrupt guard: no reaction when both in throws with matching
        // control bits (0x17867..0x1789x).
        if (0x34..=0x37).contains(&reaction) && self.players[i].flags_a & 0x0040 != 0 {
            let mine = (self.players[i].flags_a & 3) as u8;
            let theirs = (self.players[opp].flags_a & 3) as u8;
            if theirs == 2 || mine == theirs {
                return;
            }
        }
        // Damage uses the opponent's ait power table (0x1857C with SI = the
        // opponent's driver object). The per-player damage-scale structure
        // ([0x2F0C]/[0x2F15]) is not ported yet, so the default `* 0xD9 >> 8`
        // path (the CPU / no-selector case) is used for both sides.
        let opp_char = (self.objects[opp].char2 / 2) as usize;
        let dizzy = self.players[i].flags_a & hit::DIZZY != 0;
        let char2 = self.objects[i].char2;
        let table = assets.ait[opp_char]
            .power_table()
            .map(|t| t.to_vec())
            .unwrap_or_default();
        let outcome: DamageOutcome = hit::apply_reaction_damage(
            &mut self.players[i],
            char2,
            reaction,
            &table,
            None,
            dizzy,
        );
        let _ = outcome;
        let score = assets.ait[opp_char]
            .score_table()
            .and_then(|t| t.get(reaction as usize))
            .copied()
            .unwrap_or(0);
        self.score[i] = self.score[i].saturating_add(score as u32);

        // Drop timers.
        self.players[i].b1a = 0;
        self.players[i].ai_timer = 0;
        self.players[i].b1b = 0;
        self.players[i].b21 = 0;
        if self.players[i].flags_b & 0x0010 != 0 {
            self.players[i].b19 = 0;
            self.players[i].flags_b &= !0x0010;
        }
        // Reaction scripts by attacker character (0x178C5..).
        if self.players[i].flags_a & 0x0040 == 0 {
            match opp_char {
                11 => script = 0x3D, // M. BISON
                6 => script = 0x3D,  // DHALSIM
                7 if reaction == 0x34 => script = 0x32, // BLANKA
                _ => {}
            }
            if script == 0 {
                script = 0x0D;
            }
        }
        // Snap to the ground unless in a jump (0x1790F).
        if self.players[i].flags_a & 3 != 2 {
            self.players[i].y = 0xC8;
        }
        self.force_script(i, script);
    }

    /// Commit action `0x3A` with the given script (`0x17BD7`).
    fn force_script(&mut self, i: usize, script: u8) {
        let obj = &mut self.objects[i];
        obj.flags38 &= 0x7F;
        self.commit_action(i, ACTION_IDLE, script, 0);
    }

    /// `0x17929` — input or AI; returns the requested action id.
    fn choose_action(
        &mut self,
        assets: &FightAssets,
        i: usize,
        opp: usize,
        input: u8,
        ai: bool,
    ) -> u8 {
        // AI timer commitment.
        if self.players[i].ai_timer != 0 {
            self.players[i].ai_timer -= 1;
            return self.stand_action(i);
        }
        if self.players[i].flags_b & 0x0200 != 0 {
            return self.stand_action(i);
        }
        if self.players[i].flags_b & 0x0004 != 0 {
            return self.objects[i].action; // frozen: keep the current action
        }
        if ai {
            self.ai_decide(assets, i, opp)
        } else {
            // Human path (0x1794C..): raw input byte in player[0x17].
            self.players[i].b17 = input;
            self.decode_input(assets, i, opp, input)
        }
    }

    fn stand_action(&self, i: usize) -> u8 {
        if self.players[i].flags_a & 3 == 2 {
            0x48
        } else {
            0x47
        }
    }

    /// `0x1075A` — human input decoder, full flow (see
    /// `docs/port/input-recogniser.md` §5): attack/special path with the
    /// motion recogniser, normal-attack lookup from the `ait` sections, the
    /// throw-counter path and the per-input action table.
    fn decode_input(&mut self, assets: &FightAssets, i: usize, opp: usize, input: u8) -> u8 {
        let char = (self.objects[i].char2 / 2) as usize;
        let mut class = tables::input_class(input);
        if class == 0x08 {
            return self.stand_action(i);
        }
        if class == 0xFF {
            class = 0;
        }
        let al = input & 0x7F;
        if al == 0 {
            return 0x47;
        }
        let facing_right = self.objects[i].facing & 4 != 0;
        if al & 0x10 != 0 {
            // Attack/special path (0x107D9).
            let mut class2 = class;
            if !facing_right {
                let remapped =
                    (input & 0xF0) | tables::INPUT_REMAP[(input & 0x0F) as usize];
                class2 = tables::input_class(remapped);
                if class2 == 0xFF {
                    class2 = 0;
                }
            }
            // The original poller is an ISR ticking several times per game
            // frame; refresh the open run so a freshly pressed attack is
            // visible to the recogniser in the same frame (the ring's
            // stale-byte write would otherwise delay it one frame).
            if self.hist[i].newest_byte() != input {
                self.hist[i].update(input);
            }
            if let Some(cl) = crate::fight::motion::recognise(&mut self.hist[i], char, facing_right)
            {
                let mut action = 0x34 + cl;
                // 0x1082E: char2 == 8 (ZANGIEF) remaps 0x35 -> 0x33.
                if self.objects[i].char2 == 8 && action == 0x35 {
                    action = 0x33;
                }
                return action;
            }
            // Normal-attack lookup (0x1083E).
            let state = (self.players[i].flags_a & 3) as usize;
            let (section, index) = if input & 0x0F == 0 {
                // Button only: sub-table hdr[7], index = state*2 (keyboard).
                (7usize, state * 2)
            } else {
                // With direction: hdr[8] (keyboard), index =
                // class*8 + distance bucket + state*64.
                let dx = (self.players[i].x as i32 - self.players[opp].x as i32)
                    .unsigned_abs() as u16;
                let bucket = crate::fight::round::distance_bucket(dx) as usize;
                (8usize, class2 as usize * 8 + bucket + state * 64)
            };
            if let Some(table) = assets.ait[char].section(section) {
                if let Some(&action) = table.get(index) {
                    return action;
                }
            }
            return self.stand_action(i);
        }
        // Counter path: opponent attacking, I press back (0x1078A).
        if self.players[opp].flags_a & 0x0020 != 0 && self.players[i].flags_a & 3 != 2 {
            let back = if facing_right { al & 0x04 != 0 } else { al & 0x08 != 0 };
            if back {
                return if self.players[i].flags_a & 3 == 1 {
                    0x3E
                } else {
                    0x3F
                };
            }
        }
        let state = (self.players[i].flags_a & 3) as u8;
        let action =
            tables::input_action(class, state).unwrap_or_else(|| self.stand_action(i));
        // Guard: with no direction held the per-state table must not jump
        // (pending table-verification; see the analysis notes).
        if matches!(action, 0x42..=0x44) && input & 0x0F == 0 {
            return self.stand_action(i);
        }
        action
    }

    /// `0x10524` — the AI decision.
    fn ai_decide(&mut self, assets: &FightAssets, i: usize, opp: usize) -> u8 {
        let opp_attacking = self.players[opp].flags_a & 0x0020 != 0
            && self.players[i].flags_a & 3 != 2;
        let mut action = if opp_attacking {
            match self.ai_counter_attempt(i, opp) {
                Some(counter) => counter,
                None => self.ai_bucket_pick(assets, i),
            }
        } else {
            self.ai_bucket_pick(assets, i)
        };
        // Decision delay from `obj[0x1C]` (0x10554).
        let obj = &self.objects[i];
        let delay = ((((obj.flags as u16) ^ 7) >> 1).saturating_sub(
            ((obj.flags38 as u16).wrapping_sub(0x12)) >> 1,
        )) as u8;
        self.players[i].b1a = delay;
        self.players[i].b1b = 0;
        if (0x18..=0x2B).contains(&self.objects[i].action) {
            action = 0x47;
        }
        if action == 0x43 && self.objects[i].facing & 4 != 0 {
            action = 0x44;
        }
        if action == 0x44 && self.objects[i].facing & 4 != 0 {
            action = 0x43;
        }
        if matches!(action, 0x42..=0x44) {
            self.players[i].b1a = 0;
        }
        action
    }

    /// `0x10645` / `0x1068D` — throw/counter attempt when the opponent attacks.
    fn ai_counter_attempt(&mut self, i: usize, opp: usize) -> Option<u8> {
        if self.players[i].flags_a & 3 == 2 {
            return None;
        }
        if self.players[i].flags_b & 0x0800 != 0 && self.players[i].flags_a & 0x4000 == 0 {
            return None;
        }
        if matches!(self.objects[i].action, 0x3E | 0x3F) {
            return Some(self.objects[i].action);
        }
        let state = (self.players[i].flags_a & 3) as u8;
        let roll = (self.rng.next() & 0x7FFF) as u16;
        let threshold = tables::cpu_throw_threshold(state);
        if roll >= threshold {
            let opp_state = (self.players[opp].flags_a & 3) as u8;
            Some(if opp_state == 1 { 0x3E } else { 0x3F })
        } else {
            None
        }
    }

    /// `0x105B1` — distance-bucket action pick from `ait[hdr[3]]`.
    fn ai_bucket_pick(&mut self, assets: &FightAssets, i: usize) -> u8 {
        let dx = (self.players[i].x as i32 - self.players[1 - i].x as i32)
            .unsigned_abs() as u16;
        let bucket = crate::fight::round::distance_bucket(dx);
        let state = (self.players[i].flags_a & 3) as u8;
        let char = (self.objects[i].char2 / 2) as usize;
        let roll = (self.rng.next() & 7) as u8;
        let idx = bucket * 24 + state * 8 + roll;
        let table = assets.ait[char]
            .section(3)
            .map(|s| s.to_vec())
            .unwrap_or_default();
        let action = table.get(idx as usize).copied().unwrap_or(0x47);
        // Guard against repeats (0x105FE).
        if (0x34..=0x37).contains(&action) && self.players[i].b18 == action {
            // Re-roll once (the engine loops; a single re-roll is enough here).
            let roll = (self.rng.next() & 7) as u8;
            let idx = bucket * 24 + state * 8 + roll;
            let action = table.get(idx as usize).copied().unwrap_or(0x47);
            self.players[i].b18 = action;
            return action;
        }
        self.players[i].b18 = action;
        action
    }

    /// `0x179B7` — action start with all the filters.
    fn start_action(&mut self, assets: &FightAssets, i: usize, opp: usize, mut action: u8) {
        // Jump aliases.
        if action == 0x38 {
            action = 0x43;
        }
        if action == 0x39 {
            action = 0x44;
        }
        let obj_flags38 = self.objects[i].flags38;
        let current = self.objects[i].action;
        // Animation lock: frm flags 0x04 set and 0x08 clear.
        if obj_flags38 & 0x04 != 0 && obj_flags38 & 0x08 == 0 {
            return;
        }
        if self.players[i].b1f == 0 {
            self.update_facing(i, opp);
        }
        // Throws bypass the filters.
        if matches!(action, 0x3E | 0x3F | 0x40 | 0x41) {
            self.commit_named_action(assets, i, opp, action);
            return;
        }
        // No actions during a jump.
        if matches!(current, 0x42..=0x44)
            && self.objects[i].flags38 & 0x10 == 0
            && !(0x18..=0x2B).contains(&action)
        {
            return;
        }
        if matches!(action, 0x42..=0x44) && action == current {
            self.objects[i].b3b = 0xFF;
        }
        // Grabs: 0x28 needs specific opponent state and close range.
        if action == 0x28 {
            let dx = (self.players[i].x as i32 - self.players[opp].x as i32).abs();
            let dy = (self.players[i].y as i32 - self.players[opp].y as i32).abs();
            let ok = self.players[opp].flags_a & 3 == 2
                && !(0x34..=0x37).contains(&self.objects[opp].action)
                && dx < 0x50
                && dy < 0x40;
            if !ok {
                self.reject_to_idle(i);
                return;
            }
        }
        // Throws 0x2C..0x33.
        if (0x2C..=0x33).contains(&action) {
            let dx = (self.players[i].x as i32 - self.players[opp].x as i32).abs();
            let dy = (self.players[i].y as i32 - self.players[opp].y as i32).abs();
            let char = (self.objects[i].char2 / 2) as usize;
            let allowed = tables::THROW_ALLOW
                .get(char * 4 + (action as usize - 0x2C))
                .copied()
                .unwrap_or(0)
                == 1;
            let ok = self.players[opp].flags_a & 0x400 == 0
                && self.players[opp].flags_a & 3 != 2
                && dx < 0x50
                && dy < 0x40
                && allowed;
            if !ok {
                self.reject_to_idle(i);
                return;
            }
        }
        self.commit_named_action(assets, i, opp, action);
    }

    /// `0x108C6` — fall back to stand/idle (script 0x0D).
    fn reject_to_idle(&mut self, i: usize) {
        self.force_script(i, 0x0D);
    }

    /// The shared commit tail `0x17C4C..0x17C94`.
    fn commit_named_action(
        &mut self,
        assets: &FightAssets,
        i: usize,
        _opp: usize,
        action: u8,
    ) {
        let char = (self.objects[i].char2 / 2) as usize;
        // Action -> script via the sparse ait table (0x17BE1), with the
        // throw special case 0x3E/0x3F -> scripts 0x0A/0x0B.
        let (script, param) = if matches!(action, 0x3E | 0x3F) {
            (0x0A + (action - 0x3E), 0)
        } else {
            assets.ait[char]
                .script_for_action(action)
                .unwrap_or((0x0D, 0))
        };
        if matches!(action, 0x40 | 0x41) {
            self.players[i].b1a = 0;
            self.players[i].b1b = (self.rng.next() & 7) as u8 + 2;
        }
        self.objects[i].b33 = 0xFF;
        if action != ACTION_IDLE && action < 0x40 {
            if action == 0x28 {
                self.objects[i].flags38 &= 0x7F;
            }
            self.players[i].b20 = assets.ait[char]
                .lockout_table()
                .and_then(|t| t.get(action as usize))
                .copied()
                .unwrap_or(0);
            self.players[i].flags_a &= !0x0080;
            // A new action re-arms the connect latch.
            self.players[i].flags_a &= !(hit::CONNECT_800 | hit::CONNECT_1000);
            self.players[i].ai_timer = 0;
        }
        self.commit_action(i, action, script, param);
        chars::clear_latches(char, action, &mut self.latches);
        let mut sounds = std::mem::take(&mut self.sound_events);
        chars::start_hook(char, action, &mut sounds);
        self.sound_events = sounds;
    }

    /// Store action + script and let the frm header flags take effect
    /// (`0x17C84`..`0x17C94`).
    fn commit_action(&mut self, i: usize, action: u8, script: u8, param: u8) {
        let obj = &mut self.objects[i];
        obj.action = action;
        obj.sub_timer = script.wrapping_mul(4); // script id * 4 (header offset)
        obj.sub_reload = param;
    }

    /// `0x176FC` — script commit + tick + movement.
    fn script_commit(&mut self, assets: &FightAssets, i: usize, opp: usize) {
        // 0x176FC: CH = obj[0x39] (script*4), CL = obj[0x3A] (param).
        let script = self.objects[i].sub_timer;
        let param = self.objects[i].sub_reload;
        self.commit_script(assets, i, opp, script, param);
        self.objects[i].b3b = script;
        self.objects[i].b3c = param;
        // 0x17715: bonus-stage X fix-up skipped outside bonus levels.
        self.objects[i].b3b = script;
        self.objects[i].b3c = param;
    }

    /// `0x10C5D` → `0x10D45` — commit the pending script and tick the action.
    fn commit_script(&mut self, assets: &FightAssets, i: usize, opp: usize, script: u8, _param: u8) {
        let obj = &mut self.objects[i];
        if script == obj.b3b {
            // Same script: no re-init, just tick.
            self.tick_action(assets, i, opp);
            return;
        }
        obj.b3b = script;
        // The animation lock (`0x10C7E`) is skipped for the walk actions: the
        // engine starts walks through `0x110CA` (no script commit), so a walk
        // never locks out the attack that follows it.
        if !matches!(obj.action, 0x40 | 0x41) {
            obj.flags38 = (obj.flags38 | 0x04) & !0x08 & !0x10;
        }
        // A new action that is neither a walk nor a jump stops the movement
        // the previous action had: only those arms set the active path bit
        // (`0x110CA` / `0x10CC9`) and the per-frame speed, and the engine runs
        // attacks through the idle path (observed: action 0x35 with flags38
        // 0x05, no marching while the fireball comes out).
        if !matches!(obj.action, 0x40..=0x44) {
            obj.flags38 &= !0x80;
            obj.x_speed = 0;
        }
        obj.hotspot = script as u16; // already script_id * 4 (header offset)
        obj.sub_action = 0;
        let script_id = (script / 4) as usize;
        let char = (obj.char2 / 2) as usize;
        let hdr = assets.frm[char].script(script_id).copied();
        let p = &mut self.players[i];
        p.flags_a &= !0x0020;
        p.flags_b &= !0x0100;
        if script < 0x0C {
            p.flags_b |= 0x0100;
        }
        if let Some(hdr) = hdr {
            if hdr.flags & 0x20 != 0 {
                p.flags_a |= 0x0020;
            }
            if hdr.flags & 0x80 != 0 {
                obj.flags38 |= 0x08;
            }
        }
        {
            let mut sounds = std::mem::take(&mut self.sound_events);
            chars::script_hook((obj.char2 / 2) as usize, obj.action, &mut sounds);
            self.sound_events = sounds;
        }
        match obj.action {
            // `0x110CA` - start walk (0x40 = walk left, 0x41 = walk right, the
            // actions the per-input table picks for the left/right classes).
            // The walk takes the active-action path; its speed is the driver
            // default and follows the facing (the engine turns the fighter
            // before the walk starts). The engine's routine also clears the
            // frame accumulator and the speed scale.
            0x40 | 0x41 => {
                // The walk frames carry the movement (dx 4/frame, negated when
                // facing left); `0x110CA` contributes the active-path bit.
                obj.flags38 |= 0x80;
                obj.x_speed = 0;
                obj.sub_action = 0;
                obj.pos_acc = 0;
                obj.speed_scale = 0;
            }
            0x42..=0x44 => {
                obj.flags38 |= 0x80;
                let char = (obj.char2 / 2) as usize;
                // `0x10CE2`: the jump actions encode fixed directions - 0x43 is
                // always -x, 0x44 is always +x (0x42 is the neutral jump), not
                // facing-relative: the input table picks 0x43/0x44 by the
                // up-left / up-right classes.
                let sp = tables::JUMP_SPEED[char] as i16;
                obj.x_speed = match obj.action {
                    0x42 => 0,
                    0x43 => -sp,
                    _ => sp,
                };
                obj.sub_action = if obj.action == 0x42 { 0 } else { 0x1C };
                obj.mode = if obj.action == 0x42 { 4 } else { 0xFC };
                obj.pos_acc = 0;
                obj.speed_scale = (obj.x_speed as i32 * 8) as u16;
            }
            0x18..=0x27 => {
                obj.sub_action = 0;
                obj.mode = 4;
            }
            _ => {
                obj.w2e = 0;
                obj.y_target = self.players[i].y;
            }
        }
        self.tick_action(assets, i, opp);
    }

    /// `0x10D45` + movement — the per-frame action tick.
    fn tick_action(&mut self, assets: &FightAssets, i: usize, opp: usize) {
        // Mode-4 hook (0x10D57): per-character tick; may spawn a fireball.
        let char = (self.objects[i].char2 / 2) as usize;
        let obj = self.objects[i];
        let facing_right = obj.facing & 4 != 0;
        let mut sounds = std::mem::take(&mut self.sound_events);
        let spawn = {
            let mut latches = self.latches;
            let result = chars::tick_hook(
                char,
                obj.action,
                obj.sub_action,
                obj.flags38,
                self.players[i].b1f,
                obj.frame,
                facing_right,
                obj.x,
                self.players[i].y,
                obj.palette,
                &mut latches,
                &mut sounds,
            );
            self.latches = latches;
            result
        };
        self.sound_events = sounds;
        if let Some(req) = spawn {
            self.spawn_projectile(assets, i, req);
        }
        // 0x10D5E: frm header flags low bits copy into both flag words.
        let char = (self.objects[i].char2 / 2) as usize;
        let script_id = (self.objects[i].hotspot / 4) as usize;
        let flags = assets.frm[char]
            .script(script_id)
            .map(|h| h.flags)
            .unwrap_or(0);
        self.objects[i].flags38 = (self.objects[i].flags38 & 0xFC) | (flags & 3);
        self.players[i].flags_a = (self.players[i].flags_a & 0xFFFC) | (flags & 3) as u16;

        if self.objects[i].flags38 & 0x80 != 0 {
            self.action_active_step(assets, i, opp);
        } else {
            self.idle_step(assets, i, opp);
        }
        // Common tail 0x11034: end-of-script handling, throw release, timers.
        self.hit_tail(assets, i, opp);
    }

    /// The movement/physics path for an active action (0x10D85..0x11034,
    /// condensed to the documented core).
    fn action_active_step(&mut self, assets: &FightAssets, i: usize, opp: usize) {
        // 0x10D85: advance the animation frame. The movement of an active
        // action is the frame's own `(dx, dy)` pair (`0x10AB1`): walk frames
        // step 4 px, dash frames 10 px, and the jump arc is literally the `dy`
        // sequence of its frames. The `dx` sign follows the facing (`0x10E4F`).
        let (_, (fdx, fdy)) = self.advance_frame(assets, i);
        // 0x10D8B: accumulator.
        self.objects[i].pos_acc = self.objects[i].pos_acc.wrapping_add(FRAME_ACC_STEP);
        let obj = &mut self.objects[i];
        if obj.mode != 0 && obj.speed_scale != 0 {
            // 0x10DA3: t = (scale + acc_high * 64) / (2 * scale), signed.
            let acc_high = ((obj.pos_acc as u32) >> 16) as i32;
            let scale = obj.speed_scale as i32;
            let t = (scale + (acc_high << 6)) / (2 * scale);
            let mut v = 4 - t;
            // cmp ax,7 / jbe keep else 7  (negative wraps to >7 unsigned).
            if !(0..=7).contains(&v) {
                v = 7;
            }
            if obj.mode == 4 {
                v = 7 - v;
            }
            obj.sub_action = ((v << 2) as u8) & 0xFC;
        }
        // 0x10DEA: step bounds with the frame's movement pair. The walk and
        // jump actions encode their direction in the action id (0x40/0x43 =
        // left, 0x41/0x44 = right, 0x42 = neutral - the same convention the
        // engine's own jump setup uses), and the frames supply the speed
        // profile (the walk stride steps 6/5/9/10/5 px etc.). Other actions
        // use the sprite-space sign with the facing flip.
        let facing_right = self.objects[i].facing & 4 != 0;
        let dir = match self.objects[i].action {
            0x40 => -1i16,
            0x41 => 1,
            0x42 => 0,
            0x43 => -1,
            0x44 => 1,
            _ => {
                if facing_right {
                    -1
                } else {
                    1
                }
            }
        };
        let sx = if matches!(self.objects[i].action, 0x40..=0x44) {
            fdx.abs() * dir
        } else if facing_right {
            -fdx
        } else {
            fdx
        };
        let (finished, cx, dy) = self.step_bounds(i, sx, fdy);
        if std::env::var_os("RF_DEBUG_MOVE").is_some() {
            eprintln!(
                "move: action={:#04x} hotspot={} sub={:#04x} frame={} fdx={fdx} fdy={fdy} facing={:#04x} sx={sx} cx={cx} flags_a={:#06x}",
                self.objects[i].action,
                self.objects[i].hotspot,
                self.objects[i].sub_action,
                self.objects[i].frame,
                self.objects[i].facing,
                self.players[i].flags_a,
            );
        }
        let (cx, dy) = if !finished {
            if self.players[i].flags_a & 0x1000 != 0 {
                (self.players[i].b0e as i16, dy) // pushback
            } else {
                (cx, dy)
            }
        } else {
            if self.objects[i].mode != 0 {
                self.objects[i].flags38 =
                    (self.objects[i].flags38 & 0xFC) | 1;
                self.players[i].flags_a = (self.players[i].flags_a & !3) | 1;
                self.objects[i].flags38 |= 0x10;
            } else {
                self.players[i].flags_b |= 0x0100;
            }
            self.objects[i].w2e = 0;
            let dy = self.objects[i].y_target - self.players[i].y;
            (cx, dy)
        };
        if self.players[i].flags_a & 0x0800 != 0 {
            let cx = -cx;
            self.players[i].flags_a &= !0x0800;
            self.finish_move(i, opp, cx, dy);
        } else {
            self.finish_move(i, opp, cx, dy);
        }
    }

    /// The idle/follow path (`0x10E4F`..) — frame advance without physics.
    fn idle_step(&mut self, assets: &FightAssets, i: usize, opp: usize) {
        // 0x10F31: movement lockout decrement (0x10F3F).
        if self.players[i].b1f != 0 {
            // decrement happens in the frame advance below
        }
        let (_advanced, _) = self.advance_frame(assets, i);
        if self.players[i].b1f != 0 {
            self.players[i].b1f -= 1;
        }
        let _ = opp;
    }

    /// 0x1102C/0x1102F: wall clamp then apply the movement.
    fn finish_move(&mut self, i: usize, opp: usize, cx: i16, dy: i16) {
        let (cx, dy) = self.wall_clamp(i, opp, cx, dy);
        self.players[i].x = self.players[i].x.wrapping_add(cx);
        self.players[i].y = self.players[i].y.wrapping_add(dy);
    }

    /// `0x11034` tail: script-end unlock, opponent throw release, and the
    /// mode-6 / AI-timer refresh (`0x11080`..`0x110C9`).
    fn hit_tail(&mut self, assets: &FightAssets, i: usize, opp: usize) {
        if self.objects[i].flags38 & 0x10 == 0 || self.objects[i].w2e != 0 {
            return;
        }
        self.objects[i].flags38 &= !0x04;
        if self.players[i].flags_b & 0x0100 != 0 {
            self.players[i].b1a = 0;
            self.players[i].b1b = 0;
        }
        // 0x1105B: opponent grab/throw release.
        if self.players[opp].flags_b & 0x0080 != 0 {
            if self.players[opp].flags_b & 0x0400 != 0 {
                let char = (self.objects[i].char2 / 2) as usize;
                let table = assets.ait[char]
                    .power_table()
                    .map(|t| t.to_vec())
                    .unwrap_or_default();
                let reaction = self.players[opp].reaction_anim;
                let _ = hit::apply_reaction_damage(
                    &mut self.players[opp],
                    self.objects[opp].char2,
                    reaction,
                    &table,
                    None,
                    false,
                );
            }
            self.players[opp].flags_b &= !0x0400;
            self.players[opp].flags_b &= !0x0004;
            if self.players[opp].flags_b & 0x0020 != 0 {
                self.force_script(opp, 0x0D);
            }
        }
        // 0x110AD: hit-stop adds into the AI timer, else refresh from the table.
        if self.players[i].b21 != 0 {
            self.players[i].ai_timer =
                self.players[i].ai_timer.saturating_add(self.players[i].b21);
            self.players[i].b21 = 0;
        } else if self.objects[i].action != 0x3A {
            let char = (self.objects[i].char2 / 2) as usize;
            let action = self.objects[i].action as usize;
            let scaled = assets.ait[char]
                .power_table()
                .and_then(|t| t.get(action))
                .copied()
                .unwrap_or(0)
                / 6;
            self.players[i].ai_timer = scaled;
        }
    }

    /// Spawn a fireball (`0x169F8`..`0x16A71`).
    fn spawn_projectile(&mut self, assets: &FightAssets, owner: usize, req: chars::SpawnRequest) {
        if self.projectiles.len() >= 8 {
            return;
        }
        let set = match req.kind {
            chars::ProjKind::Fireball => &assets.fireball,
            chars::ProjKind::Sonic => &assets.sonic,
            chars::ProjKind::TigerShot => &assets.tiger,
        };
        let (w, h) = set.first().map(|f| (f.width, f.height)).unwrap_or((0, 0));
        self.objects[owner].flags38 |= 0x40;
        self.projectiles.push(Projectile {
            x: req.x,
            y: req.y,
            x_speed: req.x_speed,
            action: req.action,
            owner,
            flicker: 0,
            width: w,
            height: h,
            kind: req.kind,
            anim: 0,
        });
        // 0x16A71: movement lockout from the frame-rate scalar.
        self.players[owner].b1f = 4 * 4;
    }

    /// `0x16A87` — projectile tick: move, bounds-check, flicker.
    fn tick_projectiles(&mut self) {
        for p in &mut self.projectiles {
            p.x = p.x.wrapping_add(p.x_speed);
            p.flicker ^= 2;
            p.anim += 1;
        }
        self.projectiles.retain(|p| p.x > -0x30 && p.x < 0x150);
    }

    /// `0x112B0` — step bounds. Returns `(finished, cx, dy)`.
    ///
    /// General levels: compute the new Y; above 200 it clamps to the ground
    /// and reports "finished". Bonus levels 0x18/0x1C keep their constants.
    fn step_bounds(&mut self, i: usize, cx: i16, dy: i16) -> (bool, i16, i16) {
        let _new_x = self.players[i].x.wrapping_add(cx);
        let new_y = self.players[i].y.wrapping_add(dy);
        if self.level == 0x18 {
            let action = self.objects[i].action;
            if !(0x34..=0x37).contains(&action)
                && self.objects[i].w28() == 0
            {
                if new_y > 0x98 {
                    self.objects[i].y_target = 0x98;
                    return (true, cx, 0x98 - self.players[i].y);
                }
            }
        }
        if new_y > 0xC8 {
            self.objects[i].y_target = 0xC8;
            return (true, cx, 0xC8 - self.players[i].y);
        }
        (false, cx, dy)
    }

    /// `0x11172` — wall clamp + camera follow, condensed: side walls at
    /// 0x30/0x110 in screen space, camera bounded by the stage, opponent-wall
    /// pinning, and the wall flags updated the way `0x17620` does.
    fn wall_clamp(&mut self, i: usize, opp: usize, cx: i16, dy: i16) -> (i16, i16) {
        let px = self.players[i].x as i32;
        let mut cam = self.camera_x as i32;
        let mut cx32 = cx as i32;

        let mut nx = px + cx32 - cam;
        if nx < 0x30 {
            if self.players[opp].flags_a & flag_a::LEFT_WALL != 0 {
                cx32 = 0x30 - (px - cam);
                self.players[i].flags_a |= flag_a::LEFT_WALL;
            } else {
                let want = (cam + (0x30 - nx)).clamp(0, self.stage_max_pan);
                cam = want;
                nx = px + cx32 - cam;
                if nx < 0x30 {
                    cx32 = 0x30 - (px - cam);
                    self.players[i].flags_a |= flag_a::LEFT_WALL;
                } else {
                    self.players[i].flags_a &= !flag_a::LEFT_WALL;
                }
            }
        } else if nx > 0x110 {
            if self.players[opp].flags_a & flag_a::RIGHT_WALL != 0 {
                cx32 = 0x110 - (px - cam);
                self.players[i].flags_a |= flag_a::RIGHT_WALL;
            } else {
                let want = (cam + (nx - 0x110)).clamp(0, self.stage_max_pan);
                cam = want;
                nx = px + cx32 - cam;
                if nx > 0x110 {
                    cx32 = 0x110 - (px - cam);
                    self.players[i].flags_a |= flag_a::RIGHT_WALL;
                } else {
                    self.players[i].flags_a &= !flag_a::RIGHT_WALL;
                }
            }
        } else {
            self.players[i].flags_a &= !(flag_a::LEFT_WALL | flag_a::RIGHT_WALL);
        }
        self.camera_x = cam as i16;
        self.camera_pending = cam as i16;
        self.camera_dirty = true;
        (cx32.clamp(i16::MIN as i32, i16::MAX as i32) as i16, dy)
    }


    /// `0x10AB1` — advance one frame in the script's list; returns
    /// `(advanced, (dx, dy))`.
    fn advance_frame(&mut self, assets: &FightAssets, i: usize) -> (bool, (i16, i16)) {
        let char = (self.objects[i].char2 / 2) as usize;
        let script_id = (self.objects[i].hotspot / 4) as usize;
        let Some(script) = assets.frm[char].script(script_id) else {
            return (false, (0, 0));
        };
        let list_base = script.list_offset as usize;
        let slot = self.objects[i].sub_action as usize;
        let mut next = list_base + slot + 4;
        let data = assets.frm[char].frame_list(script_id).0;
        if slot / 4 >= data.len() {
            // End of list: apply header flags.
            return self.apply_end_flags(assets, i, script_id);
        }
        let entry = data[slot / 4];
        let _ = next;
        next = list_base + slot + 4;
        let _ = next;
        self.objects[i].sub_action = (slot + 4) as u8;
        self.objects[i].frame = entry.ani_index;
        if entry.flags & 0x04 != 0 {
            self.objects[i].flags ^= 0x0004;
        }
        if entry.flags & 0x08 != 0 {
            self.objects[i].flags |= 0x0008;
        }
        (true, (entry.dx as i16, entry.dy as i16))
    }

    fn apply_end_flags(
        &mut self,
        assets: &FightAssets,
        i: usize,
        script_id: usize,
    ) -> (bool, (i16, i16)) {
        let char = (self.objects[i].char2 / 2) as usize;
        let flags = assets.frm[char]
            .script(script_id)
            .map(|h| h.flags)
            .unwrap_or(0);
        if flags & 0x04 != 0 {
            self.objects[i].sub_action = 0;
            self.objects[i].flags38 |= 0x10;
        } else if flags & 0x08 != 0 {
            self.objects[i].sub_action = 0;
        } else if flags & 0x10 != 0 {
            // keep the last frame: slot steps back one entry
            self.objects[i].sub_action = self.objects[i].sub_action.saturating_sub(4);
            self.objects[i].flags38 |= 0x10;
        }
        (false, (0, 0))
    }

    /// `0x10B7F` — read the current frame without advancing; returns
    /// `(ani_index, header_flags)`.
    fn read_frame(&mut self, assets: &FightAssets, i: usize) -> (u8, u8) {
        let char = (self.objects[i].char2 / 2) as usize;
        let script_id = (self.objects[i].hotspot / 4) as usize;
        let Some(script) = assets.frm[char].script(script_id) else {
            return (self.objects[i].frame, 0);
        };
        let header_flags = script.flags;
        if header_flags & 0x20 != 0 {
            self.players[i].flags_a |= 0x0020;
        }
        if header_flags & 0x80 != 0 {
            self.objects[i].flags38 |= 0x08;
        }
        let frames = assets.frm[char].frame_list(script_id).0;
        let slot = self.objects[i].sub_action as usize / 4;
        let ani = frames.get(slot).map(|f| f.ani_index).unwrap_or(0x7F);
        self.objects[i].frame = ani;
        (ani, header_flags)
    }

    /// Start an action directly (used by the debug keys while the input
    /// recogniser is not ported).
    pub fn force_action(&mut self, assets: &FightAssets, i: usize, action: u8) {
        let opp = 1 - i;
        self.start_action(assets, i, opp, action);
    }


    /// `0x12131` — hitbox build + resolution (runs after the entity pass).
    pub fn hit_pass(&mut self, assets: &FightAssets) {
        let boxes = [self.build_boxes(assets, 0), self.build_boxes(assets, 1)];
        for atk in 0..2 {
            let def = 1 - atk;
            if self.players[atk].flags_a & hit::ATTACK_ARMED == 0 {
                continue;
            }
            // The connect latch (`0x12280`): once this attack has landed it
            // cannot land again; the bit lives in the attacker's `flags_a`
            // (`0x0800` / the wall variant `0x1000`) and is cleared when the
            // next action starts. (The old check tested the *draw object's*
            // flags, which are never set - the attack then re-hit every
            // frame and a single punch could melt a full health bar.)
            if self.players[atk].flags_a & (hit::CONNECT_800 | hit::CONNECT_1000) != 0 {
                continue;
            }
            if self.players[def].flags_b & 0x0004 != 0 {
                continue;
            }
            if self.players[atk].flags_a & hit::WAS_HIT != 0 {
                continue;
            }
            for (idx, require_kind15) in [(1usize, false), (3usize, true)] {
                let Some(ab) = boxes[atk][idx] else { continue };
                if require_kind15 && ab.kind != 15 {
                    continue;
                }
                if !hit::overlap(&ab, &boxes[def], 5) {
                    continue;
                }
                let ctx = hit::BlockContext {
                    attacker_char: (self.objects[atk].char2 / 2) as usize,
                    attacker_action: self.objects[atk].action,
                    attacker_flags_a: self.players[atk].flags_a,
                    defender_action: self.objects[def].action,
                    defender_sub_action: self.objects[def].sub_action,
                };
                match hit::block_rule(ctx) {
                    hit::BlockVerdict::Reject => continue,
                    hit::BlockVerdict::Allow => {
                        let (a_p, d_p) = index_pair(&mut self.players, atk, def);
                        let (a_o, d_o) = index_pair(&mut self.objects, atk, def);
                        let mut att = hit::FighterPair { player: a_p, obj: a_o };
                        let mut deff = hit::FighterPair { player: d_p, obj: d_o };
                        let _ = hit::apply_hit(&mut att, &mut deff, false);
                    }
                    hit::BlockVerdict::SwapRoles => {
                        let (a_p, d_p) = index_pair(&mut self.players, atk, def);
                        let (a_o, d_o) = index_pair(&mut self.objects, atk, def);
                        let mut att = hit::FighterPair { player: a_p, obj: a_o };
                        let mut deff = hit::FighterPair { player: d_p, obj: d_o };
                        let _ = hit::apply_hit(&mut deff, &mut att, true);
                    }
                }
                break;
            }
        }
        // Projectile hits (`0x123EA`/`0x12442`).
        for idx in 0..self.projectiles.len() {
            let proj = self.projectiles[idx];
            let def = 1 - proj.owner;
            if self.players[def].flags_b & 0x0004 != 0 {
                continue;
            }
            let left = proj.x.wrapping_sub(proj.width as i16 / 2);
            let top = proj.y.wrapping_sub(proj.height as i16 / 2);
            let pbox = crate::fight::ani::HitBox {
                kind: 1,
                left,
                top,
                right: left.wrapping_add(proj.width as i16),
                bottom: top.wrapping_add(proj.height as i16),
            };
            if hit::overlap(&pbox, &boxes[def], 5) {
                self.players[def].flags_a |= hit::WAS_HIT | 0x4000;
                self.players[def].reaction_anim = proj.action;
                self.projectiles[idx].owner = usize::MAX; // mark for removal
                break;
            }
        }
        self.projectiles.retain(|p| p.owner != usize::MAX);
    }

    fn build_boxes(
        &self,
        assets: &FightAssets,
        i: usize,
    ) -> [Option<crate::fight::ani::HitBox>; 4] {
        let char = (self.objects[i].char2 / 2) as usize;
        let frame_index = self.objects[i].frame as usize;
        let Some(frame) = assets.ani[char].frame(frame_index) else {
            return [None; 4];
        };
        // The mirror selector is the draw object's bit 2 (`facing & 4`).
        let flipped = self.objects[i].facing & 4 != 0;
        crate::fight::ani::build_hitboxes(frame, self.players[i].x, self.players[i].y, flipped)
    }

    /// `0x11317` — facing update based on X positions.
    fn update_facing(&mut self, i: usize, opp: usize) {
        let mine = self.players[i].x;
        let theirs = self.players[opp].x;
        let opp_facing = self.objects[opp].facing;
        let obj = &mut self.objects[i];
        obj.facing &= !0x04;
        if mine == theirs {
            // opposite of the opponent's facing
            obj.facing |= (!opp_facing) & 0x04;
        } else if mine < theirs {
            obj.facing |= 0x04;
        }
    }
}

/// Borrow two different indices of a 2-element array mutably.
fn index_pair<T>(arr: &mut [T; 2], a: usize, b: usize) -> (&mut T, &mut T) {
    if a < b {
        let (left, right) = arr.split_at_mut(b);
        (&mut left[a], &mut right[0])
    } else {
        let (left, right) = arr.split_at_mut(a);
        (&mut right[0], &mut left[b])
    }
}
