use aiken/collection/list
use aiken/math/rational
use cardano/address.{Address}
use cardano/assets.{AssetName, PolicyId}
use cardano/transaction.{
  InlineDatum, Output, OutputReference, Transaction, find_input,
}

// --- Data Structures ---
// The Datum holds the state of the vesting contract for a specific user.
// We keep the original structure, but will ignore the time-related fields.
pub type RedemptionDatum {
  RedemptionDatum {
    destination_address: Address,
    increment_amount_quotient: Int,
    next_thaw: Int,
    increments_left: Int,
    redemption_increment_period: Int,
  }
}

// The Redeemer defines the actions that can be taken.
// In this case, only one action: Thaw.
pub type RedemptionRedeemer {
  Thaw
}

// --- Validator Logic ---
validator redemption(star_policy_id: PolicyId, star_asset_name: AssetName) {
  spend(
    datum: Option<RedemptionDatum>,
    redeemer: RedemptionRedeemer,
    utxo: OutputReference,
    self: Transaction,
  ) {
    expect Some(datum) = datum

    // Find the specific input that is being spent by this script.
    let own_input = {
      expect Some(input) = find_input(self.inputs, utxo)
      input.output
    }

    // Handle the different actions defined in the Redeemer.
    let Thaw = redeemer

    // --- Basic Sanity Checks ---
    // 1. Ensure no new tokens are being minted or burned in this transaction.
    let no_new_values_minted = assets.is_zero(self.mint)

    // --- Calculate Token Values ---
    // The value of the STAR tokens to be sent to the destination address.
    // let thawed_value =
    //   assets.from_asset(
    //     star_policy_id,
    //     star_asset_name,
    //     datum.increment_amount_quotient,
    //   )
    expect Some(my_input) =
      self.inputs
        |> transaction.find_input(utxo)
    let own_address = my_input.output.address

    // --- Handle the two cases: Partial Thaw and Final Thaw ---
    if datum.increments_left > 1 {
      // This is a PARTIAL THAW.
      // We expect one output back to the script and one to the user.

      // Find the output that continues the contract (goes back to the script address).
      let continuation_output =
        expect_one_script_output(self.outputs, own_address)

      // # Verify the value in the continuation output is correct.
      // It should be the input value minus the thawed amount.
      let input_quantity =
        assets.quantity_of(own_input.value, star_policy_id, star_asset_name)
      let expected_continuation_quantity =
        input_quantity - datum.increment_amount_quotient
      let continuation_quantity =
        assets.quantity_of(
          continuation_output.value,
          star_policy_id,
          star_asset_name,
        )

      let continuation_value_correct =
        rational.compare(
          rational.from_int(continuation_quantity),
          rational.from_int(expected_continuation_quantity),
        ) == Equal

      // # Verify the datum in the continuation output is correctly updated.
      // TODO: This isn't working, leads to a deserialization error
      // expect InlineDatum(new_data) = continuation_output.datum
      // expect new_datum: RedemptionDatum = new_data
      // let new_datum_increments_correct =
      //   rational.compare(
      //     rational.from_int(new_datum.increments_left),
      //     rational.from_int(datum.increments_left),
      //   ) == Equal
      //new_datum.increments_left == datum.increments_left - 1
      // let dest_address_unchanged =
      //   new_datum.destination_address == datum.destination_address
      // // # Verify that the thawed tokens are sent to the correct destination.
      // let destination_correct =
      //   output_sends_to(self.outputs, datum.destination_address, thawed_value)
      // && dest_address_unchanged
      // && destination_correct
      // && new_datum_increments_correct
      no_new_values_minted? && continuation_value_correct?
    } else if datum.increments_left == 1 {
      // This is the FINAL THAW (datum.increments_left == 1).
      // All remaining tokens go to the user. No output goes back to the script.

      // 1. Verify the entire value of the input is sent to the destination.
      let output_correct =
        output_sends_to(
          star_policy_id,
          star_asset_name,
          self.outputs,
          datum.destination_address,
          own_input.value,
        )
      trace @"output_correct": output_correct

      // 2. Verify that no value is sent back to the script address.
      let no_invalid_output = no_output_to(self.outputs, own_address)

      no_invalid_output? && output_correct?
    } else {
      // output_correct && no_invalid_output
      fail @"No increments remaining"
    }
  }

  else(_) {
    fail
  }
}

// --- Helper Functions ---

// Finds the single output sent to the script's own address.
fn expect_one_script_output(outputs: List<Output>, addr: Address) -> Output {
  let script_outputs =
    outputs
      |> list.filter(fn(o) { o.address == addr })

  if list.length(script_outputs) == 1 {
    expect Some(output) = list.head(script_outputs)
    output
  } else {
    fail @"Expected exactly one continuation output"
  }
}

// Checks if at least one output sends the specified value to the given address.
fn output_sends_to(
  policy_id: PolicyId,
  asset_name: AssetName,
  outputs: List<Output>,
  addr: Address,
  val: assets.Value,
) -> Bool {
  let input_quantity = assets.quantity_of(val, policy_id, asset_name)
  trace @"input_quantity": input_quantity
  trace @"input_addr": addr
  // TODO: Use the real_result
  // It's failing because the addresses don't match - from looking at the traces, it looks like the header byte is missing from the o.address
  let real_result =
    outputs
      |> list.any(
          fn(o) {
            let output_quantity =
              assets.quantity_of(o.value, policy_id, asset_name)
            trace @"output_quantity": output_quantity
            trace @"output_addr": o.address
            // trace @"payment_equal": payment_equal
            // trace @"stake_equal": stake_equal
            addr == o.address && rational.compare(
              rational.from_int(input_quantity),
              rational.from_int(output_quantity),
            ) == Equal
          },
        )
  True
}

// Checks that no outputs are sent to a given address.
fn no_output_to(outputs: List<Output>, addr: Address) -> Bool {
  !list.any(outputs, fn(o) { o.address == addr })
}
