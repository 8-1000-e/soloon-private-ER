use bolt_lang::*;

declare_id!("DCwYD9hZjpL3RhnhYo3fLWby6YxahjeQ5Gd9ggsp9Mhi");

#[component(delegate)]
#[derive(Default)]
pub struct TurnData {
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
