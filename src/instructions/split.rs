use std::mem::MaybeUninit;

use pinocchio::{account_info::AccountInfo, program_error::ProgramError, ProgramResult};
use solana_winternitz::signature::WinternitzSignature;

/*
    unline traditonal cryptography, winternitz signature become vulnerable after a single use.
    this split instruction allows you to:
    1. distribute payments across multiple recipients in one transaciton
    2. Roll over remaining funds to a new quantum vault with fresh keypair (by passing a
    quantum vault as the refund account)
*/
pub struct SplitVaultAccounts<'a> {
    pub vault: &'a AccountInfo, // source vault containing stored lamports (mutable)
    pub split: &'a AccountInfo, // recipient account for the spcified amount (mutable)
    pub refund: &'a AccountInfo, // Recipient account for remaining vault balance (mutable)
}

impl<'a> TryFrom<&'a [AccountInfo]> for SplitVaultAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo]) -> Result<Self, Self::Error> {
        let [vault, split, refund] = accounts else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        Ok(Self {
            vault,
            split,
            refund,
        })
    }
}

pub struct SplitVaultInstructionData {
    pub siganture: WinternitzSignature, // winterenitz signature proving ownership of the vault's keypair
    pub amount: [u8; 8],                // lamports to transfer to the split account
    pub bump: [u8; 1],                  // PDA derivation bump for optimization
}

impl<'a> TryFrom<&'a [u8]> for SplitVaultInstructionData {
    type Error = ProgramError;
    fn try_from(data: &'a [u8]) -> Result<Self, Self::Error> {
        if data.len() != core::mem::size_of::<SplitVaultInstructionData>() {
            return Err(ProgramError::InvalidInstructionData);
        };

        let mut signature_array = MaybeUninit::<[u8; 896]>::uninit();
        unsafe {
            core::ptr::copy_nonoverlapping(
                data[0..896].as_ptr(),
                signature_array.as_mut_ptr() as *mut u8,
                896,
            );
        }

        Ok(Self {
            siganture: WinternitzSignature::from(unsafe { signature_array.assume_init() }),
            bump: data[896..897]
                .try_into()
                .map_err(|_| ProgramError::InvalidInstructionData)?,
            amount: data[897..905]
                .try_into()
                .map_err(|_| ProgramError::InvalidInstructionData)?,
        })
    }
}

/*
    Why the signature is sent in the instruction data?
    -> In a Winternitz vault, the signature is not a byproduct of the transaction — it is the transaction’s authority.
    -> Show the secret that proves you’re allowed to do this.
*/

pub struct SplitVault<'a> {
    pub accounts: SplitVaultAccounts<'a>,
    pub instruction_data: SplitVaultInstructionData,
}

impl<'a> TryFrom<(&'a [u8], &'a [AccountInfo])> for SplitVault<'a> {
    type Error = ProgramError;

    fn try_from((data, accoutns): (&'a [u8], &'a [AccountInfo])) -> Result<Self, Self::Error> {
        let accounts = SplitVaultAccounts::try_from(accoutns)?;
        let instruction_data = SplitVaultInstructionData::try_from(data)?;

        Ok(Self {
            accounts,
            instruction_data,
        })
    }
}

impl<'a> SplitVault<'a> {
    pub const DISCRIMINATOR: &'a u8 = &1;

    /*
       The verification process follows these steps:
       Message Assembly: A 72-byte message is constructed containing: Amount to split, the split account publickey and the refund account publickey
       Signature Verification: The Winternitz signature is used to recover the original public key hash, which is then compared against the vault's PDA derivation seeds.
       PDA Validation: A fast equivalence check ensures the recovered hash matches the vault's PDA, proving the signer owns the vault.
       Fund Distribution If validation succeeds: the specified amount is transferred to the split account, the remaining balance is transferred to the refund account and the vault acount is closed.
    */

    pub fn process(&self) -> ProgramResult {
        // assemble our split message
        let mut message = [0u8; 72];
        message[0..8].clone_from_slice(&self.instruction_data.amount);
        message[8..40].clone_from_slice(self.accounts.split.key());
        message[40..].clone_from_slice(self.accounts.refund.key());

        // Recover pubkey from hash from the signature
        let hash = self
            .instruction_data
            .siganture
            .recover_pubkey(&message)
            .merklize();

        // Fast PDA equivalence check
        if solana_nostd_sha256::hashv(&[
            hash.as_ref(),
            self.instruction_data.bump.as_ref(),
            crate::ID.as_ref(),
            b"ProgramDerivedAddress",
        ])
        .ne(self.accounts.vault.key())
        {
            return Err(ProgramError::MissingRequiredSignature);
        }

        // Close vault, send split balance to Split account, refund remainder to refund account
        *self.accounts.split.try_borrow_mut_lamports()? +=
            u64::from_le_bytes(self.instruction_data.amount);
        *self.accounts.refund.try_borrow_mut_lamports()? += self
            .accounts
            .vault
            .lamports()
            .saturating_sub(u64::from_le_bytes(self.instruction_data.amount));
        self.accounts.vault.close()
    }
}
