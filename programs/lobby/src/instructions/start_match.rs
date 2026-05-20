use anchor_lang::prelude::*;

use crate::constants::*;
use crate::errors::LobbyError;
use crate::state::LobbyAccount;

/// Authority transitions the lobby from Open → Started and pins down
/// the Bolt entity that hosts the match's ECS state (GameConfig,
/// PlayerRegistry, N PlayerStates).
///
/// The actual ECS-side `start-match` system + the delegation flow that
/// puts every component on the Private ER are NOT done here — they're
/// separate CPIs the back orchestrates after this returns. This call
/// just locks the roster so joins/leaves stop being accepted.
pub fn start_match(
    ctx: Context<StartMatch>,
    _lobby_id: u64,
    match_entity: Pubkey,
) -> Result<()> {
    let lobby = &mut ctx.accounts.lobby;

    require!(lobby.status == STATUS_OPEN, LobbyError::LobbyNotOpen);
    require!(
        lobby.player_count as usize >= MIN_PLAYERS,
        LobbyError::NotEnoughPlayers
    );

    lobby.status = STATUS_STARTED;
    lobby.match_entity = match_entity;
    lobby.started_at = Clock::get()?.unix_timestamp;

    msg!(
        "Lobby {} → match started. Entity: {}. {} players, vault locked.",
        lobby.lobby_id, match_entity, lobby.player_count
    );
    Ok(())
}

#[derive(Accounts)]
#[instruction(lobby_id: u64)]
pub struct StartMatch<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    #[account(
        mut,
        seeds = [LOBBY_SEED, lobby_id.to_le_bytes().as_ref()],
        bump = lobby.bump,
        has_one = authority @ LobbyError::Unauthorized
    )]
    pub lobby: Account<'info, LobbyAccount>,
}
