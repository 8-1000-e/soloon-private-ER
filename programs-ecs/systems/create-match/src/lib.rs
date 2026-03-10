use bolt_lang::*;
use game_state::GameState;

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

        let mut player_pubkeys = Vec::with_capacity(player_count as usize);
        for i in 0..player_count as usize {
            let bytes: [u8; 32] = args.player_keys[i]
                .as_slice()
                .try_into()
                .expect("Invalid pubkey length");
            player_pubkeys.push(Pubkey::from(bytes));
        }

        let g = &mut ctx.accounts.game_state;

        // ── Config ──
        g.min_players = min_p;
        g.max_players = max_p;
        g.commit_duration_secs = args.commit_duration_secs.unwrap_or(15);
        g.reveal_duration_secs = args.reveal_duration_secs.unwrap_or(15);
        g.protect_lock_turns = args.protect_lock_turns.unwrap_or(1);
        g.bullets_cap = args.bullets_cap.unwrap_or(6);
        g.mirrors_cap = args.mirrors_cap.unwrap_or(1);
        g.loot_mode = 0;
        g.win_mode = 0;

        // ── Seed ──
        let match_id_bytes = args.match_id.to_le_bytes();
        let slot_bytes = clock.slot.to_le_bytes();
        let mut seed_input = Vec::with_capacity(8 + 32 * player_count as usize + 8);
        seed_input.extend_from_slice(&match_id_bytes);
        for i in 0..player_count as usize {
            seed_input.extend_from_slice(player_pubkeys[i].as_ref());
        }
        seed_input.extend_from_slice(&slot_bytes);
        let seed = solana_sha256_hasher::hashv(&[&seed_input]).to_bytes();

        // ── MatchState ──
        g.match_id = args.match_id;
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

        // ── Players ──
        g.player_count = player_count;
        for i in 0..player_count as usize {
            g.players[i] = player_pubkeys[i];
            g.bullets[i] = 1;
            g.mirrors[i] = 1;
            g.hits_received[i] = 0;
            g.protect_lock[i] = 0;
        }

        // ── TurnData (reset) ──
        g.committed = 0;
        g.revealed = 0;
        g.actions = [0; 4];
        g.targets = [255; 4];
        g.commit_hashes = [[0u8; 32]; 4];
        g.salts = [[0u8; 32]; 4];

        Ok(ctx.accounts)
    }

    #[system_input]
    pub struct Components {
        pub game_state: GameState,
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
