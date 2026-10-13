//! Fighter state — port of the `0x23`-byte fighter struct (`DS:0x3BA9` P1 /
//! `DS:0x3BCC` P2) and the associated draw object (`DS:0x1B5A` pool).
//!
//! Constructor: `FUN_1000_7416` / `FUN_1000_746E` (`0x17416`/`0x1746E`);
//! wall clamp: `0x17620`; idle reset: `FUN_1000_11114` (`0x11114`);
//! move start: `FUN_1000_110CA` (`0x110CA`).

/// `flags_a` (`+0x0A`) bits, observed in `0x1746E`, `0x17620`, `0x121A0`, `0x11080`.
pub mod flag_a {
    /// Set at spawn (with [`SPAWN`]).
    pub const INIT: u16 = 0x0001;
    /// Set at spawn.
    pub const SPAWN: u16 = 0x0200;
    /// Computer-controlled.
    pub const CPU: u16 = 0x0400;
    /// At left wall.
    pub const LEFT_WALL: u16 = 0x0008;
    /// At right wall.
    pub const RIGHT_WALL: u16 = 0x0010;
    /// Attack active.
    pub const ATTACK: u16 = 0x0020;
    /// Hit connected.
    pub const CONNECTED: u16 = 0x0080;
    /// Was hit.
    pub const WAS_HIT: u16 = 0x0100;
}

/// `flags_b` (`+0x0C`) bits.
pub mod flag_b {
    /// At left wall.
    pub const LEFT_WALL: u16 = 0x0001;
    /// At right wall.
    pub const RIGHT_WALL: u16 = 0x0002;
    /// Invulnerable (early-out in `0x121A0`).
    pub const INVULNERABLE: u16 = 0x0004;
}

/// `DrawObject.flags` (`+0x1C`) bits.
pub mod obj_flag {
    /// Visible.
    pub const VISIBLE: u16 = 0x0001;
    /// Slot in use.
    pub const IN_USE: u16 = 0x0002;
    /// Linked to parent (adds parent-relative offsets).
    pub const LINKED: u16 = 0x0010;
    /// Hidden/immune.
    pub const HIDDEN: u16 = 0x0800;
    /// Active in the physics pass.
    pub const PHYSICS: u16 = 0x1000;
}

/// The compact per-fighter struct (stride `0x23`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fighter {
    /// `+0x00` X position (object space).
    pub x: i16,
    /// `+0x02` Y position (200 at spawn).
    pub y: i16,
    /// `+0x04` index of this fighter's main draw object (engine stores a pointer).
    pub draw_object: u16,
    /// `+0x06` unknown / unused.
    pub unused06: u16,
    /// `+0x0A` flags A (see [`flag_a`]).
    pub flags_a: u16,
    /// `+0x0C` flags B (see [`flag_b`]).
    pub flags_b: u16,
    /// `+0x0B` set to 100 at spawn (role unknown).
    pub b0b: u8,
    /// `+0x0D` set to 2 at spawn.
    pub b0d: u8,
    /// `+0x0E` set to `0x18` when a connect is made against a wall-flagged
    /// defender (`0x1228D`).
    pub b0e: u8,
    /// `+0x14` **energy** (100 at spawn).
    pub energy: u8,
    /// `+0x15` 0 at spawn.
    pub b15: u8,
    /// `+0x16` 100 at spawn.
    pub b16: u8,
    /// `+0x17` raw input byte / last press (written by `0x17967`).
    pub b17: u8,
    /// `+0x18` `0xFF` at spawn.
    pub b18: u8,
    /// `+0x19` 0 at spawn.
    pub b19: u8,
    /// `+0x1A` AI decision delay in frames (2 at spawn).
    pub b1a: u8,
    /// `+0x1B` 0 at spawn.
    pub b1b: u8,
    /// `+0x1E` reaction animation id — the action id that last hit this
    /// fighter (written by `0x122A0`, consumed by the damage routine).
    pub reaction_anim: u8,
    /// `+0x1F` 0 at spawn; movement lockout (frames).
    pub b1f: u8,
    /// `+0x20` hit-connected timer (decrements at `0x17830`).
    pub b20: u8,
    /// `+0x21` pending AI timer delta.
    pub b21: u8,
    /// `+0x22` AI action timer (`10 * action / 6`).
    pub ai_timer: u8,
}

impl Fighter {
    /// Construct a fighter the way `0x1746E` does.
    pub fn spawn(x: i16, cpu: bool) -> Self {
        Fighter {
            x,
            y: 200,
            draw_object: 0,
            unused06: 0,
            flags_a: flag_a::INIT | flag_a::SPAWN | if cpu { flag_a::CPU } else { 0 },
            flags_b: 0,
            b0b: 100,
            b0d: 2,
            b0e: 0,
            energy: 100,
            b15: 0,
            b16: 100,
            b17: 0,
            b18: 0xFF,
            b19: 0,
            b1a: 2,
            b1b: 0,
            reaction_anim: 0,
            b1f: 0,
            b20: 0,
            b21: 0,
            ai_timer: 0,
        }
    }
}

