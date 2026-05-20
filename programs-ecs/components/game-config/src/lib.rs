use bolt_lang::*;

// NOTE: this is the ID inherited from the old unified `game-state` component
// we replaced — keypair file is `target/deploy/game_state-keypair.json`. If
// you regenerate it, also update `Anchor.toml` and the `use` declarations
// in every system that imports `game_config`.
declare_id!("F5Sm5SQ5tCyq1SM9U3QBUR2dEQNfeVdXztmduF17rrt4");

/// Match-level state for a Soloon round. One per match — attached to the
/// Bolt entity the lobby program records in `LobbyAccount.match_entity`.
///
/// Per-player state (alive flag, bullets, mirrors, current action…) lives
/// in `PlayerState` PDAs, indexed by `PlayerRegistry.player_states[seat]`.
/// This component carries ONLY config + match-level counters + match-end
/// outcome.
///
/// Designed to be delegated together with `PlayerRegistry` + every
/// `PlayerState` to the same Private Ephemeral Rollup validator, with the
/// permission account members set to the player authorities — so actions
/// stay confidential to outside observers.
///
/// Fully deterministic: no on-chain RNG seed, no commit-hash chain, no
/// final-state hash. The TEE-resident validator is the source of truth
/// for resolution; integrity comes from TEE attestation, not from a
/// hash transcript replayable off-chain.
#[component(delegate)]
pub struct GameConfig {
    // ── Identity ────────────────────────────────────────────────────────
    /// Unique match identifier (passed by the lobby program on start).
    pub match_id: u64,

    // ── Phase machine ───────────────────────────────────────────────────
    /// 0 = Waiting (lobby open, players can join), 1 = Running, 2 = Finished, 3 = Cancelled.
    pub status: u8,
    /// 0 = Turn (players submit actions, hidden inside the TEE), 1 = Resolve.
    pub phase: u8,
    /// Current turn number (capped at 100 — see resolve-turn timeout).
    pub turn: u16,

    // ── Player counters ─────────────────────────────────────────────────
    /// Active players registered in `PlayerRegistry`. Equals `player_registry.count`.
    pub active_players: u8,
    /// Players still alive this match.
    pub alive_count: u8,
    /// Players who have submitted their action for the current turn.
    /// Resolve-turn fires when this reaches `alive_count` (or after the
    /// deadline expires).
    pub submitted_count: u8,

    // ── Timing ──────────────────────────────────────────────────────────
    /// How long each turn lasts (seconds). Set on start-match.
    pub turn_duration_secs: u16,
    /// Unix-seconds deadline for the current turn. Resolve-turn auto-fires
    /// after this elapses even if not all players have submitted.
    pub turn_deadline: i64,

    // ── Match configuration ─────────────────────────────────────────────
    pub min_players: u8,
    pub max_players: u8,
    /// Turns remaining where Protect is locked after a hit.
    pub protect_lock_turns: u8,
    /// Max bullets a player can hold.
    pub bullets_cap: u8,
    /// Max mirrors a player can hold.
    pub mirrors_cap: u8,
    /// Reserved for future variants (e.g. random loot drops).
    pub loot_mode: u8,
    /// Reserved for future variants (e.g. team modes).
    pub win_mode: u8,

    // ── Outcome ─────────────────────────────────────────────────────────
    /// Winner authority pubkeys (up to `MAX_PLAYERS` if everyone dies on
    /// the same turn — see the `LastAlive` tie-break in resolve-turn).
    #[max_len(MAX_PLAYERS)]
    pub winners: Vec<[u8; 32]>,
    pub winner_count: u8,

    // ── Resolve-phase coordination ─────────────────────────────────────
    /// Per-seat resolution effects written by `resolve-turn` (the
    /// COMPUTE pass) and consumed by `apply-turn` (the per-player APPLY
    /// pass). Bolt only mutates components listed in `#[system_input]`,
    /// so we can't write to N `PlayerState`s atomically from one
    /// system — instead we snapshot the post-resolution state per seat
    /// here, then the cranker fires `apply-turn` once per player to
    /// land each PlayerState individually.
    ///
    /// Layout: `pending_effects[seat]` carries the FINAL values for
    /// that seat's PlayerState after the turn resolves. Indexed by
    /// `PlayerState.seat`, which is stable for the whole match.
    #[max_len(MAX_PLAYERS)]
    pub pending_effects: Vec<PendingEffect>,
    /// Number of `apply-turn` invocations completed for the current
    /// pending resolve. When this hits `active_players`, the apply
    /// pass is done; the LAST `apply-turn` call flips `phase` back to
    /// `Turn` and is the one that actually writes the next-turn state.
    pub apply_count: u8,
}

/// Post-resolution snapshot for one seat. Written wholesale by
/// `resolve-turn`; copied verbatim onto each PlayerState by
/// `apply-turn`. Carries the FINAL field values, not deltas — keeps
/// the apply logic a trivial memcpy with zero arithmetic.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, InitSpace)]
pub struct PendingEffect {
    pub bullets: u8,
    pub mirrors: u8,
    pub hits_received: u8,
    pub protect_lock: u8,
    /// New alive flag — `false` once the seat takes its third hit
    /// (or whatever death threshold the future variants pick).
    pub alive: bool,
    /// `true` once `apply-turn` has consumed this slot. Stops a
    /// re-call from double-applying the same effect.
    pub applied: bool,
}

impl Default for PendingEffect {
    fn default() -> Self {
        Self {
            bullets: 0,
            mirrors: 0,
            hits_received: 0,
            protect_lock: 0,
            alive: false,
            applied: false,
        }
    }
}

/// Cap on simultaneous players in one match. Soloon's mechanics (3-hit
/// death threshold, no-loot mirror cap, fast turns) were designed
/// around 2–4 player rounds — short matches keep tension high and a
/// 4-player wipe-simultané tie-break stays sane. Bumping this means
/// re-tuning death threshold + bullet cap to avoid drag.
pub const MAX_PLAYERS: usize = 4;

impl Default for GameConfig {
    fn default() -> Self {
        Self {
            match_id: 0,
            status: 0,
            phase: 0,
            turn: 0,
            active_players: 0,
            alive_count: 0,
            submitted_count: 0,
            turn_duration_secs: 0,
            turn_deadline: 0,
            min_players: 2,
            max_players: 4,
            protect_lock_turns: 1,
            bullets_cap: 6,
            mirrors_cap: 1,
            loot_mode: 0,
            win_mode: 0,
            winners: Vec::new(),
            winner_count: 0,
            pending_effects: vec![PendingEffect::default(); MAX_PLAYERS],
            apply_count: 0,
            bolt_metadata: BoltMetadata::default(),
        }
    }
}
