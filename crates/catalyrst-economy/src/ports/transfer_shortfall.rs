use alloy::primitives::{Address, U256};
use alloy::sol_types::SolCall;

use crate::ports::abi::{self, balanceOfCall, transferCall};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransferIntent {
    pub user: Address,
    pub to: Address,
    pub amount: U256,
}

pub fn transfer_intent(full_data: &[u8]) -> Option<TransferIntent> {
    let user = abi::decode_meta_tx_user_address(full_data)?;
    let inner = abi::decode_meta_tx(full_data)?;
    let call = transferCall::abi_decode(&inner).ok()?;
    Some(TransferIntent {
        user,
        to: call.to,
        amount: call.amount,
    })
}

pub fn balance_of_calldata(owner: Address) -> String {
    format!(
        "0x{}",
        alloy::hex::encode(balanceOfCall { owner }.abi_encode())
    )
}

pub fn decode_balance(result_hex: &str) -> Option<U256> {
    let bytes = alloy::hex::decode(result_hex.trim().trim_start_matches("0x")).ok()?;
    balanceOfCall::abi_decode_returns(&bytes).ok()
}

pub fn shortfall_message(intent: &TransferIntent, balance: U256) -> Option<String> {
    if balance >= intent.amount {
        return None;
    }
    Some(format!(
        "the transfer of {} wei from {:#x} exceeds its token balance of {} wei (short by {} wei)",
        intent.amount,
        intent.user,
        balance,
        intent.amount - balance
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::abi::executeMetaTransactionCall;
    use alloy::primitives::{address, B256};

    const USER: Address = address!("0x951bb66ce4a5d4b1c667e386af5313753d14ba2e");
    const PAY_TO: Address = address!("0x2a7fa4cad84d8ca921bd6c04f4462d99e35db6a2");

    fn meta_transfer(amount: u128) -> Vec<u8> {
        let inner = transferCall {
            to: PAY_TO,
            amount: U256::from(amount),
        }
        .abi_encode();
        executeMetaTransactionCall {
            userAddress: USER,
            functionSignature: inner.into(),
            sigR: B256::ZERO,
            sigS: B256::ZERO,
            sigV: 27,
        }
        .abi_encode()
    }

    #[test]
    fn decodes_the_transfer_wrapped_in_a_meta_transaction() {
        let intent = transfer_intent(&meta_transfer(1_250)).unwrap();
        assert_eq!(
            intent,
            TransferIntent {
                user: USER,
                to: PAY_TO,
                amount: U256::from(1_250u64)
            }
        );
    }

    #[test]
    fn ignores_meta_transactions_that_do_not_wrap_a_transfer() {
        let other = executeMetaTransactionCall {
            userAddress: USER,
            functionSignature: vec![0xaa, 0xbb, 0xcc, 0xdd].into(),
            sigR: B256::ZERO,
            sigS: B256::ZERO,
            sigV: 27,
        }
        .abi_encode();
        assert!(transfer_intent(&other).is_none());
        assert!(transfer_intent(&[0u8; 3]).is_none());
    }

    #[test]
    fn balance_of_calldata_targets_the_user() {
        let data = balance_of_calldata(USER);
        assert!(data.starts_with("0x70a08231"));
        assert!(data.ends_with("951bb66ce4a5d4b1c667e386af5313753d14ba2e"));
        assert_eq!(data.len(), 2 + 8 + 64);
    }

    #[test]
    fn decodes_a_padded_balance_and_rejects_garbage() {
        let hex = format!("0x{:064x}", 5_000_000_000_000_000_000u128);
        assert_eq!(
            decode_balance(&hex).unwrap(),
            U256::from(5_000_000_000_000_000_000u128)
        );
        assert!(decode_balance("0x1234").is_none());
        assert!(decode_balance("zz").is_none());
    }

    #[test]
    fn names_both_amounts_only_when_the_wallet_is_short() {
        let intent = transfer_intent(&meta_transfer(39_580_378_408_756_389_366)).unwrap();
        let msg = shortfall_message(&intent, U256::ZERO).unwrap();
        assert!(msg.contains("39580378408756389366 wei"));
        assert!(msg.contains("balance of 0 wei"));
        assert!(msg.contains("0x951bb66ce4a5d4b1c667e386af5313753d14ba2e"));
        assert!(shortfall_message(&intent, intent.amount).is_none());
        assert!(shortfall_message(&intent, intent.amount + U256::from(1u64)).is_none());
    }
}