/// The 0x48-byte draw object (animation/render entity).
///
/// Only the fields observed in code are modelled; unknown gaps are kept as
/// explicit placeholders so the layout stays traceable to `docs/port/fight-engine.md` §4.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DrawObject {
    /// `+0x00` X (world).
    pub x: i16,
    /// `+0x02` Y.
    pub y: i16,
    /// `+0x04` current sprite handle (asset handle; `0xF33A` = blank sprite).
    pub sprite: u16,
    /// `+0x06` linked driver object (slot index).
    pub parent: u16,
    /// `+0x08` parent-relative X offset (added when [`obj_flag::LINKED`]).
    pub rel_x: i16,
    /// `+0x0A` parent-relative Y offset.
    pub rel_y: i16,
    /// `+0x0C` caller payload (emulated as a small integer tag).
    pub payload: u16,
    /// `+0x0E` sprite work-buffer segment.
    pub buffer_segment: u16,
    /// `+0x10` work-buffer size.
    pub buffer_size: u16,
    /// `+0x12..+0x18` saved draw box x/y/w/h.
    pub draw_box: [u16; 4],
    /// `+0x1A` palette/base offset (P1 `0x3030`, P2 `0x4040`).
    pub palette: u16,
    /// `+0x1C` flags (see [`obj_flag`]).
    pub flags: u16,
    /// `+0x1E` facing/state byte — bit 2 set for player 1 (mirrors hitboxes).
    pub facing: u8,
    /// `+0x1F` shape/state byte copied from driver (`0x20` P1 / `0x28` P2).
    pub shape: u8,
    /// `+0x20` index into `frm<CHR>` data (hotspot).
    pub hotspot: u16,
    /// `+0x22` AI timer.
    pub ai_timer: u8,
    /// `+0x24` X speed (signed).
    pub x_speed: i16,
    /// `+0x26`/`+0x28` 32-bit fixed-point position accumulator.
    pub pos_acc: i32,
    /// `+0x2C` Y source/target.
    pub y_target: i16,
    /// `+0x2E` role unknown.
    pub w2e: u16,
    /// `+0x30` speed scale (accumulator divisor).
    pub speed_scale: u16,
    /// `+0x32` sub-action / frame slot (added to the `frm` index).
    pub sub_action: u8,
    /// `+0x33` `0xFF` at spawn.
    pub b33: u8,
    /// `+0x34` mode (`0xFC` = speed mode; 0/4 in frame advance).
    pub mode: u8,
    /// `+0x35` **animation frame index** (`ani[frame * 42]`), 125 at spawn.
    pub frame: u8,
    /// `+0x36` player number (0/1).
    pub player: u8,
    /// `+0x37` character = `2 * char id`.
    pub char2: u8,
    /// `+0x38` flags: `0x80` action active, `0x40` projectile spawned.
    pub flags38: u8,
    /// `+0x39` sub-action timer.
    pub sub_timer: u8,
    /// `+0x3A` sub-action reload.
    pub sub_reload: u8,
    /// `+0x3B` `0xFF` at spawn.
    pub b3b: u8,
    /// `+0x3C` `0xFF` at spawn.
    pub b3c: u8,
    /// `+0x3D` **action id** (`0x47` at spawn, `0x3A` = stand/idle).
    pub action: u8,
}

/// Fixed-point accumulator step per frame (`+= 0x38000`, `0x10D8B`).
pub const FRAME_ACC_STEP: i32 = 0x38000;
/// Position accumulator value written by the idle reset (`0xFFF0:0`).
pub const IDLE_POS_ACC: i32 = 0xFFF0_0000u32 as i32;
/// Idle action id (`0x3A`).
pub const ACTION_IDLE: u8 = 0x3A;

impl DrawObject {
    /// Construct a fighter's main draw object the way the constructor does.
    pub fn spawn(player: usize, char2: u8, x: i16, y: i16) -> Self {
        let p1 = player == 0;
        DrawObject {
            x,
            y,
            sprite: SPRITE_BLANK,
            parent: 0,
            rel_x: 0,
            rel_y: 0,
            payload: 0,
            buffer_segment: 0,
            buffer_size: 0,
            draw_box: [0; 4],
            palette: if p1 { 0x3030 } else { 0x4040 },
            flags: obj_flag::VISIBLE | obj_flag::IN_USE,
            facing: if p1 { 4 } else { 0 },
            shape: if p1 { 0x20 } else { 0x28 },
            hotspot: 0,
            ai_timer: 0,
            x_speed: 0,
            pos_acc: 0,
            y_target: 0,
            w2e: 0,
            speed_scale: 0,
            sub_action: 0,
            b33: 0xFF,
            mode: 0,
            frame: 125,
            player: player as u8,
            char2,
            flags38: 0,
            sub_timer: 0,
            sub_reload: 0,
            b3b: 0xFF,
            b3c: 0xFF,
            action: 0x47,
        }
    }

