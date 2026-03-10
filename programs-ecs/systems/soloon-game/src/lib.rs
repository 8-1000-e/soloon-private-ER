use bolt_lang::*;
use game_state::GameState;

declare_id!("ZH7BDgpKKEfY24YhkRcpZ54EpjtLdhHVRvZa5S4YjSd");

const CMD_CREATE_MATCH: u8 = 0;
const CMD_COMMIT_ACTION: u8 = 1;
const CMD_REVEAL_ACTION: u8 = 2;
const CMD_RESOLVE_TURN: u8 = 3;
const CMD_END_MATCH: u8 = 4;

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
    #[msg("Invalid command")]      InvalidCommand,
    #[msg("Invalid player count")] InvalidPlayerCount,
    #[msg("Player not in match")]  NotInMatch,
    #[msg("Match not running")]    MatchNotRunning,
    #[msg("Wrong phase")]          WrongPhase,
    #[msg("Player is dead")]       PlayerDead,
    #[msg("Already committed")]    AlreadyCommitted,
    #[msg("Deadline passed")]      DeadlinePassed,
    #[msg("Invalid hash length")]  InvalidHashLength,
    #[msg("Not committed")]        NotCommitted,
    #[msg("Already revealed")]     AlreadyRevealed,
    #[msg("Invalid salt length")]  InvalidSaltLength,
    #[msg("Hash mismatch")]        HashMismatch,
    #[msg("Invalid action")]       InvalidAction,
    #[msg("Invalid target")]       InvalidTarget,
    #[msg("Cannot shoot self")]    CannotShootSelf,
    #[msg("Target is dead")]       TargetDead,
    #[msg("Cannot resolve yet")]   CannotResolveYet,
    #[msg("Match not finished")]   MatchNotFinished,
}

fn find_seat(players: &[Pubkey; 4], authority: &Pubkey, count: u8) -> Option<usize> {
    for i in 0..count as usize {
        if players[i] == *authority { return Some(i); }
    }
    None
}

