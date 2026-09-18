use flatbuffers::{FlatBufferBuilder, WIPOffset};
use plerkle_serialization::{
    error::PlerkleSerializationError, CompiledInnerInstruction, CompiledInnerInstructionArgs,
    CompiledInnerInstructions, CompiledInnerInstructionsArgs, CompiledInstruction,
    CompiledInstructionArgs, Pubkey as FlatbufferPubkey, TransactionInfo, TransactionInfoArgs,
    TransactionVersion,
};
use solana_message::VersionedMessage;
use solana_transaction_status::{
    option_serializer::OptionSerializer, EncodedConfirmedTransactionWithStatusMeta, UiInstruction,
};

/// Plerkle's v3 schema predates transaction v1, but its enum is represented as
/// an open `i8` on the wire. Value 2 extends the existing Legacy (0) and V0 (1)
/// values without changing the FlatBuffer layout.
pub const PLERKLE_TRANSACTION_VERSION_V1: TransactionVersion = TransactionVersion(2);

/// Serialize an RPC transaction into the Plerkle transaction stream format.
///
/// This is the v1-aware counterpart to Plerkle v3's serializer. The published
/// Plerkle crate is pinned to Solana 3.x and therefore cannot decode v1's wire
/// layout or match `VersionedMessage::V1`.
pub fn serialize_encoded_transaction_with_status<'a>(
    mut builder: FlatBufferBuilder<'a>,
    transaction: EncodedConfirmedTransactionWithStatusMeta,
) -> Result<FlatBufferBuilder<'a>, PlerkleSerializationError> {
    let meta = transaction.transaction.meta.ok_or_else(|| {
        PlerkleSerializationError::SerializationError(
            "Missing meta data for transaction".to_string(),
        )
    })?;
    let decoded_transaction = transaction
        .transaction
        .transaction
        .decode()
        .ok_or_else(|| {
            PlerkleSerializationError::SerializationError(
                "Transaction cannot be decoded".to_string(),
            )
        })?;
    let message = decoded_transaction.message;

    let account_keys = {
        let mut keys = message
            .static_account_keys()
            .iter()
            .map(|key| FlatbufferPubkey(key.to_bytes()))
            .collect::<Vec<_>>();

        if message.address_table_lookups().is_some() {
            if let OptionSerializer::Some(loaded_addresses) = &meta.loaded_addresses {
                for key in loaded_addresses
                    .writable
                    .iter()
                    .chain(&loaded_addresses.readonly)
                {
                    let mut output = [0_u8; 32];
                    bs58::decode(key).into(&mut output).map_err(|error| {
                        PlerkleSerializationError::SerializationError(error.to_string())
                    })?;
                    keys.push(FlatbufferPubkey(output));
                }
            }
        }

        (!keys.is_empty()).then(|| builder.create_vector(&keys))
    };

    let log_messages = if let OptionSerializer::Some(log_messages) = &meta.log_messages {
        let messages = log_messages
            .iter()
            .map(|message| builder.create_string(message))
            .collect::<Vec<_>>();
        Some(builder.create_vector(&messages))
    } else {
        None
    };

    let inner_instructions = if let OptionSerializer::Some(inner_instruction_groups) =
        &meta.inner_instructions
    {
        let mut groups = Vec::with_capacity(inner_instruction_groups.len());
        for inner_instruction_group in inner_instruction_groups {
            let mut instructions = Vec::with_capacity(inner_instruction_group.instructions.len());
            for instruction in &inner_instruction_group.instructions {
                let UiInstruction::Compiled(instruction) = instruction else {
                    continue;
                };
                let accounts = Some(builder.create_vector(&instruction.accounts));
                let data = bs58::decode(&instruction.data)
                    .into_vec()
                    .map_err(|error| {
                        PlerkleSerializationError::SerializationError(error.to_string())
                    })?;
                let data = Some(builder.create_vector(&data));
                let compiled_instruction = CompiledInstruction::create(
                    &mut builder,
                    &CompiledInstructionArgs {
                        program_id_index: instruction.program_id_index,
                        accounts,
                        data,
                    },
                );
                instructions.push(CompiledInnerInstruction::create(
                    &mut builder,
                    &CompiledInnerInstructionArgs {
                        compiled_instruction: Some(compiled_instruction),
                        // Available in RPC metadata but unused by DAS consumers.
                        stack_height: 0,
                    },
                ));
            }

            let instructions = Some(builder.create_vector(&instructions));
            groups.push(CompiledInnerInstructions::create(
                &mut builder,
                &CompiledInnerInstructionsArgs {
                    index: inner_instruction_group.index,
                    instructions,
                },
            ));
        }
        Some(builder.create_vector(&groups))
    } else {
        let empty: Vec<WIPOffset<CompiledInnerInstructions<'_>>> = Vec::new();
        Some(builder.create_vector(&empty))
    };

    let outer_instructions = if message.instructions().is_empty() {
        None
    } else {
        let mut instructions = Vec::with_capacity(message.instructions().len());
        for instruction in message.instructions() {
            let accounts = Some(builder.create_vector(&instruction.accounts));
            let data = Some(builder.create_vector(&instruction.data));
            instructions.push(CompiledInstruction::create(
                &mut builder,
                &CompiledInstructionArgs {
                    program_id_index: instruction.program_id_index,
                    accounts,
                    data,
                },
            ));
        }
        Some(builder.create_vector(&instructions))
    };

    let version = match message {
        VersionedMessage::Legacy(_) => TransactionVersion::Legacy,
        VersionedMessage::V0(_) => TransactionVersion::V0,
        VersionedMessage::V1(_) => PLERKLE_TRANSACTION_VERSION_V1,
    };

    let signature = decoded_transaction.signatures.first().ok_or_else(|| {
        PlerkleSerializationError::SerializationError(
            "Transaction is missing its first signature".to_string(),
        )
    })?;
    let signature = builder.create_string(&signature.to_string());
    let transaction_info = TransactionInfo::create(
        &mut builder,
        &TransactionInfoArgs {
            is_vote: false,
            account_keys,
            log_messages,
            inner_instructions: None,
            outer_instructions,
            slot: transaction.slot,
            seen_at: 0,
            slot_index: None,
            signature: Some(signature),
            compiled_inner_instructions: inner_instructions,
            version,
        },
    );

    builder.finish(transaction_info, None);
    Ok(builder)
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{prelude::BASE64_STANDARD, Engine};
    use solana_message::{compiled_instruction::CompiledInstruction, v1, MessageHeader};
    use solana_sdk::{hash::Hash, pubkey::Pubkey, signature::Signature};
    use solana_transaction_status::{
        EncodedTransaction, EncodedTransactionWithStatusMeta, TransactionBinaryEncoding,
        TransactionStatusMeta,
    };

    #[test]
    fn serializes_large_v1_accounts_instructions_and_version() {
        let payer = Pubkey::new_unique();
        let program = Pubkey::new_unique();
        let instruction_data = vec![7; 1_500];
        let message = v1::Message::new(
            MessageHeader {
                num_required_signatures: 1,
                num_readonly_signed_accounts: 0,
                num_readonly_unsigned_accounts: 1,
            },
            v1::TransactionConfig::empty()
                .with_compute_unit_limit(20_000)
                .with_loaded_accounts_data_size_limit(65_536),
            Hash::new_unique(),
            vec![payer, program],
            vec![CompiledInstruction {
                program_id_index: 1,
                accounts: vec![0],
                data: instruction_data.clone(),
            }],
        );
        let transaction = solana_sdk::transaction::VersionedTransaction {
            signatures: vec![Signature::default()],
            message: VersionedMessage::V1(message),
        };
        let serialized_transaction = wincode::serialize(&transaction).unwrap();
        assert!(serialized_transaction.len() > 1_232);
        let encoded = EncodedTransaction::Binary(
            BASE64_STANDARD.encode(serialized_transaction),
            TransactionBinaryEncoding::Base64,
        );
        let transaction = EncodedConfirmedTransactionWithStatusMeta {
            slot: 42,
            transaction: EncodedTransactionWithStatusMeta {
                transaction: encoded,
                meta: Some(TransactionStatusMeta::default().into()),
                version: Some(solana_sdk::transaction::TransactionVersion::Number(1)),
            },
            block_time: None,
            transaction_index: None,
        };

        let builder =
            serialize_encoded_transaction_with_status(FlatBufferBuilder::new(), transaction)
                .unwrap();
        let serialized =
            plerkle_serialization::root_as_transaction_info(builder.finished_data()).unwrap();

        assert_eq!(serialized.version(), PLERKLE_TRANSACTION_VERSION_V1);
        assert_eq!(serialized.slot(), 42);
        let keys = serialized.account_keys().unwrap();
        assert_eq!(keys.len(), 2);
        assert_eq!(keys.get(0).0, payer.to_bytes());
        assert_eq!(keys.get(1).0, program.to_bytes());
        let instructions = serialized.outer_instructions().unwrap();
        let instruction = instructions.get(0);
        assert_eq!(instruction.program_id_index(), 1);
        assert_eq!(
            instruction.accounts().unwrap().iter().collect::<Vec<_>>(),
            vec![0]
        );
        assert_eq!(
            instruction.data().unwrap().iter().collect::<Vec<_>>(),
            instruction_data
        );
    }
}
