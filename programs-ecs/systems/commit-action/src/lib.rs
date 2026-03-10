use bolt_lang::*;
use game_state::GameState;

declare_id!("EtF5LQ9tAfbBD7ez4GxP3S4AbTRFZupdw6PFLB4qtfL4");

const STATUS_RUNNING: u8 = 1;
const PHASE_COMMIT: u8 = 0;
const PHASE_REVEAL: u8 = 1;

#[error_code]
pub enum GameError {
    #[msg("Player not in match")]  NotInMatch,
    #[msg("Match not running")]    MatchNotRunning,
    #[msg("Wrong phase")]          WrongPhase,
    #[msg("Player is dead")]       PlayerDead,
    #[msg("Already committed")]    AlreadyCommitted,
    #[msg("Deadline passed")]      DeadlinePassed,
    #[msg("Invalid hash length")]  InvalidHashLength,
}

#[system]
pub mod commit_action {
    pub fn execute(ctx: Context<Components>, args_p: Vec<u8>) -> Result<Components> {
        let args: Args = parse_args(&args_p);
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
        require!(args.commit_hash.len() == 32, GameError::InvalidHashLength);

        let mut hash_arr = [0u8; 32];
        hash_arr.copy_from_slice(&args.commit_hash);

        g.commit_hashes[seat] = hash_arr;
        g.committed |= 1 << seat as u16;
        g.committed_count += 1;

        if g.committed_count == g.alive_count {
            g.phase = PHASE_REVEAL;
            g.reveal_deadline = clock.unix_timestamp + g.reveal_duration_secs as i64;
        }

        Ok(ctx.accounts)
    }

    #[system_input]
    pub struct Components {
        pub game_state: GameState,
    }

    #[arguments]
    struct Args { commit_hash: Vec<u8> }
}

fn find_seat(players: &[Pubkey; 4], authority: &Pubkey, count: u8) -> Option<usize> {
    for i in 0..count as usize {
        if players[i] == *authority { return Some(i); }
    }
    None
}
