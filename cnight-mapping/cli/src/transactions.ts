import { LucidEvolution, getAddressDetails, toHex, TxSignBuilder, UTxO, applyDoubleCborEncoding, Script, validatorToScriptHash, validatorToAddress } from '@lucid-evolution/lucid';
import { toJson } from './utils.js';
import { logger } from './logger.js';

import blueprint from "../../plutus.json" with { type: "json" };

const cnightMappingSkeleton = blueprint.validators.find(
  (v) => v.title === "cnight_mapping.cnight_mapping.mint",
);

export const mappingValidator: Script = {
  type: "PlutusV3",
  script: applyDoubleCborEncoding(cnightMappingSkeleton!.compiledCode),
};

export const mappingValidatorPolicyId = validatorToScriptHash(mappingValidator);

// Stateless utility class - no singleton, no internal state
export class DustTransactionsUtils {
  /**
   * Build the DUST address registration transaction (Step 03) - Pure function
   */
  static async buildRegistrationTransaction(lucid: LucidEvolution, dustPKH: string): Promise<TxSignBuilder> {
    const { Data, Constr } = await import('@lucid-evolution/lucid');
    const mappingValidatorAddress = validatorToAddress(lucid.config().network!, mappingValidator);

    logger.log('[DustTransactions]', '🔧 Building DUST Address Registration Transaction...');


    // Get environment variables for registration
    const cardanoAddress = await lucid.wallet().address();
    const cardanoPKH = getAddressDetails(cardanoAddress)?.paymentCredential?.hash;

    if (!dustPKH) {
      logger.error('[DustTransactions]', '❌ DUST PKH not configured');
      throw new Error('DUST_PKH must be configured for registration');
    }

    logger.log(
      '[DustTransactions]',
      '📋 Registration Configuration:',
      toJson({
        dustPKH,
        cardanoAddress,
      })
    );

    // Build the transaction according to Step 03 specification
    logger.log('[DustTransactions]', '🔨 Building registration transaction...');
    const txBuilder = lucid.newTx();

    // MINT: DUST Auth Token Policy - 1 token with specific token name
    const dustAuthTokenRedeemer = Data.to(new Constr(0, [])); // Constructor 0, empty fields
    const dustTokenName = toHex(new TextEncoder().encode('DUST production auth token'));
    const dustAuthTokenAssetName = mappingValidatorPolicyId + dustTokenName;

    logger.log(
      '[DustTransactions]',
      '🪙 Minting DUST Auth Token (Main Policy):',
      toJson({
        policyId: mappingValidatorPolicyId,
        tokenName: 'DUST production auth token',
        assetName: dustAuthTokenAssetName,
        amount: 1n,
      })
    );

    txBuilder.mintAssets({ [dustAuthTokenAssetName]: 1n }, dustAuthTokenRedeemer);

    // OUTPUT: DUST Mapping Validator with Registration Datum
    // The DUST PKH is encoded as the bytes of the hex string representation
    const registrationDatumData = new Constr(0, [
      cardanoPKH!, // Cardano PKH (28 bytes hex string)
      toHex(new TextEncoder().encode(dustPKH)), // DUST PKH encoded as bytes of the string representation
    ]);

    const serializedRegistrationDatum = Data.to(registrationDatumData);

    logger.log(
      '[DustTransactions]',
      '📤 Creating output to DUST Mapping Validator:',
      toJson({
        address: mappingValidatorAddress,
        assets: {
          lovelace: 1586080n, // ADA amount
          [dustAuthTokenAssetName]: 1n, // Using minting policy token
        },
        datumData: registrationDatumData,
        datumCBORHEX: serializedRegistrationDatum,
      })
    );

    txBuilder.pay.ToContract(
      mappingValidatorAddress, // DUST Mapping Validator Address
      { kind: 'inline', value: serializedRegistrationDatum }, // Registration Datum (INLINE)
      {
        lovelace: 1586080n, // Minimum ADA for UTxO
        [dustAuthTokenAssetName]: 1n, // DUST Auth Token (from main policy)
      }
    );

    logger.log('[DustTransactions]', '📎 Attaching DUST Auth Token Minting Policy script...');
    txBuilder.attach.MintingPolicy(mappingValidator);

    logger.log('[DustTransactions]', '📎 Attaching DUST Auth Token Policy script...');
    txBuilder.attach.MintingPolicy(mappingValidator);
    // Add signer
    txBuilder.addSigner(await lucid.wallet().address());

    logger.log('[DustTransactions]', '🔧 Completing registration transaction...');
    const completedTx = await txBuilder.complete();

    logger.log('[DustTransactions]', '✅ Registration transaction completed successfully');
    return completedTx;
  }

