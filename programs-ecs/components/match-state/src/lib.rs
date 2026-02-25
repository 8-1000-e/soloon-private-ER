use bolt_lang::*;

declare_id!("AKnxJJqzHKy99bD2gqM5YdjSHMEbwDYrk69mzd2pb52Y");

#[component(delegate)]
#[derive(Default)]
pub struct MatchState {
    pub match_id: u64,
    /// 0=Init, 1=Running, 2=Finished, 3=Cancelled
    pub status: u8,
    /// 0=Commit, 1=Reveal, 2=Resolve
    pub phase: u8,
    pub turn: u16,
    /// Bitset: bit i = 1 if seat i is alive (supports up to 4 seats in lower 4 bits)
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
}
