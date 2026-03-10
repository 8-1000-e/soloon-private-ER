use bolt_lang::*;

declare_id!("Cv8E8HLY9u5GdJsWYd4AKfgvMRAjUrnxypdkQngi4Umd");

#[component(delegate)]
#[derive(Default)]
pub struct GameState {
    // ── MatchConfig fields ──────────────────────────────────────────────
    pub min_players: u8,
    pub max_players: u8,
    pub commit_duration_secs: u16,
    pub reveal_duration_secs: u16,
    pub protect_lock_turns: u8,
    pub bullets_cap: u8,
    pub mirrors_cap: u8,
    pub loot_mode: u8,
    pub win_mode: u8,

    // ── MatchState fields ───────────────────────────────────────────────
    pub match_id: u64,
    /// 0=Init, 1=Running, 2=Finished, 3=Cancelled
    pub status: u8,
    /// 0=Commit, 1=Reveal, 2=Resolve
    pub phase: u8,
    pub turn: u16,
    /// Bitset: bit i = 1 if seat i is alive (up to 4 seats)
    pub alive_mask: u16,
    pub alive_count: u8,
    pub seed: [u8; 32],
    pub commit_deadline: i64,
    pub reveal_deadline: i64,
    pub committed_count: u8,
    pub revealed_count: u8,
    /// Winner pubkeys (filled at match end)
    pub winners: [Pubkey; 4],
    pub winner_count: u8,
    pub final_state_hash: [u8; 32],
    /// Rolling hash across all turns for verifiability
    pub transcript_hash: [u8; 32],

    // ── Players fields ──────────────────────────────────────────────────
    pub players: [Pubkey; 4],
    pub player_count: u8,
    pub bullets: [u8; 4],
    pub mirrors: [u8; 4],
    pub hits_received: [u8; 4],
    /// Turns remaining where Protect is locked (set on hit)
    pub protect_lock: [u8; 4],

    // ── TurnData fields ─────────────────────────────────────────────────
    /// SHA256 commit hashes per seat
    pub commit_hashes: [[u8; 32]; 4],
    /// Bitset: bit i = 1 if seat i has committed
    pub committed: u16,
    /// Bitset: bit i = 1 if seat i has revealed
    pub revealed: u16,
    /// Action type per seat: 0=Noop, 1=Protect, 2=Reload, 3=Mirror, 4=Shoot
    pub actions: [u8; 4],
    /// Target seat for Shoot action (255 = no target)
    pub targets: [u8; 4],
    /// Salt used in commit hash per seat
    pub salts: [[u8; 32]; 4],
}
