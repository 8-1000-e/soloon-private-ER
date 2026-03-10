use bolt_lang::*;
use match_config::MatchConfig;
use match_state::MatchState;
use players::Players;
use turn_data::TurnData;

declare_id!("3QSqmCakjVv3qGY4oD7hNzTHHE2LQQByU6Yj5iybkmxh");

const STATUS_RUNNING: u8 = 1;
const PHASE_REVEAL: u8 = 1;
const PHASE_RESOLVE: u8 = 2;
const ACTION_SHOOT: u8 = 4;

#[error_code]
pub enum GameError {
    #[msg("Player not in match")]
    NotInMatch,
    #[msg("Match not running")]
    MatchNotRunning,
    #[msg("Wrong phase")]
    WrongPhase,
    #[msg("Not committed")]
    NotCommitted,
    #[msg("Already revealed")]
    AlreadyRevealed,
    #[msg("Deadline passed")]
    DeadlinePassed,
    #[msg("Invalid salt length")]
    InvalidSaltLength,
    #[msg("Hash mismatch")]
    HashMismatch,
    #[msg("Invalid action")]
    InvalidAction,
    #[msg("Invalid target")]
    InvalidTarget,
    #[msg("Cannot shoot self")]
    CannotShootSelf,
    #[msg("Target is dead")]
    TargetDead,
}

#[system]
pub mod reveal_action {

    pub fn execute(ctx: Context<Components>, args_p: Vec<u8>) -> Result<Components> {
        let args: Args = parse_args(&args_p);
        let clock = Clock::get()?;
        let authority = *ctx.accounts.authority.key;

        let state = &mut ctx.accounts.match_state;
        let players = &ctx.accounts.players;
        let turn = &mut ctx.accounts.turn_data;

        // Find player seat
        let seat = find_seat(&players.players, &authority, players.player_count)
            .ok_or(error!(GameError::NotInMatch))?;

        // Validate game state
        require!(state.status == STATUS_RUNNING, GameError::MatchNotRunning);
        require!(state.phase == PHASE_REVEAL, GameError::WrongPhase);
        require!(turn.committed & (1 << seat) != 0, GameError::NotCommitted);
        require!(turn.revealed & (1 << seat) == 0, GameError::AlreadyRevealed);
        require!(
            clock.unix_timestamp <= state.reveal_deadline,
            GameError::DeadlinePassed
        );

        // Validate salt length
        require!(args.salt.len() == 32, GameError::InvalidSaltLength);
        let mut salt_arr = [0u8; 32];
        salt_arr.copy_from_slice(&args.salt);

        // Recompute commit hash and verify
        let match_id_bytes = state.match_id.to_le_bytes();
        let turn_bytes = state.turn.to_le_bytes();
        let recomputed = solana_sha256_hasher::hashv(&[
            &[args.action_type],
            &[args.target],
            &salt_arr,
            &match_id_bytes,
            &turn_bytes,
            authority.as_ref(),
        ])
        .to_bytes();

        require!(recomputed == turn.commit_hashes[seat], GameError::HashMismatch);

        // Validate action
        require!(args.action_type <= 4, GameError::InvalidAction);
        if args.action_type == ACTION_SHOOT {
            let target = args.target as usize;
            require!(target < players.player_count as usize, GameError::InvalidTarget);
            require!(target != seat, GameError::CannotShootSelf);
            require!(
                state.alive_mask & (1 << target) != 0,
                GameError::TargetDead
            );
        }

        // Store revealed action
        turn.actions[seat] = args.action_type;
        turn.targets[seat] = args.target;
        turn.salts[seat] = salt_arr;
        turn.revealed |= 1 << seat as u16;
        state.revealed_count += 1;

        // If all alive players revealed, advance to Resolve
        if state.revealed_count == state.alive_count {
            state.phase = PHASE_RESOLVE;
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
        action_type: u8,
        target: u8,
        salt: Vec<u8>,
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
