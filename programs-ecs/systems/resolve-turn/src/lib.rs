use bolt_lang::*;
use game_config::{GameConfig, PendingEffect, MAX_PLAYERS};

declare_id!("9kMyrkkyuhiZYMeGXJBFTgrfMMtfmBkkKk7R4eLV3NKv");

// ── Phase / status ────────────────────────────────────────────────────
const STATUS_RUNNING: u8 = 1;
const STATUS_FINISHED: u8 = 2;
const PHASE_TURN: u8 = 0;
const PHASE_RESOLVE: u8 = 1;

// ── Actions ───────────────────────────────────────────────────────────
const ACTION_NOOP: u8 = 0;
const ACTION_PROTECT: u8 = 1;
const ACTION_RELOAD: u8 = 2;
const ACTION_MIRROR: u8 = 3;
const ACTION_SHOOT: u8 = 4;

// ── Game-rule constants (no on-chain RNG since the PER TEE is the
//    integrity layer, so these are deterministic). ────────────────────
/// Hits required to drop a player. The original commit-reveal design
/// used a per-pubkey RNG roll in 2..=6; with randomness gone we pin
/// this at a flat 3 — short matches stay snappy without the gambling.
const DEATH_THRESHOLD: u8 = 3;
/// Bullets always hit when fired. The commit-reveal design used a
/// per-shot RNG roll to add variance; with the TEE as the integrity
/// layer we drop the variance for clarity (Reload still matters because
/// you only get one shot per bullet).
const SHOTS_ALWAYS_HIT: bool = true;

// ── Bolt-prepended account layout ─────────────────────────────────────
/// Bolt prepends one AccountInfo per `#[system_input]` component, so
/// our extras start at index 1 (GameConfig is the only input). The
/// PlayerRegistry sits at NUM_COMPONENTS (read-only raw bytes), and
/// the N PlayerStates start at NUM_COMPONENTS + 1.
const NUM_COMPONENTS: usize = 1;

// ── PlayerRegistry raw-byte layout ────────────────────────────────────
// Mirror trade-fight's pattern: we keep PlayerRegistry out of
// `#[system_input]` and read fields straight from the bytes, so the
// component doesn't get echoed back as return data and blow the
// 1024-byte set_return_data cap.
//
//   0..8                                                  discriminator
//   8..16                                                 match_id (u64) ← used for auth tie
//   16..20                                                players Vec len (u32 = MAX_PLAYERS)
//   20..(20 + 32*MAX_PLAYERS)                             players: 32B each ← authority pubkeys
//   (20 + 32*MAX_PLAYERS)..(24 + 32*MAX_PLAYERS)          player_states Vec len (u32)
//   (24 + 32*MAX_PLAYERS)..(24 + 64*MAX_PLAYERS)          player_states: 32B each ← PlayerState PDAs
//   (24 + 64*MAX_PLAYERS)                                 count (u8)
//   (25 + 64*MAX_PLAYERS)..(57 + 64*MAX_PLAYERS)          bolt_metadata.authority (Pubkey)
//
// Bolt 0.2.4's BoltMetadata only carries `authority` — no `entity`
// field — so we ship the cross-component identity tie ourselves via
// the `match_id` field we added to PlayerRegistry.
const PR_MATCH_ID_OFFSET: usize = 8;
const PR_PLAYERS_OFFSET: usize = 20;
const PR_PLAYER_STATES_OFFSET: usize = 24 + 32 * MAX_PLAYERS;
const PR_COUNT_OFFSET: usize = 24 + 64 * MAX_PLAYERS;

// ── PlayerState raw-byte layout ───────────────────────────────────────
// Anchor account = 8 discriminator + struct fields (LE, no padding for
// u8/bool) + 64 BoltMetadata at the end.
//   0..8     discriminator
//   8..40    authority   (Pubkey)
//   40       seat        (u8)
//   41       alive       (u8 / bool)
//   42       bullets     (u8)
//   43       mirrors     (u8)
//   44       hits_received (u8)
//   45       protect_lock  (u8)
//   46       submitted   (u8 / bool)
//   47       action_type (u8)
//   48       target      (u8)
//   49..113  bolt_metadata (32 + 32)
const PS_SEAT: usize = 40;
const PS_ALIVE: usize = 41;
const PS_BULLETS: usize = 42;
const PS_MIRRORS: usize = 43;
const PS_HITS: usize = 44;
const PS_PROTECT_LOCK: usize = 45;
const PS_SUBMITTED: usize = 46;
const PS_ACTION: usize = 47;
const PS_TARGET: usize = 48;
const PS_MIN_LEN: usize = 49;
const NO_TARGET: u8 = 255;

