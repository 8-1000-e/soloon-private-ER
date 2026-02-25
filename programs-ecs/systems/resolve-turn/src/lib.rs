use bolt_lang::*;
use match_config::MatchConfig;
use match_state::MatchState;
use players::Players;
use turn_data::TurnData;

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
    #[msg("Match not running")]
    MatchNotRunning,
    #[msg("Cannot resolve yet")]
    CannotResolveYet,
}

#[system]
pub mod resolve_turn {

    pub fn execute(ctx: Context<Components>, _args_p: Vec<u8>) -> Result<Components> {
        let clock = Clock::get()?;
        let config = &ctx.accounts.match_config;
        let state = &mut ctx.accounts.match_state;
        let players = &mut ctx.accounts.players;
        let turn = &mut ctx.accounts.turn_data;

        require!(state.status == STATUS_RUNNING, GameError::MatchNotRunning);

        // Callable when: phase==Resolve, or past deadline (Reveal or Commit)
        let can_resolve = match state.phase {
            PHASE_RESOLVE => true,
            PHASE_REVEAL => clock.unix_timestamp > state.reveal_deadline,
            PHASE_COMMIT => clock.unix_timestamp > state.commit_deadline,
            _ => false,
        };
        require!(can_resolve, GameError::CannotResolveYet);

        let alive_mask = state.alive_mask;
        let pc = players.player_count as usize;

        // Step 0: Default missing commits/reveals to NOOP
        for i in 0..pc {
            if alive_mask & (1 << i) != 0 {
                if turn.committed & (1 << i) == 0 || turn.revealed & (1 << i) == 0 {
                    turn.actions[i] = ACTION_NOOP;
                    turn.targets[i] = 255;
                }
            }
        }

        // Step 1: Tick down protect_lock
        for i in 0..pc {
            if players.protect_lock[i] > 0 {
                players.protect_lock[i] -= 1;
            }
        }

        // Step 2: Determine active flags
        let mut is_protecting = [false; 4];
        let mut is_mirroring = [false; 4];

        for i in 0..pc {
            if alive_mask & (1 << i) == 0 {
                continue;
            }
            match turn.actions[i] {
                ACTION_PROTECT => {
                    if players.protect_lock[i] == 0 {
                        is_protecting[i] = true;
                    }
                }
                ACTION_MIRROR => {
                    if players.mirrors[i] > 0 {
                        is_mirroring[i] = true;
                        players.mirrors[i] -= 1;
                    }
                }
                _ => {}
            }
        }

        // Step 3: Apply reloads (ascending seat order)
        for i in 0..pc {
            if alive_mask & (1 << i) == 0 {
                continue;
            }
            if turn.actions[i] == ACTION_RELOAD && players.bullets[i] < config.bullets_cap {
                players.bullets[i] += 1;
            }
        }

        // Step 4: Resolve shots (ascending seat order)
        // Russian roulette: N bullets loaded = N/6 chance to fire
        // Fixed 3 lives: player dies at hits_received >= 3
        let mut died_this_turn = [false; 4];
        let mut killer_of = [255u8; 4];

        for s in 0..pc {
            if alive_mask & (1 << s) == 0 {
                continue;
            }
            if turn.actions[s] != ACTION_SHOOT {
                continue;
            }
            if players.bullets[s] == 0 {
                continue;
            }

            // Russian roulette roll: SHA256(seed || turn || seat)[0] % 6
            let turn_bytes = state.turn.to_le_bytes();
            let seat_byte = [s as u8];
            let roll_hash = solana_sha256_hasher::hashv(&[
                state.seed.as_ref(),
                turn_bytes.as_ref(),
                seat_byte.as_ref(),
            ]).to_bytes();
            let roll = roll_hash[0] % 6;

            // If roll >= bullets, shot fails — no bullet lost, no hit
            if roll >= players.bullets[s] {
                continue;
            }

            // Shot fires — consume 1 bullet
            players.bullets[s] -= 1;

            let t = turn.targets[s] as usize;
            if t >= pc || alive_mask & (1 << t) == 0 {
                continue;
            }

            // Check mirror first
            if is_mirroring[t] {
                is_mirroring[t] = false;
                if !is_protecting[s] {
                    players.hits_received[s] += 1;
                    players.protect_lock[s] = config.protect_lock_turns;
                    if players.hits_received[s] >= 3 {
                        died_this_turn[s] = true;
                    }
                    if killer_of[s] == 255 {
                        killer_of[s] = t as u8;
                    }
                }
                continue;
            }

            // Check protect
            if is_protecting[t] {
                continue;
            }

            // Normal hit
            players.hits_received[t] += 1;
            players.protect_lock[t] = config.protect_lock_turns;
            if players.hits_received[t] >= 3 {
                died_this_turn[t] = true;
            }
            if killer_of[t] == 255 {
                killer_of[t] = s as u8;
            }
        }

        // Step 5: Apply deaths and loot
        for i in 0..pc {
            if died_this_turn[i] && (state.alive_mask & (1 << i) != 0) {
                state.alive_mask &= !(1u16 << i);
                state.alive_count -= 1;

                let turn_bytes = state.turn.to_le_bytes();
                let seat_byte = [i as u8];
                let loot_hash = solana_sha256_hasher::hashv(&[
                    state.seed.as_ref(),
                    turn_bytes.as_ref(),
                    seat_byte.as_ref(),
                ])
                .to_bytes();

                let k = killer_of[i] as usize;
                if k < pc && (state.alive_mask & (1 << k) != 0) {
                    if loot_hash[0] % 2 == 0 {
                        if players.bullets[k] < config.bullets_cap {
                            players.bullets[k] += 1;
                        }
                    } else if players.mirrors[k] < config.mirrors_cap {
                        players.mirrors[k] += 1;
                    }
                }
            }
        }

        // Step 6: Update transcript hash
        let old_hash = state.transcript_hash;
        let t_bytes = state.turn.to_le_bytes();
        state.transcript_hash = solana_sha256_hasher::hashv(&[
            old_hash.as_ref(),
            t_bytes.as_ref(),
            &turn.actions[..pc],
            &turn.targets[..pc],
        ])
        .to_bytes();

        // Step 7: Check win condition
        if state.alive_count <= 1 {
            state.status = STATUS_FINISHED;
            state.winner_count = 0;
            if state.alive_count == 1 {
                // Normal win: single survivor
                for i in 0..pc {
                    if state.alive_mask & (1 << i) != 0 {
                        state.winners[0] = players.players[i];
                        state.winner_count = 1;
                        break;
                    }
                }
            } else {
                // alive_count == 0: all remaining players died this turn.
                // alive_mask (captured before deaths at line 50) identifies who was
                // last alive — they share the prize equally.
                for i in 0..pc {
                    if alive_mask & (1 << i) != 0 {
                        let wc = state.winner_count as usize;
                        state.winners[wc] = players.players[i];
                        state.winner_count += 1;
                    }
                }
            }
            let mid_bytes = state.match_id.to_le_bytes();
            let am_bytes = state.alive_mask.to_le_bytes();
            state.final_state_hash = solana_sha256_hasher::hashv(&[
                state.seed.as_ref(),
                mid_bytes.as_ref(),
                am_bytes.as_ref(),
                state.transcript_hash.as_ref(),
            ])
            .to_bytes();
        } else {
            state.turn += 1;
            state.phase = PHASE_COMMIT;
            state.committed_count = 0;
            state.revealed_count = 0;
            state.commit_deadline =
                clock.unix_timestamp + config.commit_duration_secs as i64;
            state.reveal_deadline = 0;

            turn.committed = 0;
            turn.revealed = 0;
            turn.actions = [ACTION_NOOP; 4];
            turn.targets = [255; 4];
            turn.commit_hashes = [[0u8; 32]; 4];
            turn.salts = [[0u8; 32]; 4];
        }

        Ok(ctx.accounts)
    }

    #[system_input]
    pub struct Components {
        pub match_config: MatchConfig,
        pub match_state: MatchState,
        pub players: Players,
        pub turn_data: TurnData,
    }
}
