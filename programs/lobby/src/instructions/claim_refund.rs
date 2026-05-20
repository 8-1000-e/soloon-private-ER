use anchor_lang::prelude::*;

use crate::constants::*;
use crate::errors::LobbyError;
use crate::state::{LobbyAccount, Vault};

/// Player reclaims their full entry fee from a cancelled lobby. Each
/// player calls this individually (the lobby program can't iterate
/// all players from one tx). Unlike `leave_lobby` there's no
/// LEAVE_FEE — the cancellation isn't the player's choice so they get
/// the full refund.
pub fn claim_refund(ctx: Context<ClaimRefund>, _lobby_id: u64) -> Result<()> {
    let player_key = ctx.accounts.player.key();

    // ── Find seat. Lobby must be Cancelled. ────────────────────────
    let player_index = {
        let lobby = &ctx.accounts.lobby;
        require!(lobby.status == STATUS_CANCELLED, LobbyError::LobbyNotOpen);
        let mut found = None;
        for i in 0..lobby.player_count as usize {
            if lobby.players[i] == player_key {
                found = Some(i);
                break;
            }
        }
        found.ok_or(LobbyError::PlayerNotFound)?
    };

    let entry_fee = ctx.accounts.lobby.entry_fee;

    // ── Vault PDA → player. ────────────────────────────────────────
    **ctx.accounts.vault.to_account_info().try_borrow_mut_lamports()? -= entry_fee;
    **ctx.accounts.player.to_account_info().try_borrow_mut_lamports()? += entry_fee;

    // ── Roster bookkeeping (swap-remove). ──────────────────────────
    let lobby = &mut ctx.accounts.lobby;
    let last = (lobby.player_count - 1) as usize;
    lobby.players[player_index] = lobby.players[last];
    lobby.players[last] = Pubkey::default();
    lobby.player_count -= 1;

    let vault = &mut ctx.accounts.vault;
    vault.total_pot = vault.total_pot.saturating_sub(entry_fee);

    msg!(
        "Refund: {} received {} lamports from cancelled lobby {}",
        player_key, entry_fee, lobby.lobby_id
    );
    Ok(())
}

#[derive(Accounts)]
#[instruction(lobby_id: u64)]
pub struct ClaimRefund<'info> {
    #[account(mut)]
    pub player: Signer<'info>,

    #[account(
        mut,
        seeds = [LOBBY_SEED, lobby_id.to_le_bytes().as_ref()],
        bump = lobby.bump
    )]
    pub lobby: Account<'info, LobbyAccount>,

    #[account(
        mut,
        seeds = [VAULT_SEED, lobby.key().as_ref()],
        bump = vault.bump
    )]
    pub vault: Account<'info, Vault>,
}