#[error_code]
pub enum GameError {
    #[msg("Match not running")]                MatchNotRunning,
    #[msg("Cannot resolve yet (phase mismatch or before deadline)")]
                                                CannotResolveYet,
    #[msg("Apply pass in progress — call apply-turn for every player before re-resolving")]
                                                ApplyInProgress,
    #[msg("Invalid account size")]             InvalidAccount,
    #[msg("Player count exceeds MAX_PLAYERS")] TooManyPlayers,
    #[msg("PlayerRegistry.match_id doesn't match GameConfig.match_id — wrong registry for this match")]
                                                RegistryMatchIdMismatch,
    #[msg("PlayerState pubkey doesn't match the one registered for this seat")]
                                                PlayerStateMismatch,
}

/// Compute pass — reads every PlayerState raw (read-only), runs the
/// turn-resolution rules, and writes the post-resolution snapshot per
/// seat into `GameConfig.pending_effects`. The per-player `apply-turn`
/// then lands each snapshot onto the actual `PlayerState`.
///
/// Why two-stage:
///   Bolt only mutates components listed in `#[system_input]`. We need
///   to update N PlayerStates atomically based on each other (shots
///   resolve against targets, mirror reflections, etc.) so a single
///   per-player system can't see the whole picture. Snapshotting into
///   GameConfig keeps the compute pass single-tx + atomic.
///
/// Gates:
///   - `status == Running`
///   - Either everyone alive has submitted (`submit-action` flipped
///     `phase = Resolve`), or the turn deadline has passed (timeout).
///
/// remaining_accounts (after NUM_COMPONENTS Bolt-prepended slots):
///   [NUM_COMPONENTS]              PlayerRegistry PDA (raw read)
///   [NUM_COMPONENTS + 1 + i]      PlayerState i (raw read, 0..count)
#[system]
pub mod resolve_turn {
    pub fn execute(ctx: Context<Components>, _args_p: Vec<u8>) -> Result<Components> {
        let clock = Clock::get()?;
        let g = &mut ctx.accounts.game_config;

        // ── Gate: match running + ready to resolve. ────────────────────
        require!(g.status == STATUS_RUNNING, GameError::MatchNotRunning);
        let can_resolve = match g.phase {
            PHASE_RESOLVE => true,
            PHASE_TURN => clock.unix_timestamp > g.turn_deadline,
            _ => false,
        };
        require!(can_resolve, GameError::CannotResolveYet);

        // ── Anti-replay: prevent re-running resolve while an apply pass
        //    is still mid-flight. Without this an attacker could call
        //    resolve-turn twice without any apply-turn between, racing
        //    `g.turn`/`pending_effects` and triggering the 100-turn
        //    timeout prematurely. `apply-turn` writes `applied = true`
        //    per seat AND bumps `g.apply_count`; we ONLY accept a fresh
        //    resolve when the previous round's applies are all done
        //    (apply_count >= active_players) OR this is the very first
        //    resolve (apply_count == 0 from init-game). ─────────────
        require!(
            g.apply_count == 0 || g.apply_count >= g.active_players,
            GameError::ApplyInProgress
        );

        // ── Authenticate PlayerRegistry + read count.
        //    Chain of trust:
        //      GameConfig (Bolt validates via #[system_input])
        //          └─ match_id ─── must equal Registry.match_id
        //    so a malicious caller can't substitute a fake registry
        //    pointing at a different match. Bolt 0.2.4 doesn't expose
        //    the entity pubkey on bolt_metadata, so we ship the tie
        //    via a `match_id` field on PlayerRegistry that init-game
        //    writes once from GameConfig.match_id. ──────────────────
        let registry_acc = &ctx.remaining_accounts[NUM_COMPONENTS];
        let registry_data = registry_acc.try_borrow_data()?;
        require!(
            registry_data.len() >= PR_COUNT_OFFSET + 1,
            GameError::InvalidAccount
        );

        let registry_match_id = u64::from_le_bytes(
            registry_data[PR_MATCH_ID_OFFSET..PR_MATCH_ID_OFFSET + 8]
                .try_into()
                .map_err(|_| GameError::InvalidAccount)?,
        );
        require!(
            registry_match_id == g.match_id,
            GameError::RegistryMatchIdMismatch
        );

        let count = registry_data[PR_COUNT_OFFSET] as usize;
        require!(count <= MAX_PLAYERS, GameError::TooManyPlayers);

        // Copy out BOTH the registered PlayerState pubkeys (used to
        // authenticate each remaining_accounts slot) AND the player
        // authority pubkeys (used to populate `winners` at end-of-game).
        // Done up-front so we can drop the registry borrow before
        // iterating the player state accounts (try_borrow_data on
        // those will need its own borrow).
        let mut registered_ps_pubkeys: Vec<[u8; 32]> = Vec::with_capacity(count);
        let mut player_authorities: Vec<[u8; 32]> = Vec::with_capacity(count);
        for i in 0..count {
            let ps_start = PR_PLAYER_STATES_OFFSET + i * 32;
            let auth_start = PR_PLAYERS_OFFSET + i * 32;
            let mut ps_buf = [0u8; 32];
            let mut auth_buf = [0u8; 32];
            ps_buf.copy_from_slice(&registry_data[ps_start..ps_start + 32]);
            auth_buf.copy_from_slice(&registry_data[auth_start..auth_start + 32]);
            registered_ps_pubkeys.push(ps_buf);
            player_authorities.push(auth_buf);
        }
        drop(registry_data);

        // ── Snapshot every PlayerState into local arrays. We can't
        //    hold borrows across writes, so copy out into fixed-size
        //    locals up-front. ────────────────────────────────────────
        let mut alive          = [false; MAX_PLAYERS];
        let mut bullets        = [0u8; MAX_PLAYERS];
        let mut mirrors        = [0u8; MAX_PLAYERS];
        let mut hits           = [0u8; MAX_PLAYERS];
        let mut protect_lock   = [0u8; MAX_PLAYERS];
        let mut submitted      = [false; MAX_PLAYERS];
        let mut action_type    = [ACTION_NOOP; MAX_PLAYERS];
        let mut target         = [NO_TARGET; MAX_PLAYERS];

        for i in 0..count {
            let acc = &ctx.remaining_accounts[NUM_COMPONENTS + 1 + i];

            // ── Identity check: the passed account MUST be the same
            //    PlayerState PDA we registered at this seat in
            //    spawn-player. Without this an attacker could swap
            //    accounts to fake bullets/hits/alive values. ─────────
            require!(
                acc.key.as_ref() == registered_ps_pubkeys[i].as_ref(),
                GameError::PlayerStateMismatch
            );

            let data = acc.try_borrow_data()?;
            require!(data.len() >= PS_MIN_LEN, GameError::InvalidAccount);
            // Use the seat stored ON the PlayerState (which equals i
            // for well-formed registries — we trust the registry's
            // ordering since spawn-player writes both atomically).
            let seat = data[PS_SEAT] as usize;
            require!(seat < MAX_PLAYERS, GameError::InvalidAccount);
            alive[seat]        = data[PS_ALIVE] != 0;
            bullets[seat]      = data[PS_BULLETS];
            mirrors[seat]      = data[PS_MIRRORS];
            hits[seat]         = data[PS_HITS];
            protect_lock[seat] = data[PS_PROTECT_LOCK];
            submitted[seat]    = data[PS_SUBMITTED] != 0;
            action_type[seat]  = data[PS_ACTION];
            target[seat]       = data[PS_TARGET];
        }

        // ── Capture the PRE-turn alive mask now, before any mutations
        //    below decide who dies. Used by the simultaneous-wipe tie-
        //    break in `collect_winners`: when every remaining seat
        //    dies on the SAME resolve, joint-win to whoever was alive
        //    at the start of this turn. Without this snapshot,
        //    everyone's `alive[i]` reads false at win-check time and
        //    the pot has nowhere to go. ─────────────────────────────
        let pre_turn_alive = alive;

        // (100-turn timeout check moved to AFTER the resolution step
        //  below so the last allowed turn still gets played + any
        //  pending shots resolve before the cap fires.)

        // ── Default missing submitters to NOOP. ───────────────────────
        for i in 0..count {
            if alive[i] && !submitted[i] {
                action_type[i] = ACTION_NOOP;
                target[i]      = NO_TARGET;
            }
        }

        // ── Step 1: tick down protect_lock for every alive seat. ──────
        for i in 0..count {
            if alive[i] && protect_lock[i] > 0 {
                protect_lock[i] -= 1;
            }
        }

        // ── Step 2: figure out is_protecting / is_mirroring flags.
        //    Protect is no-op when locked; Mirror consumes a charge. ──
        let mut is_protecting = [false; MAX_PLAYERS];
        let mut is_mirroring  = [false; MAX_PLAYERS];
        for i in 0..count {
            if !alive[i] { continue; }
            match action_type[i] {
                ACTION_PROTECT => {
                    if protect_lock[i] == 0 {
                        is_protecting[i] = true;
                    }
                }
                ACTION_MIRROR => {
                    if mirrors[i] > 0 {
                        is_mirroring[i] = true;
                        mirrors[i] -= 1;
                    }
                }
                _ => {}
            }
        }

        // ── Step 3: apply reloads (capped at bullets_cap). ────────────
        let bullets_cap = g.bullets_cap;
        for i in 0..count {
            if !alive[i] { continue; }
            if action_type[i] == ACTION_RELOAD && bullets[i] < bullets_cap {
                bullets[i] += 1;
            }
        }

        // ── Step 4: resolve shots. Loop in seat order — deterministic
        //    + matches the original. Killer ↔ killed for loot
        //    attribution tracked in `killer_of[t] = first attacker`.
        //
        //    Game design note: shots resolve SEQUENTIALLY in seat-index
        //    order, not "atomically simultaneous". If seat 0 shoots
        //    seat 2 and seat 1 also shoots seat 2 in the same turn:
        //      - seat 0's shot lands first (seat 2 takes hit #1)
        //      - seat 1's shot lands second (seat 2 takes hit #2)
        //    The bullet is still spent (line `bullets[s] -= 1` runs
        //    before the alive check). If seat 0 had killed seat 2,
        //    seat 1's shot would `continue` on the `!alive[t]` check
        //    and waste their bullet — that's the "fog of war" trade-
        //    off for shooting a target that already died this turn.
        //
        //    Tie-breaker for "who killed whom" (used by loot drop):
        //    `killer_of[t]` is set on the FIRST landed hit only. If
        //    two shooters land hits on the same victim, only the
        //    earlier (lower seat) gets the bullet bounty. ───────────
        let mut died_this_turn = [false; MAX_PLAYERS];
        let mut killer_of      = [NO_TARGET; MAX_PLAYERS];

        for s in 0..count {
            if !alive[s] { continue; }
            if action_type[s] != ACTION_SHOOT { continue; }
            if bullets[s] == 0 { continue; }
            // Always hit (no per-shot RNG without `seed`).
            if !SHOTS_ALWAYS_HIT { continue; }

            bullets[s] -= 1;
            let t = target[s] as usize;
            if t >= count || !alive[t] { continue; }

            if is_mirroring[t] {
                // Mirror reflects ONE shot. Disarm the mirror so a
                // simultaneous second shot in the same turn still lands.
                is_mirroring[t] = false;
                if !is_protecting[s] {
                    hits[s] += 1;
                    protect_lock[s] = g.protect_lock_turns;
                    if hits[s] >= DEATH_THRESHOLD {
                        died_this_turn[s] = true;
                    }
                    if killer_of[s] == NO_TARGET {
                        killer_of[s] = t as u8;
                    }
                }
                continue;
            }

            if is_protecting[t] {
                continue;
            }

            hits[t] += 1;
            protect_lock[t] = g.protect_lock_turns;
            if hits[t] >= DEATH_THRESHOLD {
                died_this_turn[t] = true;
            }
            if killer_of[t] == NO_TARGET {
                killer_of[t] = s as u8;
            }
        }

        // ── Step 5: apply deaths + deterministic loot.
        //    Loot rule (no RNG): killer always gets +1 bullet up to cap.
        //    No mirror loot drops — they only refresh via the dwindling
        //    Mirror cap which already favours scarcity. ───────────────
        let mut new_alive_count: u8 = 0;
        for i in 0..count {
            if died_this_turn[i] && alive[i] {
                alive[i] = false;
                let k = killer_of[i] as usize;
                if k < count && alive[k] && bullets[k] < bullets_cap {
                    bullets[k] += 1;
                }
            }
            if alive[i] {
                new_alive_count += 1;
            }
        }
        g.alive_count = new_alive_count;

        // ── Step 6: write the post-resolution snapshot into GameConfig
        //    for `apply-turn` to consume per player. ──────────────────
        for i in 0..count {
            g.pending_effects[i] = PendingEffect {
                bullets:       bullets[i],
                mirrors:       mirrors[i],
                hits_received: hits[i],
                protect_lock:  protect_lock[i],
                alive:         alive[i],
                applied:       false,
            };
        }
        // Zero out trailing slots so a shrinking match doesn't carry
        // stale data from previous turns.
        for i in count..MAX_PLAYERS {
            g.pending_effects[i] = PendingEffect::default();
        }
        g.apply_count = 0;

        // ── Step 7: win check (after deaths). ─────────────────────────
        if new_alive_count <= 1 {
            // Game over — write winners + flip status. apply-turn will
            // still run to reset per-player turn fields, but won't
            // bump the turn.
            collect_winners(g, count, &alive, &pre_turn_alive, &player_authorities);
            g.status = STATUS_FINISHED;
            g.phase = PHASE_RESOLVE;
            return Ok(ctx.accounts);
        }

        // ── Step 8a: 100-turn timeout. Fires AFTER the resolution
        //    above so turn 99's shots, deaths, and loot actually
        //    apply — the timeout is a "next turn would be #100, stop
        //    here" check rather than an "abort mid-turn" kill switch.
        //    If turn 99 didn't produce a winner, we finalize the match
        //    with no winner (pot goes to house via lobby's payout). ──
        if g.turn >= 99 {
            finalize_match(
                g, count, &alive, &bullets, &mirrors, &hits, &protect_lock,
                &player_authorities,
            );
            return Ok(ctx.accounts);
        }

        // ── Step 8b: prep next turn (the LAST apply-turn flips
        //    phase back to Turn). ──────────────────────────────────────
        g.turn         = g.turn.saturating_add(1);
        g.turn_deadline = clock.unix_timestamp + g.turn_duration_secs as i64;
        g.submitted_count = 0;
        g.phase        = PHASE_RESOLVE;

        Ok(ctx.accounts)
    }

