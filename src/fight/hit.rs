//! Hit detection & resolution — ports of `0x12131` (pipeline), `0x1281E`
//! (overlap), `0x121A0`/`0x12256` (resolution + block rules `CS:0x22B3`),
//! `0x1857C` (reaction damage) and `0x17803` (KO check).
//!
//! All values here are taken from disassembly of the shipped build (the rule
//! functions were hand-decoded because Ghidra never analysed that region).

use crate::fight::ani::HitBox;
use crate::fight::fighter::{flag_a, DrawObject, Fighter};

/// Bit in `flags_a` meaning "attack active" (arm).
pub const ATTACK_ARMED: u16 = 0x0020;
/// Attacker flag set on connect (normal).
pub const CONNECT_800: u16 = 0x0800;
/// Attacker flag set on connect (blocked-by-wall variant).
pub const CONNECT_1000: u16 = 0x1000;
/// Defender flag set when hit.
pub const WAS_HIT: u16 = 0x0100;
/// Defender flag set by the KO path.
pub const KO_FLAG: u16 = 0x2000;
/// Dizzy flag tested by the damage routine (`[di+0x0A] & 0x40`).
pub const DIZZY: u16 = 0x0040;

/// Verdict of the per-character block rule (`CS:0x22B3`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockVerdict {
    /// Hit accepted.
    Allow,
    /// Hit rejected (no damage, no reaction).
    Reject,
    /// Roles swap: the attacker becomes the victim (`xchg si, di`, Zangief).
    SwapRoles,
}

/// Inputs for [`block_rule`], mirroring the registers at the `CALL AX`.
///
/// Engine character order (from the `DS:0x60` handle table):
/// `0 KEN, 1 GUILE, 2 E-HONDA, 3 RYU, 4 ZANGIEF, 5 CHUN LI, 6 DHALSIM,
/// 7 BLANKA, 8 BALROG, 9 VEGA, 10 SAGAT, 11 M.BISON`.
#[derive(Debug, Clone, Copy)]
pub struct BlockContext {
    /// Attacker character index (0..=11).
    pub attacker_char: usize,
    /// Attacker action id (attacker draw object `+0x3D`).
    pub attacker_action: u8,
    /// Attacker `flags_a` (`[si+0x0A]`).
    pub attacker_flags_a: u16,
    /// Defender (opponent) action id (`[bx+0x3D]`).
    pub defender_action: u8,
    /// Defender (opponent) sub-action (`[bx+0x32]`, tested by the Vega rule).
    pub defender_sub_action: u8,
}

/// Per-character anti-hit rule — port of the 12-entry table at `CS:0x22B3`
/// (entries: 0x22CB allow, 0x22CF reject, 0x22E4/0x22F7/0x2302/0x2312 special).
pub fn block_rule(ctx: BlockContext) -> BlockVerdict {
    match ctx.attacker_char {
        4 => {
            // ZANGIEF: opponent attacking with 0x34 and our control bits set.
            if ctx.defender_action == 0x34 && ctx.attacker_flags_a & 3 != 0 {
                BlockVerdict::Reject
            } else {
                BlockVerdict::Allow
            }
        }
        5 => BlockVerdict::Reject, // CHUN LI: this path never lands
        7 => {
            // BLANKA: opponent in 0x34 -> roles swap.
            if ctx.defender_action == 0x34 {
                BlockVerdict::SwapRoles
            } else {
                BlockVerdict::Allow
            }
        }
        9 => {
            // VEGA: opponent's early 0x34 frames are immune.
            if ctx.defender_action == 0x34 && ctx.defender_sub_action < 0x3C {
                BlockVerdict::Reject
            } else {
                BlockVerdict::Allow
            }
        }
        11 => {
            // M. BISON: only counters while ourselves attacking 0x34..=0x37.
            if ctx.defender_action != 0x34 {
                return BlockVerdict::Allow;
            }
            if (0x34..=0x37).contains(&ctx.attacker_action) {
                BlockVerdict::Allow
            } else {
                BlockVerdict::Reject
            }
        }
        _ => BlockVerdict::Allow,
    }
}

