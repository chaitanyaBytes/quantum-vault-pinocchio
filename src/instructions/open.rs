use pinocchio::{
    account_info::AccountInfo,
    instruction::{Seed, Signer},
    program_error::ProgramError,
    sysvars::{rent::Rent, Sysvar},
    ProgramResult,
};
use pinocchio_system::instructions::CreateAccount;

pub struct OpenVaultAccounts<'a> {
    pub payer: &'a AccountInfo,
    pub vault: &'a AccountInfo,
}

impl<'a> TryFrom<&'a [AccountInfo]> for OpenVaultAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo]) -> Result<Self, Self::Error> {
        let [payer, vault, _system_program] = accounts else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        Ok(Self { payer, vault })
    }
}

pub struct OpenVaultInstructionData {
    pub hash: [u8; 32], // SHA-256 hash of the user's winternitz keypair publick key
    pub bump: [u8; 1],  // PDA derivation bump passed from client (u8)
}

impl<'a> TryFrom<&'a [u8]> for OpenVaultInstructionData {
    type Error = ProgramError;

    fn try_from(data: &'a [u8]) -> Result<Self, Self::Error> {
        if data.len() != core::mem::size_of::<OpenVaultInstructionData>() {
            return Err(ProgramError::InvalidInstructionData);
        }

        let hash = data[0..32]
            .try_into()
            .map_err(|_| ProgramError::InvalidInstructionData)?;
        let bump = data[32..33]
            .try_into()
            .map_err(|_| ProgramError::InvalidInstructionData)?;

        Ok(Self { hash, bump })
    }
}

pub struct OpenVault<'a> {
    pub accounts: OpenVaultAccounts<'a>,
    pub instruction_data: OpenVaultInstructionData,
}

impl<'a> TryFrom<(&'a [u8], &'a [AccountInfo])> for OpenVault<'a> {
    type Error = ProgramError;

    fn try_from((data, accounts): (&'a [u8], &'a [AccountInfo])) -> Result<Self, Self::Error> {
        let instruction_data = OpenVaultInstructionData::try_from(data)?;
        let accounts = OpenVaultAccounts::try_from(accounts)?;

        Ok(Self {
            accounts,
            instruction_data,
        })
    }
}

impl<'a> OpenVault<'a> {
    pub const DISCRIMINATOR: &'a u8 = &0;

    pub fn process(&self) -> ProgramResult {
        let lamports = Rent::get()?.minimum_balance(0);
        let seeds = [
            Seed::from(&self.instruction_data.hash),
            Seed::from(&self.instruction_data.bump),
        ];

        CreateAccount {
            from: self.accounts.payer,
            to: self.accounts.vault,
            lamports,
            space: 0,
            owner: &crate::ID,
        }
        .invoke_signed(&[Signer::from(&seeds)])
    }
}
