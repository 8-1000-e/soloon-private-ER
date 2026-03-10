use bolt_lang::*;
use match_state::MatchState;
use players::Players;

declare_id!("63aHmRtezLwdybGnSLDYU679CTFUB9JMhQfbst1zA5sb");

const STATUS_FINISHED: u8 = 2;

#[error_code]
pub enum GameError {
    #[msg("Match not finished")]
    MatchNotFinished,
}

#[system]
pub mod end_match {

    pub fn execute(ctx: Context<Components>, _args_p: Vec<u8>) -> Result<Components> {
        let state = &ctx.accounts.match_state;
        require!(state.status == STATUS_FINISHED, GameError::MatchNotFinished);
        Ok(ctx.accounts)
    }

    #[system_input]
    pub struct Components {
        pub match_state: MatchState,
        pub players: Players,
    }
}
