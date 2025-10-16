use aiken/collection/list
use aiken/crypto.{VerificationKeyHash}
use cardano/address.{Script}
use cardano/assets.{PolicyId}
use cardano/transaction.{OutputReference, Transaction}
use cocktail.{key_signed, output_inline_datum}

pub type DustMappingDatum {
  cardano_pub_key: VerificationKeyHash,
  dust_pub_key: ByteArray,
}

pub type Action {
  Update
  Deregister
}

validator cnight_mapping {
  mint(_redeemer: Data, policy_id: PolicyId, self: Transaction) {
    let Transaction { outputs, mint, extra_signatories, .. } = self

    let token_is_minted =
      assets.quantity_of(mint, policy_id, "DUST production auth token") == 1

    // Use list.filter here to ensure there's only one output to this script's address
    expect [output] =
      outputs
        |> list.filter(
            fn(output) {
              output.address.payment_credential == Script(policy_id)
            },
          )

    expect DustMappingDatum { cardano_pub_key, .. } =
      output_inline_datum(output)

    let owner_signed_tx = key_signed(extra_signatories, cardano_pub_key)

    owner_signed_tx && token_is_minted
  }

  spend(
    datum: Option<Data>,
    redeemer: Data,
    utxo: OutputReference,
    self: Transaction,
  ) {
    let Transaction { inputs, outputs, mint, extra_signatories, .. } = self
    expect Some(datum_data) = datum
    expect DustMappingDatum { cardano_pub_key, .. } = datum_data

    // Check for the owner's signature
    let signed_by_owner = key_signed(extra_signatories, cardano_pub_key)

    // Getting the script hash from this validator. Note that since the
    // `mint` handler is defined as part of the same validator, they share
    // the same hash digest. Thus, our `payment_credential` is ALSO our STT
    // minting policy.
    expect Some(own_input) =
      list.find(inputs, fn(input) { input.output_reference == utxo })
    expect Script(own_script_hash) = own_input.output.address.payment_credential

    expect action: Action = redeemer
    when action is {
      Update -> {
        // Find the output that continues the contract (sends funds back to this script)
        expect Some(continuing_output) =
          list.find(
            outputs,
            fn(output) {
              // The address must be the same as the input we are spending
              output.address == own_input.output.address
            },
          )

        // Check that the auth token is present in this continuing output
        let token_is_passed_through =
          assets.quantity_of(
            continuing_output.value,
            own_script_hash,
            "DUST production auth token",
          ) == 1

        // Get the new datum from the continuing output
        expect new_datum: DustMappingDatum =
          output_inline_datum(continuing_output)

        // CRITICAL: Ensure the owner in the new datum is the same as the old one
        let owner_is_unchanged = new_datum.cardano_pub_key == cardano_pub_key

        signed_by_owner && token_is_passed_through && owner_is_unchanged
      }
      Deregister -> {
        let token_is_burned =
          assets.quantity_of(
            mint,
            own_script_hash,
            "DUST production auth token",
          ) == -1

        signed_by_owner && token_is_burned
      }
    }
  }

  else(_) {
    fail
  }
}
