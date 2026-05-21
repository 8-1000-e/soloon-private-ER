use bolt_lang::*;
use ephemeral_rollups_sdk::access_control::structs::Member as SdkMember;
use ephemeral_rollups_sdk::consts::{MAGIC_PROGRAM_ID, PERMISSION_PROGRAM_ID};
use anchor_lang::solana_program::{
    instruction::{AccountMeta, Instruction},
    program::invoke_signed,
};

// `MagicVau1t999999999999999999999999999999999` — see game-config for why
// the bytes are hard-coded instead of `pubkey!(…)`.
pub const EPHEMERAL_VAULT_ID: Pubkey = Pubkey::new_from_array([
    5, 69, 180, 36, 224, 197, 24, 97, 240, 41, 76, 112, 66, 34, 84, 78,
    202, 127, 133, 79, 194, 135, 136, 166, 123, 118, 113, 80, 62, 224, 143, 184,
]);

const CREATE_EPHEMERAL_PERMISSION_DISCRIMINATOR: u64 = 6;

declare_id!("8cKqpw8r4GwdKRFZFvDM95RB3C4kDgBLdtvCYNa7dwkw");

pub const MAX_PLAYERS: usize = 4;

#[account]
#[derive(InitSpace)]
pub struct PlayerRegistry {
    pub match_id: u64,
    #[max_len(MAX_PLAYERS)]
    pub players: Vec<[u8; 32]>,
    #[max_len(MAX_PLAYERS)]
    pub player_states: Vec<[u8; 32]>,
    pub count: u8,
    pub bolt_metadata: BoltMetadata,
}

pub struct PlayerRegistryInit {
    pub match_id: u64,
    pub players: Vec<[u8; 32]>,
    pub player_states: Vec<[u8; 32]>,
    pub count: u8,
}

impl PlayerRegistry {
    pub fn new(init: PlayerRegistryInit) -> Self {
        Self {
            match_id: init.match_id,
            players: init.players,
            player_states: init.player_states,
            count: init.count,
            bolt_metadata: BoltMetadata::default(),
        }
    }
}

#[automatically_derived]
impl ComponentTraits for PlayerRegistry {
    fn seed() -> &'static [u8] {
        "".as_bytes()
    }

    fn size() -> usize {
        8 + <PlayerRegistry>::INIT_SPACE
    }
}

impl Default for PlayerRegistry {
    fn default() -> Self {
        Self {
            match_id: 0,
            players: vec![[0u8; 32]; MAX_PLAYERS],
            player_states: vec![[0u8; 32]; MAX_PLAYERS],
            count: 0,
            bolt_metadata: BoltMetadata::default(),
        }
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct MemberArg {
    pub flags: u8,
    pub pubkey: Pubkey,
}

#[delegate(PlayerRegistry)]
#[bolt_program(PlayerRegistry)]
pub mod player_registry {
    use super::*;

    pub fn init_permission(
        ctx: Context<InitPermission>,
        members: Vec<MemberArg>,
    ) -> Result<()> {
        let entity_key = ctx.accounts.entity.key();
        let bump = ctx.bumps.component;
        let pda_seeds: &[&[u8]] = &[
            <PlayerRegistry as ComponentTraits>::seed(),
            entity_key.as_ref(),
            std::slice::from_ref(&bump),
        ];

        let mut args_bytes = Vec::with_capacity(1 + members.len() * 33);
        args_bytes.push(1u8); // is_private = true
        for m in &members {
            args_bytes.push(m.flags);
            args_bytes.extend_from_slice(m.pubkey.as_ref());
        }

        let mut data = Vec::with_capacity(8 + args_bytes.len());
        data.extend_from_slice(&CREATE_EPHEMERAL_PERMISSION_DISCRIMINATOR.to_le_bytes());
        data.extend_from_slice(&args_bytes);

        // See game-config for why the PDA is the CPI payer (delegated-
        // account requirement on the TEE).
        let accounts = vec![
            AccountMeta::new(ctx.accounts.component.key(), true),
            AccountMeta::new_readonly(ctx.accounts.component.key(), true),
            AccountMeta::new(ctx.accounts.permission.key(), false),
            AccountMeta::new(ctx.accounts.ephemeral_vault.key(), false),
            AccountMeta::new_readonly(ctx.accounts.magic_program.key(), false),
        ];
        let ix = Instruction {
            program_id: PERMISSION_PROGRAM_ID,
            accounts,
            data,
        };
        invoke_signed(
            &ix,
            &[
                ctx.accounts.component.to_account_info(),
                ctx.accounts.permission.to_account_info(),
                ctx.accounts.ephemeral_vault.to_account_info(),
                ctx.accounts.magic_program.to_account_info(),
            ],
            &[pda_seeds],
        )?;

        let _silence: Vec<SdkMember> = Vec::new();
        Ok(())
    }

    #[derive(Accounts)]
    pub struct InitPermission<'info> {
        #[account(mut)]
        pub payer: Signer<'info>,
        pub entity: Account<'info, Entity>,
        #[account(
            mut,
            seeds = [<PlayerRegistry as ComponentTraits>::seed(), entity.key().as_ref()],
            bump,
        )]
        /// CHECK: PDA verified by seeds + bump; CPI signer.
        pub component: UncheckedAccount<'info>,
        /// CHECK: validated by the permission-program CPI.
        #[account(mut)]
        pub permission: AccountInfo<'info>,
        /// CHECK: address-pinned to the magicblock ephemeral vault.
        #[account(mut, address = EPHEMERAL_VAULT_ID)]
        pub ephemeral_vault: UncheckedAccount<'info>,
        /// CHECK: address-pinned to the magicblock magic program.
        #[account(address = MAGIC_PROGRAM_ID)]
        pub magic_program: UncheckedAccount<'info>,
        /// CHECK: address-pinned to the magicblock permission program.
        #[account(address = PERMISSION_PROGRAM_ID)]
        pub permission_program: UncheckedAccount<'info>,
        pub system_program: Program<'info, System>,
    }
}