/// AABB overlap used by `0x1281E`: attacker rectangle vs any defender box.
///
/// `mode` mirrors the `SI` argument (the engine passes 5 for the main check);
/// its exact additional semantics are not fully decoded, so only the geometric
/// test is applied.
pub fn overlap(attacker: &HitBox, defender: &[Option<HitBox>; 4], _mode: u8) -> bool {
    defender.iter().flatten().any(|d| {
        attacker.left < d.right
            && attacker.right > d.left
            && attacker.top < d.bottom
            && attacker.bottom > d.top
    })
}

/// One fighter as a coherent pair for the hit pipeline.
pub struct FighterPair<'a> {
    /// The compact fighter struct.
    pub player: &'a mut Fighter,
    /// The animation/render object.
    pub obj: &'a mut DrawObject,
}

/// Outcome of a successful hit application (`0x12272`..`0x122B2`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HitApplied {
    /// Whether the roles were swapped by the block rule.
    pub swapped: bool,
    /// Sound index requested (`DS:0x1638[action]`, `None` = 0xFF).
    pub sound: Option<u8>,
    /// A hit spark should be spawned.
    pub spark: bool,
}

/// Apply a hit: flag both players, set the reaction animation and pick the
/// sound (`0x12272`–`0x122B2`). Caller has already verified overlap and the
/// block rule.
pub fn apply_hit(
    attacker: &mut FighterPair<'_>,
    defender: &mut FighterPair<'_>,
    swapped: bool,
) -> HitApplied {
    let (att_player, att_obj) = (&mut *attacker.player, &mut *attacker.obj);
    let (def_player, _def_obj) = (&mut *defender.player, &mut *defender.obj);

    // 0x12280: 0x0800 normally, 0x1000 when the defender's wall flags are set.
    let mut connect = CONNECT_800;
    if def_player.flags_b & 0x0003 != 0 {
        connect = CONNECT_1000;
        att_player.b0e = 0x18; // [si+0x0E] = 0x18
    }
    att_player.flags_a |= connect | flag_a::CONNECTED;
    def_player.flags_a |= WAS_HIT;

    // Reaction animation id = the attacker's action.
    def_player.reaction_anim = att_obj.action;

    // Sound lookup DS:0x1638[action]; 0xFF = silent.
    let sound = crate::fight::SOUND_BY_ACTION
        .get(att_obj.action as usize)
        .copied()
        .filter(|&s| s != 0xFF);

    HitApplied {
        swapped,
        sound,
        spark: true,
    }
}

/// Result of [`apply_reaction_damage`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DamageOutcome {
    /// Damage actually subtracted (after all halving/clamping).
    pub damage: u8,
    /// Energy dropped below 25 (`[0x43B9] = 0xFF`).
    pub low_energy: bool,
    /// The defender is one of the three "combo reaction" characters.
    pub combo_reaction: bool,
}

/// Subtract `amount` from energy with clamping.
fn sub_energy(energy: u8, amount: u8) -> u8 {
    energy.saturating_sub(amount)
}