#[system]
pub mod soloon_game {
    pub fn execute(ctx: Context<Components>, args_p: Vec<u8>) -> Result<Components> {
        let args: Args = parse_args(&args_p);
        let command = args.command;

        if command == CMD_CREATE_MATCH {
            let clock = Clock::get()?;
            let player_count = args.player_count.unwrap_or(2);
            let min_p = args.min_players.unwrap_or(2);
            let max_p = args.max_players.unwrap_or(4);

            require!(
                player_count >= min_p && player_count <= max_p && player_count <= 4,
                GameError::InvalidPlayerCount
            );

            let player_keys = args.player_keys.as_ref().expect("player_keys required");
            let mut player_pubkeys = Vec::with_capacity(player_count as usize);
            for i in 0..player_count as usize {
                let bytes: [u8; 32] = player_keys[i]
                    .as_slice()
                    .try_into()
                    .expect("Invalid pubkey length");
                player_pubkeys.push(Pubkey::from(bytes));
            }

            let g = &mut ctx.accounts.game_state;
            g.min_players = min_p;
            g.max_players = max_p;
            g.commit_duration_secs = args.commit_duration_secs.unwrap_or(15);
            g.reveal_duration_secs = args.reveal_duration_secs.unwrap_or(15);
            g.protect_lock_turns = args.protect_lock_turns.unwrap_or(1);
            g.bullets_cap = args.bullets_cap.unwrap_or(6);
            g.mirrors_cap = args.mirrors_cap.unwrap_or(1);
            g.loot_mode = 0;
            g.win_mode = 0;

            let match_id = args.match_id.unwrap_or(0);
            let match_id_bytes = match_id.to_le_bytes();
            let slot_bytes = clock.slot.to_le_bytes();
            let mut seed_input = Vec::with_capacity(8 + 32 * player_count as usize + 8);
            seed_input.extend_from_slice(&match_id_bytes);
            for i in 0..player_count as usize {
                seed_input.extend_from_slice(player_pubkeys[i].as_ref());
            }
            seed_input.extend_from_slice(&slot_bytes);
            let seed = solana_sha256_hasher::hashv(&[&seed_input]).to_bytes();

            g.match_id = match_id;
            g.status = STATUS_RUNNING;
            g.phase = PHASE_COMMIT;
            g.turn = 0;
            g.alive_mask = (1u16 << player_count) - 1;
            g.alive_count = player_count;
            g.seed = seed;
            g.commit_deadline = clock.unix_timestamp + g.commit_duration_secs as i64;
            g.reveal_deadline = 0;
            g.committed_count = 0;
            g.revealed_count = 0;
            g.winner_count = 0;
            g.transcript_hash = [0u8; 32];
            g.final_state_hash = [0u8; 32];

            g.player_count = player_count;
            for i in 0..player_count as usize {
                g.players[i] = player_pubkeys[i];
                g.bullets[i] = 1;
                g.mirrors[i] = 1;
                g.hits_received[i] = 0;
                g.protect_lock[i] = 0;
            }

            g.committed = 0;
            g.revealed = 0;
            g.actions = [0; 4];
            g.targets = [255; 4];
            g.commit_hashes = [[0u8; 32]; 4];
            g.salts = [[0u8; 32]; 4];

        } else if command == CMD_COMMIT_ACTION {
            let clock = Clock::get()?;
            let authority = *ctx.accounts.authority.key;
            let g = &mut ctx.accounts.game_state;

            let seat = find_seat(&g.players, &authority, g.player_count)
                .ok_or(error!(GameError::NotInMatch))?;

            require!(g.status == STATUS_RUNNING, GameError::MatchNotRunning);
            require!(g.phase == PHASE_COMMIT, GameError::WrongPhase);
            require!(g.alive_mask & (1 << seat) != 0, GameError::PlayerDead);
            require!(g.committed & (1 << seat) == 0, GameError::AlreadyCommitted);
            require!(clock.unix_timestamp <= g.commit_deadline, GameError::DeadlinePassed);

            let commit_hash_bytes = args.commit_hash.as_ref().expect("commit_hash required");
            require!(commit_hash_bytes.len() == 32, GameError::InvalidHashLength);

            let mut hash_arr = [0u8; 32];
            hash_arr.copy_from_slice(commit_hash_bytes);

            g.commit_hashes[seat] = hash_arr;
            g.committed |= 1 << seat as u16;
            g.committed_count += 1;

            if g.committed_count == g.alive_count {
                g.phase = PHASE_REVEAL;
                g.reveal_deadline = clock.unix_timestamp + g.reveal_duration_secs as i64;
            }

        } else if command == CMD_REVEAL_ACTION {
            let clock = Clock::get()?;
            let authority = *ctx.accounts.authority.key;
            let g = &mut ctx.accounts.game_state;

            let seat = find_seat(&g.players, &authority, g.player_count)
                .ok_or(error!(GameError::NotInMatch))?;

            require!(g.status == STATUS_RUNNING, GameError::MatchNotRunning);
            require!(g.phase == PHASE_REVEAL, GameError::WrongPhase);
            require!(g.committed & (1 << seat) != 0, GameError::NotCommitted);
            require!(g.revealed & (1 << seat) == 0, GameError::AlreadyRevealed);
            require!(clock.unix_timestamp <= g.reveal_deadline, GameError::DeadlinePassed);

            let salt_bytes = args.salt.as_ref().expect("salt required");
            require!(salt_bytes.len() == 32, GameError::InvalidSaltLength);

            let action_type = args.action_type.unwrap_or(0);
            let target = args.target.unwrap_or(255);

            let mut salt_arr = [0u8; 32];
            salt_arr.copy_from_slice(salt_bytes);

            let match_id_bytes = g.match_id.to_le_bytes();
            let turn_bytes = g.turn.to_le_bytes();
            let recomputed = solana_sha256_hasher::hashv(&[
                &[action_type], &[target], &salt_arr,
                &match_id_bytes, &turn_bytes, authority.as_ref(),
            ]).to_bytes();

            require!(recomputed == g.commit_hashes[seat], GameError::HashMismatch);
            require!(action_type <= 4, GameError::InvalidAction);

            if action_type == ACTION_SHOOT {
                let t = target as usize;
                require!(t < g.player_count as usize, GameError::InvalidTarget);
                require!(t != seat, GameError::CannotShootSelf);
                require!(g.alive_mask & (1 << t) != 0, GameError::TargetDead);
            }

            g.actions[seat] = action_type;
            g.targets[seat] = target;
            g.salts[seat] = salt_arr;
            g.revealed |= 1 << seat as u16;
            g.revealed_count += 1;

            if g.revealed_count == g.alive_count {
                g.phase = PHASE_RESOLVE;
            }

        } else if command == CMD_RESOLVE_TURN {
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

            if g.turn >= 100 {
                g.status = STATUS_FINISHED;
                g.winner_count = 0;
                let mid_bytes = g.match_id.to_le_bytes();
                let am_bytes = g.alive_mask.to_le_bytes();
                g.final_state_hash = solana_sha256_hasher::hashv(&[
                    g.seed.as_ref(), mid_bytes.as_ref(),
                    am_bytes.as_ref(), g.transcript_hash.as_ref(),
                ]).to_bytes();
            } else {
                let alive_mask = g.alive_mask;
                let pc = g.player_count as usize;

                for i in 0..pc {
                    if alive_mask & (1 << i) != 0 {
                        if g.committed & (1 << i) == 0 || g.revealed & (1 << i) == 0 {
                            g.actions[i] = ACTION_NOOP;
                            g.targets[i] = 255;
                        }
                    }
                }

                for i in 0..pc {
                    if g.protect_lock[i] > 0 { g.protect_lock[i] -= 1; }
                }

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

                for i in 0..pc {
                    if alive_mask & (1 << i) == 0 { continue; }
                    if g.actions[i] == ACTION_RELOAD && g.bullets[i] < g.bullets_cap {
                        g.bullets[i] += 1;
                    }
                }

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

                let old_hash = g.transcript_hash;
                let t_bytes = g.turn.to_le_bytes();
                g.transcript_hash = solana_sha256_hasher::hashv(&[
                    old_hash.as_ref(), t_bytes.as_ref(),
                    &g.actions[..pc], &g.targets[..pc],
                ]).to_bytes();

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
            }

        } else if command == CMD_END_MATCH {
            let g = &ctx.accounts.game_state;
            require!(g.status == STATUS_FINISHED, GameError::MatchNotFinished);

        } else {
            return Err(GameError::InvalidCommand.into());
        }

        Ok(ctx.accounts)
    }

    #[system_input]
    pub struct Components {
        pub game_state: GameState,
    }

    #[arguments]
    struct Args {
        command: u8,
        match_id: Option<u64>,
        player_count: Option<u8>,
        player_keys: Option<Vec<Vec<u8>>>,
        min_players: Option<u8>,
        max_players: Option<u8>,
        commit_duration_secs: Option<u16>,
        reveal_duration_secs: Option<u16>,
        protect_lock_turns: Option<u8>,
        bullets_cap: Option<u8>,
        mirrors_cap: Option<u8>,
        commit_hash: Option<Vec<u8>>,
        action_type: Option<u8>,
        target: Option<u8>,
        salt: Option<Vec<u8>>,
    }
}