    #[system_input]
    pub struct Components {
        pub game_config: GameConfig,
    }
}

/// 100-turn timeout handler: hard-finish the match with no winner.
/// Still snapshots the current per-seat state so `apply-turn` can run
/// uniformly (resetting submitted/action_type/target). Avoids a
/// special-case branch in apply-turn.
///
/// Per the original Soloon rules, a turn-100 timeout has no winner —
/// `winners` stays empty even if seats are still alive. The pot would
/// then be eligible for the house-rake path in the lobby program.
fn finalize_match(
    g: &mut GameConfig,
    count: usize,
    alive: &[bool; MAX_PLAYERS],
    bullets: &[u8; MAX_PLAYERS],
    mirrors: &[u8; MAX_PLAYERS],
    hits: &[u8; MAX_PLAYERS],
    protect_lock: &[u8; MAX_PLAYERS],
    _player_authorities: &[[u8; 32]],
) {
    g.status = STATUS_FINISHED;
    g.winner_count = 0;
    g.winners.clear();
    for i in 0..count {
        g.pending_effects[i] = PendingEffect {
            bullets: bullets[i],
            mirrors: mirrors[i],
            hits_received: hits[i],
            protect_lock: protect_lock[i],
            alive: alive[i],
            applied: false,
        };
    }
    for i in count..MAX_PLAYERS {
        g.pending_effects[i] = PendingEffect::default();
    }
    g.apply_count = 0;
    g.phase = PHASE_RESOLVE;
}

