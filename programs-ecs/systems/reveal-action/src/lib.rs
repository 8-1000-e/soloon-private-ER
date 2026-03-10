use bolt_lang::*;
use game_state::GameState;

declare_id!("3QSqmCakjVv3qGY4oD7hNzTHHE2LQQByU6Yj5iybkmxh");

const STATUS_RUNNING: u8 = 1;
const PHASE_REVEAL: u8 = 1;
const PHASE_RESOLVE: u8 = 2;
const ACTION_SHOOT: u8 = 4;

#[error_code]
pub enum GameError {
    #[msg("Player not in match")]  NotInMatch,
    #[msg("Match not running")]    MatchNotRunning,
    #[msg("Wrong phase")]          WrongPhase,
    #[msg("Not committed")]        NotCommitted,
    #[msg("Already revealed")]     AlreadyRevealed,
    #[msg("Deadline passed")]      DeadlinePassed,
    #[msg("Invalid salt length")]  InvalidSaltLength,
    #[msg("Hash mismatch")]        HashMismatch,
    #[msg("Invalid action")]       InvalidAction,
    #[msg("Invalid target")]       InvalidTarget,
    #[msg("Cannot shoot self")]    CannotShootSelf,
    #[msg("Target is dead")]       TargetDead,
}

#[system]
pub mod reveal_action {
    pub fn execute(ctx: Context<Components>, args_p: Vec<u8>) -> Result<Components> {
        let args: Args = parse_args(&args_p);
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
        require!(args.salt.len() == 32, GameError::InvalidSaltLength);

        let mut salt_arr = [0u8; 32];
        salt_arr.copy_from_slice(&args.salt);

        // Recompute commit hash and verify
        let match_id_bytes = g.match_id.to_le_bytes();
        let turn_bytes = g.turn.to_le_bytes();
        let recomputed = solana_sha256_hasher::hashv(&[
            &[args.action_type],
            &[args.target],
            &salt_arr,
            &match_id_bytes,
            &turn_bytes,
            authority.as_ref(),
        ]).to_bytes();

        require!(recomputed == g.commit_hashes[seat], GameError::HashMismatch);
        require!(args.action_type <= 4, GameError::InvalidAction);

        if args.action_type == ACTION_SHOOT {
            let target = args.target as usize;
            require!(target < g.player_count as usize, GameError::InvalidTarget);
            require!(target != seat, GameError::CannotShootSelf);
            require!(g.alive_mask & (1 << target) != 0, GameError::TargetDead);
        }

        g.actions[seat] = args.action_type;
        g.targets[seat] = args.target;
        g.salts[seat] = salt_arr;
        g.revealed |= 1 << seat as u16;
        g.revealed_count += 1;

        if g.revealed_count == g.alive_count {
            g.phase = PHASE_RESOLVE;
        }

        Ok(ctx.accounts)
    }

    #[system_input]
    pub struct Components {
        pub game_state: GameState,
    }

    #[arguments]
    struct Args { action_type: u8, target: u8, salt: Vec<u8> }
}

fn find_seat(players: &[Pubkey; 4], authority: &Pubkey, count: u8) -> Option<usize> {
    for i in 0..count as usize {
        if players[i] == *authority { return Some(i); }
    }
    None
}
