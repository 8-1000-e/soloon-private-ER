use anchor_lang::prelude::*;

use crate::constants::*;
use crate::errors::LobbyError;
use crate::state::{LobbyAccount, Vault};

/// Authority recovers the rent of a settled lobby. Closes BOTH the
/// LobbyAccount and the Vault PDA — vault must be empty (= every prize
/// distributed / refund claimed) before this can run.
pub fn close_lobby(ctx: Context<CloseLobby>, _lobby_id: u64) -> Result<()> {
    let lobby = &ctx.accounts.lobby;
    require!(
        lobby.status == STATUS_FINISHED || lobby.status == STATUS_CANCELLED,
        LobbyError::NotClosable
    );
    require!(
        ctx.accounts.vault.total_pot == 0,
        LobbyError::VaultNotEmpty
    );

    msg!("Lobby {} closed, rent reclaimed to authority.", lobby.lobby_id);
    Ok(())
}

#[derive(Accounts)]
#[instruction(lobby_id: u64)]
pub struct CloseLobby<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    #[account(
        mut,
        seeds = [LOBBY_SEED, lobby_id.to_le_bytes().as_ref()],
        bump = lobby.bump,
        has_one = authority @ LobbyError::Unauthorized,
        close = authority
    )]
    pub lobby: Account<'info, LobbyAccount>,

    #[account(
        mut,
        seeds = [VAULT_SEED, lobby.key().as_ref()],
        bump = vault.bump,
        close = authority
    )]
    pub vault: Account<'info, Vault>,
}