    /// Whether the action is marked active (`flags38 & 0x80`).
    pub fn action_active(&self) -> bool {
        self.flags38 & 0x80 != 0
    }

    /// Port of the idle reset `FUN_1000_11114` (`0x11114`).
    pub fn reset_to_idle(&mut self, facing_left: bool) {
        self.action = ACTION_IDLE;
        self.sub_timer = 0xF8;
        self.sub_reload = 0;
        self.flags38 |= 0x80;
        self.x_speed = if facing_left { 0x0C } else { -0x0C };
        self.sub_action = 0x1C;
        self.mode = 0xFC;
        self.pos_acc = IDLE_POS_ACC;
    }

    /// Port of the move start `FUN_1000_110CA` (`0x110CA`).
    pub fn start_move(&mut self, speed: i16, facing_left: bool) {
        let facing = if facing_left { -1 } else { 1 };
        self.flags38 |= 0x80;
        self.mode = 0;
        self.speed_scale = (8 * speed * facing) as u16;
    }

    /// Fixed-point frame advance (`0x10D8B`): accumulate and derive the
    /// sub-action slot (`0x10DA3`).
    pub fn advance_accumulator(&mut self) {
        self.pos_acc = self.pos_acc.wrapping_add(FRAME_ACC_STEP);
        if self.mode != 0 && self.speed_scale != 0 {
            let scaled = (self.pos_acc as u32 as u64 * 8) / self.speed_scale as u64;
            let clamped = scaled.min(7) as u8;
            self.sub_action = clamped << 2;
        }
    }
}

impl DrawObject {
    /// Low word of the fixed-point accumulator (`+0x26`).
    pub fn w28(&self) -> u16 {
        (self.pos_acc & 0xFFFF) as u16
    }
}

/// Sprite handle sentinel meaning "draw nothing" (`sprBLANKSPR`, same value as
/// in [`crate::fight::ani`] records).
pub const SPRITE_BLANK: u16 = crate::fight::ani::SPRITE_BLANK;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fighter_spawn_matches_constructor() {
        let f = Fighter::spawn(80, false);
        assert_eq!(f.y, 200);
        assert_eq!(f.energy, 100);
        assert_eq!(f.b1a, 2);
        assert_eq!(f.b0b, 100);
        assert_eq!(f.b0d, 2);
        assert_eq!(f.b16, 100);
        assert_eq!(f.b18, 0xFF);
        assert!(f.flags_a & flag_a::INIT != 0);
        assert!(f.flags_a & flag_a::SPAWN != 0);
        assert!(f.flags_a & flag_a::CPU == 0);
        let cpu = Fighter::spawn(0xD0, true);
        assert!(cpu.flags_a & flag_a::CPU != 0);
    }

    #[test]
    fn draw_object_spawn_and_idle_reset() {
        let mut obj = DrawObject::spawn(0, 0, 80, 200);
        assert_eq!(obj.action, 0x47);
        assert_eq!(obj.frame, 125);
        assert_eq!(obj.palette, 0x3030);
        assert_eq!(obj.facing & 4, 4); // P1 mirror bit
        assert_eq!(obj.b33, 0xFF);
        let p2 = DrawObject::spawn(1, 2, 240, 200);
        assert_eq!(p2.palette, 0x4040);
        assert_eq!(p2.facing & 4, 0);
        assert_eq!(p2.char2, 2);

        obj.reset_to_idle(false);
        assert_eq!(obj.action, ACTION_IDLE);
        assert_eq!(obj.sub_timer, 0xF8);
        assert_eq!(obj.sub_action, 0x1C);
        assert_eq!(obj.mode, 0xFC);
        assert_eq!(obj.pos_acc, IDLE_POS_ACC);
        assert!(obj.action_active());
        assert_eq!(obj.x_speed, -0x0C);
    }

    #[test]
    fn accumulator_derives_sub_action() {
        let mut obj = DrawObject::spawn(0, 0, 0, 0);
        obj.mode = 1;
        obj.speed_scale = 8;
        obj.pos_acc = 0;
        // 8 * 0x38000 / 8 = 0x38000 >> ... clamped to 0..7 << 2.
        obj.advance_accumulator();
        let after_first = obj.sub_action;
        assert!(after_first <= 0x1C, "sub_action {after_first:#x}");
        for _ in 0..8 {
            obj.advance_accumulator();
        }
        assert!(obj.sub_action >= after_first);
    }
}
