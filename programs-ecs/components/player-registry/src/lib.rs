use bolt_lang::*;

// TODO: regenerate keypair before first deploy —
//   `solana-keygen new -o target/deploy/player_registry-keypair.json`
//   `solana address -k target/deploy/player_registry-keypair.json`
// Then paste the resulting pubkey here AND in `Anchor.toml`. The address
// below is a fresh placeholder so the workspace compiles; do not deploy
// it as-is or it will collide with whoever else picked the same string.
declare_id!("8cKqpw8r4GwdKRFZFvDM95RB3C4kDgBLdtvCYNa7dwkw");

/// Per-match cap on simultaneous players. Mirrors `game_config::MAX_PLAYERS`
/// (must stay in lockstep — both crates compute byte offsets from this).
/// Soloon is designed around 2–4 player rounds; bumping this requires
/// re-tuning death threshold + bullet cap to keep matches snappy.
pub const MAX_PLAYERS: usize = 4;

/// Parallel-arrays registry of every player in this match. The seat index
/// (= position in both vectors) is the player's stable handle for the
/// entire match — used as the `target` value for Shoot actions and the
/// row index in the `actions` / `targets` arrays inside `resolve-turn`.
///
/// Fields:
///   `match_id`         = mirror of `GameConfig.match_id`; written once by
///                        `init-game` and used by resolve-turn / apply-turn
///                        to defend against cross-match account swaps.
///                        Bolt 0.2.4's `BoltMetadata` only carries
///                        `authority` (no entity field), so we ship the
///                        cross-component identity tie ourselves.
///   `players[i]`       = player authority pubkey (signs every submit-action)
///   `player_states[i]` = PlayerState PDA for that player
///   `count`            = number of slots actively used (Vecs are pre-sized
///                        to MAX_PLAYERS so positional writes work the same
///                        way the trade-fight pattern does).
///
/// Why Vec rather than `[[u8; 32]; MAX_PLAYERS]`: Borsh deserializes a Vec
/// element-by-element on the stack instead of allocating the whole array
/// at once, keeping the auto-generated `update` / `update_with_session`
/// methods inside the BPF 4 KB stack frame budget.
#[component(delegate)]
pub struct PlayerRegistry {
    pub match_id: u64,
    #[max_len(MAX_PLAYERS)]
    pub players: Vec<[u8; 32]>,
    #[max_len(MAX_PLAYERS)]
    pub player_states: Vec<[u8; 32]>,
    pub count: u8,
}

impl Default for PlayerRegistry {
    fn default() -> Self {
        Self {
            match_id: 0,
            players: vec![[0u8; 32]; MAX_PLAYERS],
            player_states: vec![[0u8; 32]; MAX_PLAYERS],
            count: 0,
            bolt_metadata: BoltMetadata::default(),
        }
    }
}
