use anchor_lang::prelude::*;

use crate::constants::*;
use crate::errors::LobbyError;
use crate::state::LobbyAccount;

/// Authority kills a lobby BEFORE the match starts. Status flips to
/// Cancelled, players reclaim their entry fee through `claim_refund`.
///
/// Only allowed in OPEN: once `start_match` has fired, the ECS layer
/// owns the match state independently of the lobby. If we let cancel
/// run in Started, the lobby would flag Cancelled while
/// submit-action / resolve-turn keep running on-chain — players would
/// race the lobby's claim_refund against the ECS payout path,
/// possibly double-paying or stranding the pot. If we ever need a
/// mid-match bail-out, build a dedicated `abort_match` flow that
/// pauses the ECS first (e.g. by flipping `GameConfig.status` via a
/// privileged CPI), then lets the lobby cancel.
pub fn cancel_lobby(ctx: Context<CancelLobby>, _lobby_id: u64) -> Result<()> {
    let lobby = &mut ctx.accounts.lobby;
    require!(lobby.status == STATUS_OPEN, LobbyError::LobbyNotOpen);

    lobby.status = STATUS_CANCELLED;
    msg!("Lobby {} cancelled — players can call claim_refund.", lobby.lobby_id);
    Ok(())
}

#[derive(Accounts)]
#[instruction(lobby_id: u64)]
pub struct CancelLobby<'info> {
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