/// Walk the alive bitset and write the winners list.
///
/// Standard case (most matches):
///   `alive_after` has exactly 1 true bit → 1 winner takes the pot.
///
/// Simultaneous-wipe tie-break (rare):
///   The last surviving seats all die on the SAME resolve (e.g. two
///   players each at 2 hits, both Shoot, both land → both reach 3
///   hits → both die). `alive_after` is all-false. We fall back to
///   `pre_turn_alive` and joint-win to the players who were alive at
///   the START of this turn — the lobby's distribute_prize then
///   splits the prize equally among them.
///
/// `alive_after_count` is always ≤ 1 on entry (caller gates on that).
fn collect_winners(
    g: &mut GameConfig,
    count: usize,
    alive_after: &[bool; MAX_PLAYERS],
    pre_turn_alive: &[bool; MAX_PLAYERS],
    player_authorities: &[[u8; 32]],
) {
    g.winners.clear();
    g.winner_count = 0;

    let alive_after_count = (0..count).filter(|&i| alive_after[i]).count();

    if alive_after_count >= 1 {
        // ── LastAlive — one (or zero, see below) winner. ───────────
        for i in 0..count {
            if alive_after[i] {
                g.winners.push(player_authorities[i]);
                g.winner_count = g.winner_count.saturating_add(1);
            }
        }
    } else {
        // ── Simultaneous wipe — joint-win to the pre-turn survivors.
        //    Captured in the main loop before any death was applied. ─
        for i in 0..count {
            if pre_turn_alive[i] {
                g.winners.push(player_authorities[i]);
                g.winner_count = g.winner_count.saturating_add(1);
            }
        }
    }
}
