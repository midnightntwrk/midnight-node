use cardano/address
use cardano/assets
use cardano/transaction.{InlineDatum, Input}
use cnight_mapping.{Deregister, DustMappingDatum, Update}
use mocktail.{
  add_datum, add_input, add_output, complete, mint, mock_output,
  mock_pub_key_hash, mock_tx_hash, mock_utxo_ref, mocktail_tx,
  required_signer_hash, tx_in, tx_in_inline_datum,
}

// Import the validator we want to test

// --- Common Test Data ---
const tx_hash = mock_tx_hash(0)

const owner = mock_pub_key_hash(0)

const attacker = mock_pub_key_hash(1)

const attacker_address = address.from_verification_key(attacker)

const dust_addr = "mock_dust_address"

const dust_addr_updated = "updated_mock_dust_address"

// This is the validator we are testing.
// Since mint and spend are in the same script, the validator hash is the policy id.

const policy_id = #"77ea856b811981e676bb7ab77702111ff36bccad722a00cc722be43e"

const script_address = address.from_script(policy_id)

const auth_token_name = "DUST production auth token"

// A mock UTxO sitting at the script address, containing the auth token
const script_utxo = mock_utxo_ref(0, 1)

const input_datum =
  DustMappingDatum { cardano_pub_key: owner, dust_pub_key: dust_addr }

// --- Minting Tests ---

test test_mint_success() {
  let output =
    mock_output(
      address.from_script(policy_id),
      assets.from_asset(policy_id, auth_token_name, 1),
      InlineDatum(input_datum),
      None,
    )

  let tx =
    mocktail_tx()
      |> mint(True, 1, policy_id, auth_token_name)
      |> required_signer_hash(True, owner)
      |> complete()
      |> add_output(True, output)
      |> add_datum(True, input_datum)

  cnight_mapping.cnight_mapping.mint(Void, policy_id, tx)
}

test test_mint_fails_if_not_signed() fail {
  let output =
    mock_output(
      address.from_script(policy_id),
      assets.from_asset(policy_id, auth_token_name, 1),
      InlineDatum(input_datum),
      None,
    )

  let tx =
    mocktail_tx()
      |> mint(True, 1, policy_id, auth_token_name)
      |> required_signer_hash(True, attacker)
      // Signed by wrong person
      |> complete()
      |> add_output(True, output)
      |> add_datum(True, input_datum)

  cnight_mapping.cnight_mapping.mint(Void, policy_id, tx)
}

test test_mint_fails_if_sent_to_wrong_address() fail {
  let output =
    mock_output(
      // Wrong address
      attacker_address,
      assets.from_asset(policy_id, auth_token_name, 1),
      InlineDatum(input_datum),
      None,
    )

  let tx =
    mocktail_tx()
      |> mint(True, 1, policy_id, auth_token_name)
      |> required_signer_hash(True, owner)
      |> complete()
      |> add_output(True, output)
      |> add_datum(True, input_datum)

  cnight_mapping.cnight_mapping.mint(Void, policy_id, tx)
}

test test_mint_fails_if_wrong_quantity_minted() fail {
  let output =
    mock_output(
      script_address,
      // Mints 2 instead of 1
      assets.from_asset(policy_id, auth_token_name, 2),
      InlineDatum(input_datum),
      None,
    )

  let tx =
    mocktail_tx()
      |> mint(True, 2, policy_id, auth_token_name)
      |> required_signer_hash(True, owner)
      |> complete()
      |> add_output(True, output)
      |> add_datum(True, input_datum)

  cnight_mapping.cnight_mapping.mint(Void, policy_id, tx)
}

// --- Spending Tests (Deregister) ---

test test_deregister_success() {
  let output =
    mock_output(
      address.from_script(policy_id),
      assets.from_asset(policy_id, auth_token_name, 1),
      InlineDatum(input_datum),
      None,
    )

  let input = Input { output_reference: script_utxo, output }

  let tx =
    mocktail_tx()
      |> tx_in(True, tx_hash, 0, assets.zero, script_address)
      |> tx_in_inline_datum(True, input_datum)
      |> mint(True, -1, policy_id, auth_token_name)
      // Burn the token
      |> required_signer_hash(True, owner)
      |> complete()
      |> add_input(True, input)

  let input_data: Data = input_datum
  cnight_mapping.cnight_mapping.spend(
    Some(input_data),
    Deregister,
    script_utxo,
    tx,
  )
}

