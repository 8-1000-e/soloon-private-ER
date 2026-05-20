use bolt_lang::*;
use game_config::GameConfig;

declare_id!("8k8tVsZR2NvB7cAPzcJeyiq7rckBCL6MDQ2kBAGKXK3s");

const STATUS_FINISHED: u8 = 2;
const PHASE_RESOLVE: u8 = 1;

#[error_code]
pub enum GameError {
    #[msg("Match not finished — call resolve-turn until status = Finished first")]
    MatchNotFinished,
    #[msg("Apply pass incomplete — every player must call apply-turn before end-match")]
    ApplyPassIncomplete,
}

/// Tombstone assertion called by the lobby program (or off-chain
/// indexer) AFTER `resolve-turn` has set the match to Finished and
/// EVERY player has run `apply-turn`. Verifies the match is in a clean
/// terminal state — no half-applied effects, no zombie turn fields —
/// before the lobby distributes the pot via `distribute_prize`.
///
/// No state mutations: `resolve-turn` already wrote the winners list,
/// final alive_count, and flipped status to Finished. `apply-turn`
/// already cleaned up each PlayerState. All this system does is yell
/// if the lobby tries to call distribute before either of those is
/// complete.
#[system]
pub mod end_match {
    pub fn execute(ctx: Context<Components>, _args_p: Vec<u8>) -> Result<Components> {
        let g = &ctx.accounts.game_config;

        require!(g.status == STATUS_FINISHED, GameError::MatchNotFinished);

        // Belt-and-suspenders: the final apply-turn pass must have
        // completed too, otherwise per-player state on PlayerStates is
        // out of sync with what GameConfig says (some seat may still
        // show submitted=true / wrong bullets / etc.). The lobby's
        // distribute_prize only reads GameConfig.winners so this isn't
        // a payout safety issue per se, but it keeps the audit trail
        // self-consistent: when status=Finished is observed by an
        // indexer, every PlayerState is guaranteed to reflect the
        // match's final state.
        //
        // Note: when status==Finished, phase stays at Resolve as a
        // tombstone (apply-turn doesn't flip to Turn for the final
        // resolve). So phase==Resolve is fine; we only need to check
        // that every seat has been applied.
        require!(g.phase == PHASE_RESOLVE, GameError::MatchNotFinished);
        require!(
            g.apply_count >= g.active_players,
            GameError::ApplyPassIncomplete
        );

        Ok(ctx.accounts)
    }

    #[system_input]
    pub struct Components {
        pub game_config: GameConfig,
    }
}