  /**
   * Create a transaction executor for DUST registration - Pure function
   * This returns a function that can be used with TransactionContext.executeTransaction()
   */
  static createRegistrationExecutor(lucid: LucidEvolution, dustPKH: string) {
    return async (onProgress?: (step: string, progress: number) => void): Promise<string> => {
      // Step 1: Build transaction
      onProgress?.('Preparing registration transaction...', 20);
      const completedTx = await DustTransactionsUtils.buildRegistrationTransaction(lucid, dustPKH);

      // Step 2: Sign and submit transaction
      onProgress?.('Signing registration transaction...', 40);
      logger.log('[DustTransactions]', '✍️ Signing registration transaction...');
      const signedTx = await completedTx.sign.withWallet().complete();

      onProgress?.('Submitting registration transaction...', 60);
      logger.log('[DustTransactions]', '📤 Submitting registration transaction...');
      const txHash = await signedTx.submit();

      logger.log('[DustTransactions]', '🎯 Registration transaction submitted successfully:', txHash);
      return txHash;
    };
  }

  /**
   * Build DUST unregistration transaction - Pure function
   * Based on Haskell buildDeregisterTx implementation
   * Consumes existing registration UTXO without creating a new one
   */
  static async buildUnregistrationTransaction(lucid: LucidEvolution, registrationUtxo: UTxO): Promise<TxSignBuilder> {
    const { Data, Constr } = await import('@lucid-evolution/lucid');

    logger.log('[DustTransactions]', '🔧 Building DUST Address Unregistration Transaction...');

    // Build the unregistration transaction
    logger.log('[DustTransactions]', '🔨 Building unregistration transaction...');
    const txBuilder = lucid.newTx();

    // REFERENCE INPUTS: Version Oracle UTxOs with required policies
    logger.log('[DustTransactions]', '📥 Adding reference inputs (Version Oracle UTxOs with policies)...');
    // CONSUME INPUT: Existing registration UTXO from DUST Mapping Validator
    // Redeemer for unregistration (empty constructor)
    const unregistrationRedeemer = Data.to(new Constr(0, [])); // Empty constructor for unregister

    logger.log(
      '[DustTransactions]',
      '🔥 Consuming registration UTXO:',
      toJson({
        txHash: registrationUtxo.txHash,
        outputIndex: registrationUtxo.outputIndex,
        address: registrationUtxo.address,
        redeemerCBORHEX: unregistrationRedeemer,
      })
    );

    txBuilder.collectFrom([registrationUtxo], unregistrationRedeemer);

    // BURN: DUST Auth Token Policy - burn the actual authentication token (-1)
    const dustTokenName = toHex(new TextEncoder().encode('DUST production auth token'));
    const dustAuthTokenAssetName = mappingValidatorPolicyId + dustTokenName;
    const dustAuthTokenBurnRedeemer = Data.to(new Constr(1, [])); // Constructor 1 for Burn

    logger.log(
      '[DustTransactions]',
      '🔥 Burning DUST Auth Token (Main Policy):',
      toJson({
        policyId: mappingValidatorPolicyId,
        tokenName: 'DUST production auth token',
        assetName: dustAuthTokenAssetName,
        amount: -1n, // Burning (negative amount)
        redeemer: 'Constructor 1 (Burn)',
        redeemerCBORHEX: dustAuthTokenBurnRedeemer,
      })
    );

    txBuilder.mintAssets({ [dustAuthTokenAssetName]: -1n }, dustAuthTokenBurnRedeemer);

    // Attach the required scripts
    logger.log('[DustTransactions]', '📎 Attaching required scripts...');
    txBuilder.attach.SpendingValidator(mappingValidator);
    txBuilder.attach.MintingPolicy(mappingValidator);

    // Add signer
    txBuilder.addSigner(await lucid.wallet().address());

    logger.log('[DustTransactions]', '🔧 Completing unregistration transaction...');
    const completedTx = await txBuilder.complete();

    logger.log('[DustTransactions]', '✅ Unregistration transaction completed successfully');
    return completedTx;
  }