test test_deregister_fails_if_not_signed() fail {
  let output =
    mock_output(
      address.from_script(policy_id),
      assets.from_asset(policy_id, auth_token_name, 1),
      InlineDatum(input_datum),
      None,
    )

  let input = Input { output_reference: script_utxo, output }

  let tx =
    mocktail_tx()
      |> tx_in(True, tx_hash, 0, assets.zero, script_address)
      |> tx_in_inline_datum(True, input_datum)
      |> mint(True, -1, policy_id, auth_token_name)
      |> required_signer_hash(True, attacker)
      // Signed by wrong person
      |> complete()
      |> add_input(True, input)

  let input_data: Data = input_datum
  cnight_mapping.cnight_mapping.spend(
    Some(input_data),
    Deregister,
    script_utxo,
    tx,
  )
}

test test_deregister_fails_if_token_not_burned() fail {
  let output =
    mock_output(
      address.from_script(policy_id),
      assets.from_asset(policy_id, auth_token_name, 1),
      InlineDatum(input_datum),
      None,
    )

  let input = Input { output_reference: script_utxo, output }

  let tx =
    mocktail_tx()
      |> tx_in(True, tx_hash, 0, assets.zero, script_address)
      |> tx_in_inline_datum(True, input_datum)
      // Missing token burn (mint: -1)
      |> required_signer_hash(True, owner)
      |> complete()
      |> add_input(True, input)

  let input_data: Data = input_datum
  cnight_mapping.cnight_mapping.spend(
    Some(input_data),
    Deregister,
    script_utxo,
    tx,
  )
}

// --- Spending Tests (Update) ---

test test_update_success() {
  let updated_datum =
    DustMappingDatum {
      cardano_pub_key: owner,
      // Owner is the same
      dust_pub_key: dust_addr_updated,
    }

  let new_output =
    mock_output(
      address.from_script(policy_id),
      assets.from_asset(policy_id, auth_token_name, 1),
      InlineDatum(updated_datum),
      None,
    )

  let output =
    mock_output(
      address.from_script(policy_id),
      assets.from_asset(policy_id, auth_token_name, 1),
      InlineDatum(input_datum),
      None,
    )

  let input = Input { output_reference: script_utxo, output }

  let tx =
    mocktail_tx()
      |> tx_in(True, tx_hash, 0, assets.zero, script_address)
      |> tx_in_inline_datum(True, input_datum)
      |> required_signer_hash(True, owner)
      |> complete()
      |> add_input(True, input)
      |> add_output(True, new_output)

  let input_data: Data = input_datum
  cnight_mapping.cnight_mapping.spend(Some(input_data), Update, script_utxo, tx)
}

test test_update_fails_if_owner_changed() fail {
  let updated_datum =
    DustMappingDatum {
      cardano_pub_key: attacker,
      // Owner is the same
      dust_pub_key: dust_addr_updated,
    }

  let new_output =
    mock_output(
      address.from_script(policy_id),
      assets.from_asset(policy_id, auth_token_name, 1),
      InlineDatum(updated_datum),
      None,
    )

  let output =
    mock_output(
      address.from_script(policy_id),
      assets.from_asset(policy_id, auth_token_name, 1),
      InlineDatum(input_datum),
      None,
    )

  let input = Input { output_reference: script_utxo, output }

  let tx =
    mocktail_tx()
      |> tx_in(True, tx_hash, 0, assets.zero, script_address)
      |> tx_in_inline_datum(True, input_datum)
      |> required_signer_hash(True, owner)
      |> complete()
      |> add_input(True, input)
      |> add_output(True, new_output)

  let input_data: Data = input_datum
  cnight_mapping.cnight_mapping.spend(Some(input_data), Update, script_utxo, tx)
}
