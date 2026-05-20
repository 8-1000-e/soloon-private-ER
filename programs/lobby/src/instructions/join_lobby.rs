use anchor_lang::prelude::*;
use anchor_lang::system_program;

use crate::constants::*;
use crate::errors::LobbyError;
use crate::state::{LobbyAccount, Vault};

/// Player joins an open lobby by paying the entry fee into the vault.
/// Linear-scan dedup so a wallet can't claim two seats — at
/// MAX_PLAYERS=10 this is trivial work.
pub fn join_lobby(ctx: Context<JoinLobby>, _lobby_id: u64) -> Result<()> {
    let player_key = ctx.accounts.player.key();
    let entry_fee = ctx.accounts.lobby.entry_fee;

    // ── Gates (immutable borrow scope) ─────────────────────────────
    {
        let lobby = &ctx.accounts.lobby;
        require!(lobby.status == STATUS_OPEN, LobbyError::LobbyNotOpen);
        require!(
            (lobby.player_count as usize) < lobby.max_players as usize,
            LobbyError::LobbyFull
        );
        for i in 0..lobby.player_count as usize {
            require!(
                lobby.players[i] != player_key,
                LobbyError::AlreadyJoined
            );
        }
    }

    // ── Transfer entry_fee: player → vault PDA (system program CPI). ─
    system_program::transfer(
        CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            system_program::Transfer {
                from: ctx.accounts.player.to_account_info(),
                to:   ctx.accounts.vault.to_account_info(),
            },
        ),
        entry_fee,
    )?;

    // ── Append to roster + bump counters. ──────────────────────────
    let lobby = &mut ctx.accounts.lobby;
    let seat = lobby.player_count as usize;
    lobby.players[seat] = player_key;
    lobby.player_count = lobby.player_count.saturating_add(1);

    let vault = &mut ctx.accounts.vault;
    vault.total_pot = vault.total_pot.saturating_add(entry_fee);

    msg!(
        "Player {} joined lobby {} ({}/{}). Vault: {} lamports",
        player_key,
        lobby.lobby_id,
        lobby.player_count,
        lobby.max_players,
        vault.total_pot
    );
    Ok(())
}

#[derive(Accounts)]
#[instruction(lobby_id: u64)]
pub struct JoinLobby<'info> {
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

    pub system_program: Program<'info, System>,
}
