use bolt_lang::*;
use match_config::MatchConfig;
use match_state::MatchState;
use players::Players;
use turn_data::TurnData;

declare_id!("EtF5LQ9tAfbBD7ez4GxP3S4AbTRFZupdw6PFLB4qtfL4");

const STATUS_RUNNING: u8 = 1;
const PHASE_COMMIT: u8 = 0;
const PHASE_REVEAL: u8 = 1;

#[error_code]
pub enum GameError {
    #[msg("Player not in match")]
    NotInMatch,
    #[msg("Match not running")]
    MatchNotRunning,
    #[msg("Wrong phase")]
    WrongPhase,
    #[msg("Player is dead")]
    PlayerDead,
    #[msg("Already committed")]
    AlreadyCommitted,
    #[msg("Deadline passed")]
    DeadlinePassed,
    #[msg("Invalid hash length")]
    InvalidHashLength,
}

#[system]
pub mod commit_action {

    pub fn execute(ctx: Context<Components>, args_p: Vec<u8>) -> Result<Components> {
        let args: Args = parse_args(&args_p);
        let clock = Clock::get()?;
        let authority = *ctx.accounts.authority.key;

        let config = &ctx.accounts.match_config;
        let state = &mut ctx.accounts.match_state;
        let players = &ctx.accounts.players;
        let turn = &mut ctx.accounts.turn_data;

        // Find player seat
        let seat = find_seat(&players.players, &authority, players.player_count)
            .ok_or(error!(GameError::NotInMatch))?;

        // Validate game state
        require!(state.status == STATUS_RUNNING, GameError::MatchNotRunning);
        require!(state.phase == PHASE_COMMIT, GameError::WrongPhase);
        require!(state.alive_mask & (1 << seat) != 0, GameError::PlayerDead);
        require!(turn.committed & (1 << seat) == 0, GameError::AlreadyCommitted);
        require!(
            clock.unix_timestamp <= state.commit_deadline,
            GameError::DeadlinePassed
        );

        // Validate commit_hash length
        require!(args.commit_hash.len() == 32, GameError::InvalidHashLength);
        let mut hash_arr = [0u8; 32];
        hash_arr.copy_from_slice(&args.commit_hash);

        // Store commit hash
        turn.commit_hashes[seat] = hash_arr;
        turn.committed |= 1 << seat as u16;
        state.committed_count += 1;

        // If all alive players committed, advance to Reveal
        if state.committed_count == state.alive_count {
            state.phase = PHASE_REVEAL;
            state.reveal_deadline =
                clock.unix_timestamp + config.reveal_duration_secs as i64;
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

    #[arguments]
    struct Args {
        commit_hash: Vec<u8>,
    }
}

fn find_seat(players: &[Pubkey; 4], authority: &Pubkey, count: u8) -> Option<usize> {
    for i in 0..count as usize {
        if players[i] == *authority {
            return Some(i);
        }
    }
    None
}
