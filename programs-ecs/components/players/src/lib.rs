use bolt_lang::*;

declare_id!("2mwqV9NyisXsRgKFQC62Ne1JhNLSYKxD2GuD75W7Rgcz");

#[component(delegate)]
#[derive(Default)]
pub struct Players {
    pub players: [Pubkey; 4],
    pub player_count: u8,
    pub bullets: [u8; 4],
    pub mirrors: [u8; 4],
    pub hits_received: [u8; 4],
    /// Turns remaining where Protect is locked (set on hit)
    pub protect_lock: [u8; 4],
}
