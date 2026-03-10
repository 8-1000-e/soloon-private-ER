use bolt_lang::*;
use match_config::MatchConfig;
use match_state::MatchState;
use players::Players;
use turn_data::TurnData;

declare_id!("EaXW3WKMewu7hzeDCJX4phpQj75uWtNse8aaqyAsguPy");

const STATUS_RUNNING: u8 = 1;
const PHASE_COMMIT: u8 = 0;

#[error_code]
pub enum GameError {
    #[msg("Invalid player count")]
    InvalidPlayerCount,
}

#[system]
pub mod create_match {

    pub fn execute(ctx: Context<Components>, args_p: Vec<u8>) -> Result<Components> {
        let args: Args = parse_args(&args_p);
        let clock = Clock::get()?;

        let player_count = args.player_count;
        let min_p = args.min_players.unwrap_or(2);
        let max_p = args.max_players.unwrap_or(4);

        require!(
            player_count >= min_p && player_count <= max_p && player_count <= 4,
            GameError::InvalidPlayerCount
        );

        // Parse player pubkeys from raw byte arrays
        let mut player_pubkeys = Vec::with_capacity(player_count as usize);
        for i in 0..player_count as usize {
            let bytes: [u8; 32] = args.player_keys[i]
                .as_slice()
                .try_into()
                .expect("Invalid pubkey length");
            player_pubkeys.push(Pubkey::from(bytes));
        }

        // Initialize MatchConfig
        let config = &mut ctx.accounts.match_config;
        config.min_players = min_p;
        config.max_players = max_p;
        config.commit_duration_secs = args.commit_duration_secs.unwrap_or(15);
        config.reveal_duration_secs = args.reveal_duration_secs.unwrap_or(15);
        config.protect_lock_turns = args.protect_lock_turns.unwrap_or(1);
        config.bullets_cap = args.bullets_cap.unwrap_or(6);
        config.mirrors_cap = args.mirrors_cap.unwrap_or(1);
        config.loot_mode = 0; // OnKill
        config.win_mode = 0; // LastAlive

        // Build seed = SHA256(match_id || player_pubkeys || clock.slot)
        let match_id_bytes = args.match_id.to_le_bytes();
        let slot_bytes = clock.slot.to_le_bytes();
        let mut seed_input = Vec::with_capacity(8 + 32 * player_count as usize + 8);
        seed_input.extend_from_slice(&match_id_bytes);
        for i in 0..player_count as usize {
            seed_input.extend_from_slice(player_pubkeys[i].as_ref());
        }
        seed_input.extend_from_slice(&slot_bytes);
        let seed = solana_sha256_hasher::hashv(&[&seed_input]).to_bytes();

        // Initialize MatchState
        let state = &mut ctx.accounts.match_state;
        state.match_id = args.match_id;
        state.status = STATUS_RUNNING;
        state.phase = PHASE_COMMIT;
        state.turn = 0;
        state.alive_mask = (1u16 << player_count) - 1;
        state.alive_count = player_count;
        state.seed = seed;
        state.commit_deadline = clock.unix_timestamp + config.commit_duration_secs as i64;
        state.reveal_deadline = 0;
        state.committed_count = 0;
        state.revealed_count = 0;
        state.winner_count = 0;
        state.transcript_hash = [0u8; 32];
        state.final_state_hash = [0u8; 32];

        // Initialize Players
        let players = &mut ctx.accounts.players;
        players.player_count = player_count;
        for i in 0..player_count as usize {
            players.players[i] = player_pubkeys[i];
            players.bullets[i] = 1; // start with 1 bullet
            players.mirrors[i] = 1; // start with 1 mirror
            players.hits_received[i] = 0;
            players.protect_lock[i] = 0;
        }

        // TurnData starts zeroed (from Default), no extra init needed

        Ok(ctx.accounts)
    }

    #[system_input]
    pub struct Components {
        pub match_config: MatchConfig,
        pub match_state: MatchState,
        pub players: Players,
        pub turn_data: TurnData,
    }

    #[arguments]
    struct Args {
        match_id: u64,
        player_count: u8,
        player_keys: Vec<Vec<u8>>,
        min_players: Option<u8>,
        max_players: Option<u8>,
        commit_duration_secs: Option<u16>,
        reveal_duration_secs: Option<u16>,
        protect_lock_turns: Option<u8>,
        bullets_cap: Option<u8>,
        mirrors_cap: Option<u8>,
    }
}
