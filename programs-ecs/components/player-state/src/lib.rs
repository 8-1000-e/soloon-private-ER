use bolt_lang::*;

// TODO: regenerate keypair before first deploy —
//   `solana-keygen new -o target/deploy/player_state-keypair.json`
//   `solana address -k target/deploy/player_state-keypair.json`
// Then paste the resulting pubkey here AND in `Anchor.toml`. The address
// below is a fresh placeholder so the workspace compiles.
declare_id!("CpHKf8pn8VvBMjJAzEePyBAgNJSxofBQLySi3uTMDfA3");

/// One PDA per player in a match. Holds the seat-local inventory, hit
/// counters, protect cooldown, and the action the player is submitting
/// this turn. Lives delegated inside the Private Ephemeral Rollup so the
/// `action_type` / `target` stay confidential to the opponents until
/// `resolve-turn` runs and writes the post-resolution effects.
///
/// The per-turn submission fields (`submitted`, `action_type`, `target`)
/// are reset by `resolve-turn` at the start of every new turn.
#[component(delegate)]
pub struct PlayerState {
    // ── Identity ────────────────────────────────────────────────────────
    /// Wallet that signs `submit-action` for this slot.
    pub authority: Pubkey,
    /// Index in `PlayerRegistry.players` / `player_states`. Used as the
    /// `target` value for Shoot actions and as the row index in the
    /// `actions` / `targets` arrays that `resolve-turn` walks.
    pub seat: u8,

    // ── Combat state ────────────────────────────────────────────────────
    /// Flipped to false the turn the player gets liquidated (≥ 3 hits or
    /// future variants).
    pub alive: bool,
    /// Loaded bullets. Capped by `game_config.bullets_cap` on every
    /// `Reload` / loot gain.
    pub bullets: u8,
    /// Mirror charges (consumed by `Mirror` action — single-use per
    /// charge). Capped by `game_config.mirrors_cap`.
    pub mirrors: u8,
    /// Total hits taken across the match. Player dies at `hits_received >= 3`.
    pub hits_received: u8,
    /// Turns remaining where `Protect` is disabled after taking a hit.
    /// Set to `game_config.protect_lock_turns` whenever a hit lands and
    /// counts down inside `resolve-turn`.
    pub protect_lock: u8,

    // ── Per-turn submission (cleared by resolve-turn) ──────────────────
    /// True once the player has called `submit-action` for the current
    /// turn. `resolve-turn` reads this to know whether to default the
    /// missing submitters to Noop.
    pub submitted: bool,
    /// 0 = Noop, 1 = Protect, 2 = Reload, 3 = Mirror, 4 = Shoot. Valid
    /// only when `submitted == true`; arbitrary garbage otherwise.
    pub action_type: u8,
    /// Seat index of the shoot target. `255` when the action carries no
    /// target (every action other than Shoot). Validated against the
    /// alive mask + `seat != self` at submit time.
    pub target: u8,
}

impl Default for PlayerState {
    fn default() -> Self {
        Self {
            authority: Pubkey::default(),
            seat: 0,
            alive: false,
            bullets: 0,
            mirrors: 0,
            hits_received: 0,
            protect_lock: 0,
            submitted: false,
            action_type: 0,
            target: 255,
            bolt_metadata: BoltMetadata::default(),
        }
    }
}
