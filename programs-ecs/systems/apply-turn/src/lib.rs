use bolt_lang::*;
use game_config::{GameConfig, MAX_PLAYERS};
use player_state::PlayerState;

// TODO: regenerate keypair before first deploy —
//   `solana-keygen new -o target/deploy/apply_turn-keypair.json --no-bip39-passphrase`
//   `solana address -k target/deploy/apply_turn-keypair.json`
// Then paste here AND in `Anchor.toml`.
declare_id!("FpvpcEJCqRwCq4u4h3HgiX8ZhrseUtM8tSPkB8Mq77nv");

const STATUS_RUNNING: u8 = 1;
const STATUS_FINISHED: u8 = 2;
const PHASE_TURN: u8 = 0;
const PHASE_RESOLVE: u8 = 1;

const ACTION_NOOP: u8 = 0;
const NO_TARGET: u8 = 255;

// ── PlayerRegistry raw-byte layout (mirror resolve-turn) ──────────────
// match_id sits at offset 8 (right after the Anchor discriminator).
// player_states Vec data starts after match_id + Vec1 metadata.
const PR_MATCH_ID_OFFSET: usize = 8;
const PR_PLAYER_STATES_OFFSET: usize = 24 + 32 * MAX_PLAYERS;
const PR_COUNT_OFFSET: usize = 24 + 64 * MAX_PLAYERS;

// Bolt prepends one AccountInfo per #[system_input] component —
// GameConfig + PlayerState = 2 — so the PlayerRegistry sits at
// index NUM_COMPONENTS.
const NUM_COMPONENTS: usize = 2;

#[error_code]
pub enum GameError {
    #[msg("Not in the Resolve phase — call resolve-turn first")]
    NotInResolve,
    #[msg("Apply slot already consumed this turn")]
    AlreadyApplied,
    #[msg("Match status invalid for apply (must be Running or Finished)")]
    InvalidMatchStatus,
    #[msg("Invalid account size")]
    InvalidAccount,
    #[msg("PlayerRegistry.match_id doesn't match GameConfig.match_id — wrong registry for this match")]
    RegistryMatchIdMismatch,
    #[msg("PlayerState pubkey doesn't match the one registered for this seat")]
    PlayerStateMismatch,
    #[msg("Seat index out of bounds")]
    InvalidSeat,
}

/// Per-player APPLY pass — runs once per registered player after
/// `resolve-turn` has snapshotted the post-resolution state into
/// `GameConfig.pending_effects`. Copies the seat's snapshot onto the
/// PlayerState, resets the per-turn submission fields, and increments
/// `apply_count`. The LAST `apply-turn` invocation flips `GameConfig.phase`
/// back to `Turn` so the next `submit-action` round can begin.
///
/// Idempotent: the per-slot `applied` flag on `pending_effects` means a
/// re-call (e.g., cranker retry after a tx failure) is a no-op.
///
/// Security tie: takes PlayerRegistry as an extra account at
/// `remaining_accounts[NUM_COMPONENTS]` and verifies (a) the registry
/// is bound to the same Bolt entity as GameConfig and (b) the
/// PlayerState pubkey matches the one registered for this seat. Stops
/// cross-match account substitution attacks.
///
/// Auth: same as submit-action — Bolt entity-auth scopes the
/// PlayerState to its owner, no explicit authority check needed here.
#[system]
pub mod apply_turn {
    pub fn execute(ctx: Context<Components>, _args_p: Vec<u8>) -> Result<Components> {
        let g = &mut ctx.accounts.game_config;
        let p = &mut ctx.accounts.player_state;

        // ── Gates ─────────────────────────────────────────────────────
        // Apply runs in BOTH Running (mid-match resolve) and Finished
        // (last resolve of the game still needs per-player cleanup).
        require!(
            g.status == STATUS_RUNNING || g.status == STATUS_FINISHED,
            GameError::InvalidMatchStatus
        );
        require!(g.phase == PHASE_RESOLVE, GameError::NotInResolve);

        let seat = p.seat as usize;
        require!(seat < MAX_PLAYERS, GameError::InvalidSeat);

        // ── Cross-match anti-collision check.
        //    Bolt validates GameConfig + PlayerState individually, but
        //    NOT that they belong to the same match. Without this
        //    check an attacker could pass GameConfig from match A and
        //    PlayerState from match B (with matching seat) and apply
        //    A's pending_effects[seat] onto B's PlayerState.
        //
        //    Fix: chain through PlayerRegistry. We can't use
        //    bolt_metadata.entity (not exposed in Bolt 0.2.4), so the
        //    registry carries its own `match_id` field that init-game
        //    set from GameConfig.match_id. Two checks:
        //      1. Registry.match_id == GameConfig.match_id
        //      2. Registry.player_states[seat] == player_state.key()
        let registry_acc = &ctx.remaining_accounts[NUM_COMPONENTS];
        let registry_data = registry_acc.try_borrow_data()?;
        require!(
            registry_data.len() >= PR_COUNT_OFFSET + 1,
            GameError::InvalidAccount
        );
        let registry_match_id = u64::from_le_bytes(
            registry_data[PR_MATCH_ID_OFFSET..PR_MATCH_ID_OFFSET + 8]
                .try_into()
                .map_err(|_| GameError::InvalidAccount)?,
        );
        require!(
            registry_match_id == g.match_id,
            GameError::RegistryMatchIdMismatch
        );
        // Check the PlayerState pubkey matches what the registry has
        // for this seat.
        let ps_offset = PR_PLAYER_STATES_OFFSET + seat * 32;
        let registered_ps = &registry_data[ps_offset..ps_offset + 32];
        require!(
            p.key().as_ref() == registered_ps,
            GameError::PlayerStateMismatch
        );
        drop(registry_data);

        let effect = g.pending_effects[seat];
        require!(!effect.applied, GameError::AlreadyApplied);

        // ── Copy the snapshot onto the PlayerState. ───────────────────
        p.bullets       = effect.bullets;
        p.mirrors       = effect.mirrors;
        p.hits_received = effect.hits_received;
        p.protect_lock  = effect.protect_lock;
        p.alive         = effect.alive;

        // ── Reset per-turn submission fields. Dead seats also reset
        //    so the next turn's NOOP defaulting in resolve-turn sees
        //    a clean state. ───────────────────────────────────────────
        p.submitted   = false;
        p.action_type = ACTION_NOOP;
        p.target      = NO_TARGET;

        // ── Mark the slot consumed + bump the apply counter. ──────────
        g.pending_effects[seat].applied = true;
        g.apply_count = g.apply_count.saturating_add(1);

        // ── Last apply of the round: flip phase back to Turn so
        //    submit-action can resume. If status flipped to Finished
        //    by resolve-turn, leave phase=Resolve as a tombstone (no
        //    more submits anyway). ───────────────────────────────────
        if g.apply_count >= g.active_players && g.status == STATUS_RUNNING {
            g.phase = PHASE_TURN;
        }

        Ok(ctx.accounts)
    }

    #[system_input]
    pub struct Components {
        pub game_config: GameConfig,
        pub player_state: PlayerState,
    }
}