  /**
   * Create a transaction executor for DUST unregistration - Pure function
   */
  static createUnregistrationExecutor(lucid: LucidEvolution, registrationUtxo: UTxO) {
    return async (onProgress?: (step: string, progress: number) => void): Promise<string> => {
      // Step 1: Build transaction
      onProgress?.('Preparing unregistration transaction...', 20);
      const completedTx = await DustTransactionsUtils.buildUnregistrationTransaction(lucid, registrationUtxo);

      // Step 2: Sign and submit transaction
      onProgress?.('Signing unregistration transaction...', 40);
      logger.log('[DustTransactions]', '✍️ Signing unregistration transaction...');
      const signedTx = await completedTx.sign.withWallet().complete();

      onProgress?.('Submitting unregistration transaction...', 60);
      logger.log('[DustTransactions]', '📤 Submitting unregistration transaction...');
      const txHash = await signedTx.submit();

      logger.log('[DustTransactions]', '🎯 Unregistration transaction submitted successfully:', txHash);
      return txHash;
    };
  }

  /**
   * Build DUST update transaction - Pure function
   * Based on Haskell buildUpdateTx implementation
   * Consumes existing registration UTXO and creates a new one with updated datum
   */
  static async buildUpdateTransaction(lucid: LucidEvolution, newDustPKH: string, registrationUtxo: UTxO): Promise<TxSignBuilder> {
    const { Data, Constr } = await import('@lucid-evolution/lucid');
    const mappingValidatorAddress = validatorToAddress(lucid.config().network!, mappingValidator);

    logger.log('[DustTransactions]', '🔧 Building DUST Address Update Transaction...');

    // Get current user's Cardano address and PKH
    const cardanoAddress = await lucid.wallet().address();
    const cardanoPKH = getAddressDetails(cardanoAddress)?.paymentCredential?.hash;

    if (!newDustPKH) {
      logger.error('[DustTransactions]', '❌ New DUST PKH not provided');
      throw new Error('New DUST PKH must be provided for update');
    }

    logger.log(
      '[DustTransactions]',
      '📋 Update Configuration:',
      toJson({
        newDustPKH,
        cardanoAddress,
      })
    );

    // Build the update transaction
    logger.log('[DustTransactions]', '🔨 Building update transaction...');
    const txBuilder = lucid.newTx();

    // CONSUME INPUT: Existing registration UTXO from DUST Mapping Validator
    // Redeemer for update (empty constructor)
    const updateRedeemer = Data.to(new Constr(0, [])); // Empty constructor for update

    logger.log(
      '[DustTransactions]',
      '🔄 Consuming existing registration UTXO:',
      toJson({
        txHash: registrationUtxo.txHash,
        outputIndex: registrationUtxo.outputIndex,
        redeemerCBORHEX: updateRedeemer,
      })
    );

    txBuilder.collectFrom([registrationUtxo], updateRedeemer);

    // MINT: DUST Mapping Validator Spend Policy - 1 token for Update action
    // This permits spending from the mapping validator
    const dustMappingValidatorSpendRedeemer = Data.to(new Constr(1, [])); // Constructor 1 for Update
    const dustMappingValidatorSpendAssetName = mappingValidatorPolicyId + '';

    logger.log(
      '[DustTransactions]',
      '🪙 Minting DUST Mapping Validator Spend Policy (Update):',
      toJson({
        policyId: mappingValidatorPolicyId,
        assetName: dustMappingValidatorSpendAssetName,
        amount: 1n,
        redeemer: 'Constructor 1 (Update)',
        redeemerCBORHEX: dustMappingValidatorSpendRedeemer,
      })
    );

    txBuilder.mintAssets({ [dustMappingValidatorSpendAssetName]: 1n }, dustMappingValidatorSpendRedeemer);

    // CREATE OUTPUT: New registration UTXO with updated datum
    // The new DUST PKH is encoded as the bytes of the hex string representation
    const updatedRegistrationDatumData = new Constr(0, [
      cardanoPKH!, // Cardano PKH (28 bytes hex string) - same as before
      toHex(new TextEncoder().encode(newDustPKH)), // New DUST PKH encoded as bytes of the string representation
    ]);

    const serializedUpdatedRegistrationDatum = Data.to(updatedRegistrationDatumData);

    // Get the DUST Auth Token from the existing UTXO to preserve it
    const dustTokenName = toHex(new TextEncoder().encode('DUST production auth token'));
    const dustAuthTokenAssetName = mappingValidatorPolicyId + dustTokenName;

    logger.log(
      '[DustTransactions]',
      '📤 Creating updated output to DUST Mapping Validator:',
      toJson({
        address: mappingValidatorAddress,
        assets: {
          lovelace: 1586080n, // Minimum ADA for UTxO
          [dustAuthTokenAssetName]: 1n, // DUST Auth Token (preserved)
        },
        datumData: updatedRegistrationDatumData,
        datumCBORHEX: serializedUpdatedRegistrationDatum,
      })
    );

    txBuilder.pay.ToContract(
      mappingValidatorAddress, // DUST Mapping Validator Address (same as before)
      { kind: 'inline', value: serializedUpdatedRegistrationDatum }, // Updated Registration Datum (INLINE)
      {
        lovelace: 1586080n, // Minimum ADA for UTxO
        [dustAuthTokenAssetName]: 1n, // DUST Auth Token (preserved from original UTXO)
      }
    );

    // Attach the required scripts
    logger.log('[DustTransactions]', '📎 Attaching required scripts...');
    txBuilder.attach.SpendingValidator(mappingValidator);
    txBuilder.attach.MintingPolicy(mappingValidator);

    // Add signer
    txBuilder.addSigner(await lucid.wallet().address());

    logger.log('[DustTransactions]', '🔧 Completing update transaction...');
    const completedTx = await txBuilder.complete();

    logger.log('[DustTransactions]', '✅ Update transaction completed successfully');
    return completedTx;
  }

  /**
   * Create a transaction executor for DUST update - Pure function
   */
  static createUpdateExecutor(lucid: LucidEvolution, newDustPKH: string, registrationUtxo: UTxO) {
    return async (onProgress?: (step: string, progress: number) => void): Promise<string> => {
      // Step 1: Build transaction
      onProgress?.('Preparing update transaction...', 20);
      const completedTx = await DustTransactionsUtils.buildUpdateTransaction(lucid, newDustPKH, registrationUtxo);

      // Step 2: Sign and submit transaction
      onProgress?.('Signing update transaction...', 40);
      logger.log('[DustTransactions]', '✍️ Signing update transaction...');
      const signedTx = await completedTx.sign.withWallet().complete();

      onProgress?.('Submitting update transaction...', 60);
      logger.log('[DustTransactions]', '📤 Submitting update transaction...');
      const txHash = await signedTx.submit();

      logger.log('[DustTransactions]', '🎯 Update transaction submitted successfully:', txHash);
      return txHash;
    };
  }
}

export default DustTransactionsUtils;