/// Reaction damage — port of `0x1857C`. Callers supply the reaction id via
/// `player.reaction_anim`.
///
/// * `char2` — the defender's `2 * char id` (from the draw object).
/// * `damage_table` — the character's `ait` byte table at `hdr[0]`, indexed by
///   the reaction id (asset handle table `DS:0x60`).
/// * `difficulty_scale` — `[bx+0x1C]` of the per-player damage-scale structure
///   when one is set; `None` selects the build's default `* 0xD9 >> 8` path.
/// * `dizzy` — `player.flags_a & 0x40`.
pub fn apply_reaction_damage(
    player: &mut Fighter,
    char2: u8,
    reaction_id: u8,
    damage_table: &[u8],
    difficulty_scale: Option<u8>,
    dizzy: bool,
) -> DamageOutcome {
    // 0x108D8: per-character damage lookup, indexed by the reaction id.
    let base = damage_table.get(reaction_id as usize).copied().unwrap_or(0);

    // 0x1859A: AI-timer refresh (damage / 5) | 1.
    player.b21 = (base / 5) | 1;

    // 0x185A8: damage * 2, then scale.
    let mut damage = (base as u32) * 2;
    let mut extra = 0u32;
    match difficulty_scale {
        Some(scale) => {
            // 0x185CC: CL = [bx+0x1C]; CH = CL*2; CL += CH  =>  3 * scale.
            extra = 3 * scale as u32;
        }
        None => {
            // 0x185C2: AH = 0xD9; MUL AH; AL = AH  =>  (* 217) >> 8.
            damage = (damage * 0xD9) >> 8;
        }
    }

    // 0x185DE: reaction special cases (char2 4/0xC/0xE = CHUN LI / BLANKA /
    // ZANGIEF).
    let combo_reaction = matches!(char2, 4 | 0xC | 0xE);
    if combo_reaction && reaction_id != 0x2C {
        extra >>= 1;
    }

    // 0x185FE: add extra, clamp to >= 1.
    damage += extra;
    if damage == 0 {
        damage = 1;
    }

    // 0x18607: dizzy plus attack-range reaction caps damage at 6.
    if dizzy && (0x34..=0x37).contains(&reaction_id) {
        damage = 6;
    }

    // 0x1861C: unconditional halving.
    damage >>= 1;

    // 0x18620: hit-counter accumulation (skipped when flags_b bit 4 is set).
    let mut counter_damage = damage;
    if player.flags_b & 0x0010 == 0 {
        if combo_reaction {
            counter_damage >>= 1;
        }
        player.b19 = player.b19.saturating_add(counter_damage as u8);
    }

    // 0x18635: energy subtraction with floor at zero.
    let before = player.energy;
    player.energy = sub_energy(player.energy, damage as u8);
    DamageOutcome {
        damage: before - player.energy,
        low_energy: player.energy < 0x19,
        combo_reaction,
    }
}

