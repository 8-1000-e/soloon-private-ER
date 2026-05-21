use bolt_lang::*;
use game_config::GameConfig;
use player_state::PlayerState;

declare_id!("FrwwoEpAHoqqiNrH4kQsD4tZpsXNwKkuE3btDVcrqXdo");

// ── Phase + status enums (mirror game-config doc-comments) ────────────
const STATUS_RUNNING: u8 = 1;
const PHASE_TURN: u8 = 0;
const PHASE_RESOLVE: u8 = 1;

// ── Action enum (mirror player-state doc-comments) ────────────────────
const ACTION_NOOP: u8 = 0;
const ACTION_PROTECT: u8 = 1;
const ACTION_RELOAD: u8 = 2;
const ACTION_MIRROR: u8 = 3;
const ACTION_SHOOT: u8 = 4;

const NO_TARGET: u8 = 255;

#[error_code]
pub enum GameError {
    #[msg("Match not running")]            MatchNotRunning,
    #[msg("Not in the Turn phase")]        WrongPhase,
    #[msg("Player is dead")]               PlayerDead,
    #[msg("Already submitted this turn")]  AlreadySubmitted,
    #[msg("Turn deadline has passed")]     DeadlinePassed,
    #[msg("Invalid action type (0..=4)")]  InvalidAction,
    #[msg("Invalid target (out of bounds)")] InvalidTarget,
    #[msg("Cannot shoot self")]            CannotShootSelf,
}

/// Player-signed action submission for the current turn. Replaces the
/// commit-reveal pair the old design used — actions land in plaintext
/// inside the TEE-resident Private Ephemeral Rollup, so other players
/// can't read them off-chain until `resolve-turn` lands the effects.
///
/// Auth model (mirrors stay-calm / trade-fight):
///   * Bolt's CPI validates the entity ↔ PlayerState relationship — the
///     caller can only pass the PlayerState that belongs to the entity
///     they own, so no explicit `authority == player.authority` check
///     is needed.
///   * The back issues session keys scoped to *this* program only, so a
///     leaked session key can submit actions but not, say, drain the
///     lobby pot.
///
/// Target validation is "seat in-bounds + not self". The dead-target
/// check happens in `resolve-turn` — a target can flip dead between
/// submit time and resolve time (mirror reflection, simultaneous shots
/// landing on the same seat), so the canonical alive read lives there.
#[system]
pub mod submit_action {
    pub fn execute(ctx: Context<Components>, args_p: Vec<u8>) -> Result<Components> {
        let args: Args = parse_args(&args_p);

        // ── Match-level gates ──────────────────────────────────────────
        let g = &mut ctx.accounts.game_config;
        require!(g.status == STATUS_RUNNING, GameError::MatchNotRunning);
        require!(g.phase == PHASE_TURN, GameError::WrongPhase);
        require!(
            Clock::get()?.unix_timestamp <= g.turn_deadline,
            GameError::DeadlinePassed
        );

        // ── Player-level gates ────────────────────────────────────────
        let p = &mut ctx.accounts.player_state;
        require!(p.alive, GameError::PlayerDead);
        require!(!p.submitted, GameError::AlreadySubmitted);
        require!(args.action_type <= ACTION_SHOOT, GameError::InvalidAction);

        // ── Target validation (only meaningful for SHOOT) ─────────────
        let target = if args.action_type == ACTION_SHOOT {
            let t = args.target;
            require!(
                (t as u8) < g.active_players,
                GameError::InvalidTarget
            );
            require!(t != p.seat, GameError::CannotShootSelf);
            t
        } else {
            // Non-shoot actions ignore the target — coerce to the
            // canonical "none" sentinel so resolve-turn doesn't see
            // garbage left over from a prior turn.
            NO_TARGET
        };

        // ── Persist on PlayerState ────────────────────────────────────
        p.action_type = args.action_type;
        p.target      = target;
        p.submitted   = true;

        // ── Match progression: if everyone alive has now submitted,
        //    flip to Resolve so the cranker can fire resolve-turn
        //    without waiting for the deadline. ────────────────────────
        g.submitted_count = g.submitted_count.saturating_add(1);
        if g.submitted_count >= g.alive_count {
            g.phase = PHASE_RESOLVE;
        }

        // Silence dead-action constants (kept here as the canonical
        // numbering reference for resolve-turn / front clients).
        let _ = (ACTION_NOOP, ACTION_PROTECT, ACTION_RELOAD, ACTION_MIRROR);

        Ok(ctx.accounts)
    }

    #[system_input]
    pub struct Components {
        pub game_config: GameConfig,
        pub player_state: PlayerState,
    }

    #[arguments]
    struct Args {
        /// 0 = Noop, 1 = Protect, 2 = Reload, 3 = Mirror, 4 = Shoot.
        action_type: u8,
        /// Seat index of the shoot target. Ignored unless `action_type == 4`;
        /// the system normalises it to `255` (no-target sentinel) for the
        /// other actions before persisting.
        target: u8,
    }
}
