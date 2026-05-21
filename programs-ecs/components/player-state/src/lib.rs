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

declare_id!("CpHKf8pn8VvBMjJAzEePyBAgNJSxofBQLySi3uTMDfA3");

#[account]
#[derive(InitSpace)]
pub struct PlayerState {
    pub authority: Pubkey,
    pub seat: u8,
    pub alive: bool,
    pub bullets: u8,
    pub mirrors: u8,
    pub hits_received: u8,
    pub protect_lock: u8,
    pub submitted: bool,
    pub action_type: u8,
    pub target: u8,
    pub bolt_metadata: BoltMetadata,
}

pub struct PlayerStateInit {
    pub authority: Pubkey,
    pub seat: u8,
    pub alive: bool,
    pub bullets: u8,
    pub mirrors: u8,
    pub hits_received: u8,
    pub protect_lock: u8,
    pub submitted: bool,
    pub action_type: u8,
    pub target: u8,
}

impl PlayerState {
    pub fn new(init: PlayerStateInit) -> Self {
        Self {
            authority: init.authority,
            seat: init.seat,
            alive: init.alive,
            bullets: init.bullets,
            mirrors: init.mirrors,
            hits_received: init.hits_received,
            protect_lock: init.protect_lock,
            submitted: init.submitted,
            action_type: init.action_type,
            target: init.target,
            bolt_metadata: BoltMetadata::default(),
        }
    }
}

#[automatically_derived]
impl ComponentTraits for PlayerState {
    fn seed() -> &'static [u8] {
        "".as_bytes()
    }

    fn size() -> usize {
        8 + <PlayerState>::INIT_SPACE
    }
}

impl Default for PlayerState {
    fn default() -> Self {
        Self {
            authority: Pubkey::default(),
            seat: 0,
            alive: false,
            bullets: 0,
            mirrors: 0,
            hits_received: 0,
            protect_lock: 0,
            submitted: false,
            action_type: 0,
            target: 255,
            bolt_metadata: BoltMetadata::default(),
        }
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct MemberArg {
    pub flags: u8,
    pub pubkey: Pubkey,
}

#[delegate(PlayerState)]
#[bolt_program(PlayerState)]
pub mod player_state {
    use super::*;

    pub fn init_permission(
        ctx: Context<InitPermission>,
        members: Vec<MemberArg>,
    ) -> Result<()> {
        let entity_key = ctx.accounts.entity.key();
        let bump = ctx.bumps.component;
        let pda_seeds: &[&[u8]] = &[
            <PlayerState as ComponentTraits>::seed(),
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
            seeds = [<PlayerState as ComponentTraits>::seed(), entity.key().as_ref()],
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
