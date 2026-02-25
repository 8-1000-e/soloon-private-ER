use bolt_lang::*;

declare_id!("35GEdGYpCaZfafmtQoea5pgz4Y2G2ciYQXjwDm7dCGYv");

#[component(delegate)]
#[derive(Default)]
pub struct MatchConfig {
    pub min_players: u8,
    pub max_players: u8,
    pub commit_duration_secs: u16,
    pub reveal_duration_secs: u16,
    pub protect_lock_turns: u8,
    pub bullets_cap: u8,
    pub mirrors_cap: u8,
    pub loot_mode: u8,
    pub win_mode: u8,
}
