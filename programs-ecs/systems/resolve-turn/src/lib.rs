use bolt_lang::*;
use game_state::GameState;

declare_id!("DjN7EguTF3dhZ3S1ueFZDFNB4VeEXruPRdVJQ9LbMqZ");

const STATUS_RUNNING: u8 = 1;
const STATUS_FINISHED: u8 = 2;
const PHASE_COMMIT: u8 = 0;
const PHASE_REVEAL: u8 = 1;
const PHASE_RESOLVE: u8 = 2;

const ACTION_NOOP: u8 = 0;
const ACTION_PROTECT: u8 = 1;
const ACTION_RELOAD: u8 = 2;
const ACTION_MIRROR: u8 = 3;
const ACTION_SHOOT: u8 = 4;

#[error_code]
pub enum GameError {
    #[msg("Match not running")]    MatchNotRunning,
    #[msg("Cannot resolve yet")]   CannotResolveYet,
}

#[system]
pub mod resolve_turn {
    pub fn execute(ctx: Context<Components>, _args_p: Vec<u8>) -> Result<Components> {
        let clock = Clock::get()?;
        let g = &mut ctx.accounts.game_state;

        require!(g.status == STATUS_RUNNING, GameError::MatchNotRunning);

        let can_resolve = match g.phase {
            PHASE_RESOLVE => true,
            PHASE_REVEAL => clock.unix_timestamp > g.reveal_deadline,
            PHASE_COMMIT => clock.unix_timestamp > g.commit_deadline,
            _ => false,
        };
        require!(can_resolve, GameError::CannotResolveYet);

        // Turn-100 timeout
        if g.turn >= 100 {
            g.status = STATUS_FINISHED;
            g.winner_count = 0;
            let mid_bytes = g.match_id.to_le_bytes();
            let am_bytes = g.alive_mask.to_le_bytes();
            g.final_state_hash = solana_sha256_hasher::hashv(&[
                g.seed.as_ref(), mid_bytes.as_ref(),
                am_bytes.as_ref(), g.transcript_hash.as_ref(),
            ]).to_bytes();
            return Ok(ctx.accounts);
        }

        let alive_mask = g.alive_mask;
        let pc = g.player_count as usize;

        // Step 0: Default missing commits/reveals to NOOP
        for i in 0..pc {
            if alive_mask & (1 << i) != 0 {
                if g.committed & (1 << i) == 0 || g.revealed & (1 << i) == 0 {
                    g.actions[i] = ACTION_NOOP;
                    g.targets[i] = 255;
                }
            }
        }

        // Step 1: Tick down protect_lock
        for i in 0..pc {
            if g.protect_lock[i] > 0 { g.protect_lock[i] -= 1; }
        }

        // Step 2: Determine active flags
        let mut is_protecting = [false; 4];
        let mut is_mirroring = [false; 4];
        for i in 0..pc {
            if alive_mask & (1 << i) == 0 { continue; }
            match g.actions[i] {
                ACTION_PROTECT => {
                    if g.protect_lock[i] == 0 { is_protecting[i] = true; }
                }
                ACTION_MIRROR => {
                    if g.mirrors[i] > 0 {
                        is_mirroring[i] = true;
                        g.mirrors[i] -= 1;
                    }
                }
                _ => {}
            }
        }

        // Step 3: Apply reloads
        for i in 0..pc {
            if alive_mask & (1 << i) == 0 { continue; }
            if g.actions[i] == ACTION_RELOAD && g.bullets[i] < g.bullets_cap {
                g.bullets[i] += 1;
            }
        }

        // Step 4: Resolve shots
        let mut died_this_turn = [false; 4];
        let mut killer_of = [255u8; 4];

        for s in 0..pc {
            if alive_mask & (1 << s) == 0 { continue; }
            if g.actions[s] != ACTION_SHOOT { continue; }
            if g.bullets[s] == 0 { continue; }

            let turn_bytes = g.turn.to_le_bytes();
            let seat_byte = [s as u8];
            let roll_hash = solana_sha256_hasher::hashv(&[
                g.seed.as_ref(), turn_bytes.as_ref(), seat_byte.as_ref(),
            ]).to_bytes();
            let roll = roll_hash[0] % 6;

            if roll >= g.bullets[s] { continue; }

            g.bullets[s] -= 1;
            let t = g.targets[s] as usize;
            if t >= pc || alive_mask & (1 << t) == 0 { continue; }

            if is_mirroring[t] {
                is_mirroring[t] = false;
                if !is_protecting[s] {
                    g.hits_received[s] += 1;
                    g.protect_lock[s] = g.protect_lock_turns;
                    if g.hits_received[s] >= 3 { died_this_turn[s] = true; }
                    if killer_of[s] == 255 { killer_of[s] = t as u8; }
                }
                continue;
            }

            if is_protecting[t] { continue; }

            g.hits_received[t] += 1;
            g.protect_lock[t] = g.protect_lock_turns;
            if g.hits_received[t] >= 3 { died_this_turn[t] = true; }
            if killer_of[t] == 255 { killer_of[t] = s as u8; }
        }

        // Step 5: Apply deaths and loot
        for i in 0..pc {
            if died_this_turn[i] && (g.alive_mask & (1 << i) != 0) {
                g.alive_mask &= !(1u16 << i);
                g.alive_count -= 1;

                let turn_bytes = g.turn.to_le_bytes();
                let seat_byte = [i as u8];
                let loot_hash = solana_sha256_hasher::hashv(&[
                    g.seed.as_ref(), turn_bytes.as_ref(), seat_byte.as_ref(),
                ]).to_bytes();

                let k = killer_of[i] as usize;
                if k < pc && (g.alive_mask & (1 << k) != 0) {
                    if loot_hash[0] % 2 == 0 {
                        if g.bullets[k] < g.bullets_cap { g.bullets[k] += 1; }
                    } else if g.mirrors[k] < g.mirrors_cap {
                        g.mirrors[k] += 1;
                    }
                }
            }
        }

        // Step 6: Update transcript hash
        let old_hash = g.transcript_hash;
        let t_bytes = g.turn.to_le_bytes();
        g.transcript_hash = solana_sha256_hasher::hashv(&[
            old_hash.as_ref(), t_bytes.as_ref(),
            &g.actions[..pc], &g.targets[..pc],
        ]).to_bytes();

        // Step 7: Check win condition
        if g.alive_count <= 1 {
            g.status = STATUS_FINISHED;
            g.winner_count = 0;
            if g.alive_count == 1 {
                for i in 0..pc {
                    if g.alive_mask & (1 << i) != 0 {
                        g.winners[0] = g.players[i];
                        g.winner_count = 1;
                        break;
                    }
                }
            } else {
                // all died simultaneously
                for i in 0..pc {
                    if alive_mask & (1 << i) != 0 {
                        let wc = g.winner_count as usize;
                        g.winners[wc] = g.players[i];
                        g.winner_count += 1;
                    }
                }
            }
            let mid_bytes = g.match_id.to_le_bytes();
            let am_bytes = g.alive_mask.to_le_bytes();
            g.final_state_hash = solana_sha256_hasher::hashv(&[
                g.seed.as_ref(), mid_bytes.as_ref(),
                am_bytes.as_ref(), g.transcript_hash.as_ref(),
            ]).to_bytes();
        } else {
            g.turn += 1;
            g.phase = PHASE_COMMIT;
            g.committed_count = 0;
            g.revealed_count = 0;
            g.commit_deadline = clock.unix_timestamp + g.commit_duration_secs as i64;
            g.reveal_deadline = 0;

            g.committed = 0;
            g.revealed = 0;
            g.actions = [ACTION_NOOP; 4];
            g.targets = [255; 4];
            g.commit_hashes = [[0u8; 32]; 4];
            g.salts = [[0u8; 32]; 4];
        }

        Ok(ctx.accounts)
    }

    #[system_input]
    pub struct Components {
        pub game_state: GameState,
    }
}
