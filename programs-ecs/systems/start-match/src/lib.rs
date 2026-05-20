use bolt_lang::*;
use game_config::GameConfig;

// TODO: regenerate keypair before first deploy —
//   `solana-keygen new -o target/deploy/start_match-keypair.json --no-bip39-passphrase`
//   `solana address -k target/deploy/start_match-keypair.json`
// Then paste here AND in `Anchor.toml`.
declare_id!("DzxgjcQkfAW43Xt7gQcp9WBmr2vopb88UZBrX5wFMhqg");

#[error_code]
pub enum GameError {
    #[msg("Cannot start: game is not in the Waiting phase")]
    GameNotWaiting,
    #[msg("Cannot start: not enough players (active < min_players)")]
    NotEnoughPlayers,
    #[msg("Cannot start: turn_duration_secs not configured")]
    DurationNotConfigured,
}

/// Extra seconds added to turn 0's deadline so the back has slack to
/// notify players + open the input UI before `submit-action` calls
/// would start failing on `DeadlinePassed`. Subsequent turns get the
/// nominal `turn_duration_secs` only (set in `resolve-turn`).
const FIRST_TURN_GRACE_SECS: i64 = 10;

/// Locks the lobby and opens turn 0. Called by the lobby program (or
/// the back's cranker) once `spawn-player` has been called at least
/// `min_players` times.
///
/// State transition: `status: Waiting → Running`, `phase = Turn`,
/// `turn = 0`, `turn_deadline = now + turn_duration_secs`. Players can
/// start firing `submit-action` immediately after this lands.
///
/// Components touched: only `GameConfig`. `PlayerRegistry` and the
/// per-player `PlayerState` PDAs were already populated by
/// `spawn-player` and don't need touching here — their `alive` /
/// `submitted` defaults from spawn carry into turn 0.
#[system]
pub mod start_match {
    pub fn execute(ctx: Context<Components>, _args_p: Vec<u8>) -> Result<Components> {
        let g = &mut ctx.accounts.game_config;

        require!(g.status == 0, GameError::GameNotWaiting);
        require!(
            g.active_players >= g.min_players,
            GameError::NotEnoughPlayers
        );
        require!(g.turn_duration_secs > 0, GameError::DurationNotConfigured);

        let now = Clock::get()?.unix_timestamp;

        // ── Flip to RUNNING + open turn 0. ─────────────────────────────
        g.status          = 1;            // Running
        g.phase           = 0;            // Turn (players submit actions)
        g.turn            = 0;
        g.turn_deadline   = now + FIRST_TURN_GRACE_SECS + g.turn_duration_secs as i64;
        // Defensive: spawn-player never bumped these, but a re-call of
        // start-match on a mis-init'd state could leave stale counters.
        g.submitted_count = 0;

        Ok(ctx.accounts)
    }

    #[system_input]
    pub struct Components {
        pub game_config: GameConfig,
    }
}