/// KO check — port of `0x17803`.
///
/// Returns true when the KO fired this call (energy == 0 and not already
/// flagged); the caller reacts by setting the round result.
pub fn check_ko(player: &mut Fighter) -> bool {
    if player.flags_a & KO_FLAG != 0 {
        return false;
    }
    if player.energy != 0 {
        return false;
    }
    player.flags_a |= KO_FLAG;
    // 0x1781A-0x17823: clear flags_b bits 2 (invulnerable), 7 and 5.
    player.flags_b &= !0x0004;
    player.flags_b &= !0x0080;
    player.flags_b &= !0x0020;
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fight::fighter::DrawObject;

    fn box_at(l: i16, t: i16, r: i16, b: i16) -> HitBox {
        HitBox {
            kind: 1,
            left: l,
            top: t,
            right: r,
            bottom: b,
        }
    }

    #[test]
    fn overlap_geometry() {
        let att = box_at(10, 10, 30, 30);
        let mut def = [None; 4];
        def[0] = Some(box_at(20, 20, 40, 40));
        assert!(overlap(&att, &def, 5));
        def[0] = Some(box_at(31, 31, 50, 50));
        assert!(!overlap(&att, &def, 5));
        def[0] = None;
        assert!(!overlap(&att, &def, 5));
    }

    #[test]
    fn block_rules_match_disassembly() {
        let base = BlockContext {
            attacker_char: 0,
            attacker_action: 0x34,
            attacker_sub_action: 0,
            attacker_flags_a: 0,
            defender_action: 0x34,
        };
        // Ryu-like: always allow.
        assert_eq!(block_rule(base), BlockVerdict::Allow);
        // Chun Li (4): reject with control bits set.
        let mut ctx = base;
        ctx.attacker_char = 4;
        assert_eq!(block_rule(ctx), BlockVerdict::Allow);
        ctx.attacker_flags_a = 1;
        assert_eq!(block_rule(ctx), BlockVerdict::Reject);
        // Dhalsim (5): always reject.
        let mut ctx = base;
        ctx.attacker_char = 5;
        assert_eq!(block_rule(ctx), BlockVerdict::Reject);
        // Zangief (7): swap on 0x34.
        let mut ctx = base;
        ctx.attacker_char = 7;
        assert_eq!(block_rule(ctx), BlockVerdict::SwapRoles);
        ctx.defender_action = 0x35;
        assert_eq!(block_rule(ctx), BlockVerdict::Allow);
        // Bison (9): reject while sub-action < 0x3C.
        let mut ctx = base;
        ctx.attacker_char = 9;
        ctx.attacker_sub_action = 0x3B;
        assert_eq!(block_rule(ctx), BlockVerdict::Reject);
        ctx.attacker_sub_action = 0x3C;
        assert_eq!(block_rule(ctx), BlockVerdict::Allow);
        // Vega (11): reject unless we are attacking 0x34..=0x37.
        let mut ctx = base;
        ctx.attacker_char = 11;
        ctx.attacker_action = 0x30;
        assert_eq!(block_rule(ctx), BlockVerdict::Reject);
        ctx.attacker_action = 0x35;
        assert_eq!(block_rule(ctx), BlockVerdict::Allow);
    }

    #[test]
    fn damage_pipeline_default_path() {
        let mut player = Fighter::spawn(0, false);
        // Table value 10 -> ((10*2*0xD9)>>8) = 16.95 -> 16; >>1 => 8.
        let table = [10u8; 64];
        let out = apply_reaction_damage(&mut player, 0, 4, &table, None, false);
        assert_eq!(out.damage, 8);
        assert_eq!(player.energy, 92);
        assert_eq!(player.b21, (10 / 5) | 1);
        assert!(!out.low_energy);
    }

    #[test]
    fn damage_pipeline_with_difficulty_and_combo_character() {
        let mut player = Fighter::spawn(0, false);
        let table = [20u8; 64];
        // Chun Li (char2 = 4): extra = 3*4 = 12, halved to 6 (reaction != 0x2C);
        // damage = 20*2 + 6 = 46; >>1 = 23.
        let out = apply_reaction_damage(&mut player, 4, 4, &table, Some(4), false);
        assert_eq!(out.damage, 23);
        assert!(out.combo_reaction);
        // Dizzy + attack range caps at 6 -> >>1 => 3.
        let mut player = Fighter::spawn(0, false);
        let out = apply_reaction_damage(&mut player, 0, 0x34, &table, Some(4), true);
        assert_eq!(out.damage, 3);
    }

    #[test]
    fn ko_check_and_flags() {
        let mut player = Fighter::spawn(0, false);
        player.energy = 0;
        player.flags_b = 0x00FF;
        assert!(check_ko(&mut player));
        assert!(player.flags_a & KO_FLAG != 0);
        assert_eq!(player.flags_b & 0x0004, 0);
        // Second call is a no-op.
        assert!(!check_ko(&mut player));
        // Non-zero energy never KOs.
        let mut player = Fighter::spawn(0, false);
        player.energy = 1;
        assert!(!check_ko(&mut player));
    }

    #[test]
    fn apply_hit_sets_flags_and_reaction() {
        let mut p1 = Fighter::spawn(80, false);
        let mut o1 = DrawObject::spawn(0, 0, 80, 200);
        let mut p2 = Fighter::spawn(240, true);
        let mut o2 = DrawObject::spawn(1, 0, 240, 200);
        o1.action = 0x35;
        let applied = {
            let mut att = FighterPair {
                player: &mut p1,
                obj: &mut o1,
            };
            let mut def = FighterPair {
                player: &mut p2,
                obj: &mut o2,
            };
            apply_hit(&mut att, &mut def, false)
        };
        assert!(applied.spark);
        assert!(p1.flags_a & CONNECT_800 != 0);
        assert!(p2.flags_a & WAS_HIT != 0);
        assert_eq!(p2.reaction_anim, 0x35);
        assert_eq!(applied.sound, Some(crate::fight::SOUND_BY_ACTION[0x35]));
        // Wall flags select the 0x1000 variant and set [si+0x0E].
        let mut p1 = Fighter::spawn(80, false);
        let mut o1 = DrawObject::spawn(0, 0, 80, 200);
        let mut p2 = Fighter::spawn(240, true);
        p2.flags_b = 1;
        let mut o2 = DrawObject::spawn(1, 0, 240, 200);
        let mut att = FighterPair {
            player: &mut p1,
            obj: &mut o1,
        };
        let mut def = FighterPair {
            player: &mut p2,
            obj: &mut o2,
        };
        apply_hit(&mut att, &mut def, false);
        assert!(p1.flags_a & CONNECT_1000 != 0);
        assert_eq!(p1.b0e, 0x18);
    }
}
