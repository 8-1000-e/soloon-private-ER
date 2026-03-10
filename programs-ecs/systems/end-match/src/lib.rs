use bolt_lang::*;
use game_state::GameState;

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
        let g = &ctx.accounts.game_state;
        require!(g.status == STATUS_FINISHED, GameError::MatchNotFinished);
        Ok(ctx.accounts)
    }

    #[system_input]
    pub struct Components {
        pub game_state: GameState,
    }
}
