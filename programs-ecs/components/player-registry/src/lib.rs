use bolt_lang::*;

// TODO: regenerate keypair before first deploy —
//   `solana-keygen new -o target/deploy/player_registry-keypair.json`
//   `solana address -k target/deploy/player_registry-keypair.json`
// Then paste the resulting pubkey here AND in `Anchor.toml`. The address
// below is a fresh placeholder so the workspace compiles; do not deploy
// it as-is or it will collide with whoever else picked the same string.
declare_id!("REGENME11111111111111111111111111111111111A");

/// Per-match cap on simultaneous players. Two upper bounds force this:
///   1. Bolt CPI return-data ceiling (1024 B) — at ~64 B per slot
///      (`[u8; 32]` authority + `[u8; 32]` PlayerState PDA), 10 fits with
///      headroom for `count`.
///   2. BPF 4 KB stack frame — Borsh deserializes Vec element-by-element
///      so this stays well under, but anything > ~16 starts getting tight
///      once `resolve-turn` reads everything in one frame.
/// Re-export from `game_config` so both crates stay in lockstep.
pub const MAX_PLAYERS: usize = 10;

/// Parallel-arrays registry of every player in this match. The seat index
/// (= position in both vectors) is the player's stable handle for the
/// entire match — used as the `target` value for Shoot actions and the
/// row index in the `actions` / `targets` arrays inside `resolve-turn`.
///
/// `players[i]`        = player authority pubkey (signs every submit-action)
/// `player_states[i]`  = PlayerState PDA for that player
/// `count`             = number of slots actively used (Vecs are pre-sized
///                       to MAX_PLAYERS so positional writes work the same
///                       way the trade-fight pattern does).
///
/// Why Vec rather than `[[u8; 32]; MAX_PLAYERS]`: Borsh deserializes a Vec
/// element-by-element on the stack instead of allocating the whole array
/// at once, keeping the auto-generated `update` / `update_with_session`
/// methods inside the BPF 4 KB stack frame budget.
#[component(delegate)]
pub struct PlayerRegistry {
    #[max_len(MAX_PLAYERS)]
    pub players: Vec<[u8; 32]>,
    #[max_len(MAX_PLAYERS)]
    pub player_states: Vec<[u8; 32]>,
    pub count: u8,
}

impl Default for PlayerRegistry {
    fn default() -> Self {
        Self {
            players: vec![[0u8; 32]; MAX_PLAYERS],
            player_states: vec![[0u8; 32]; MAX_PLAYERS],
            count: 0,
            bolt_metadata: BoltMetadata::default(),
        }
    }
}
