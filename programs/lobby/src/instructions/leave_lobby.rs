use anchor_lang::prelude::*;

use crate::constants::*;
use crate::errors::LobbyError;
use crate::state::{LobbyAccount, Vault};

/// Player bails out of an open lobby and gets `entry_fee - LEAVE_FEE`
/// back. The LEAVE_FEE stays in the vault (= adds to the eventual pot
/// for the players who stick around) — meant to discourage join/leave
/// spam, not to harvest from the leaver.
pub fn leave_lobby(ctx: Context<LeaveLobby>, _lobby_id: u64) -> Result<()> {
    let player_key = ctx.accounts.player.key();

    // ── Find the player's seat. ────────────────────────────────────
    let player_index = {
        let lobby = &ctx.accounts.lobby;
        require!(lobby.status == STATUS_OPEN, LobbyError::LobbyNotOpen);
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
    let refund = entry_fee.saturating_sub(LEAVE_FEE_LAMPORTS);

    // ── Vault PDA → player (direct lamport manipulation, vault is
    //    program-owned so we can sub_lamports freely). The leave fee
    //    stays in the vault. ──────────────────────────────────────
    **ctx.accounts.vault.to_account_info().try_borrow_mut_lamports()? -= refund;
    **ctx.accounts.player.to_account_info().try_borrow_mut_lamports()? += refund;

    // ── Roster bookkeeping: swap-remove keeps the slot 0..count
    //    range tightly packed. ─────────────────────────────────────
    let lobby = &mut ctx.accounts.lobby;
    let last = (lobby.player_count - 1) as usize;
    lobby.players[player_index] = lobby.players[last];
    lobby.players[last] = Pubkey::default();
    lobby.player_count -= 1;

    let vault = &mut ctx.accounts.vault;
    vault.total_pot = vault.total_pot.saturating_sub(refund);

    msg!(
        "Player {} left lobby {}. Refund: {} lamports. Vault: {} lamports",
        player_key, lobby.lobby_id, refund, vault.total_pot
    );
    Ok(())
}

#[derive(Accounts)]
#[instruction(lobby_id: u64)]
pub struct LeaveLobby<'info> {
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
