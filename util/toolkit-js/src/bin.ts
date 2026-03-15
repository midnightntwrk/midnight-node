#!/usr/bin/env node

// Currently supported ledger versions.
const SUPPORTED_LEDGER_VERSIONS = [7, 8];

const ledgerVersionStr = process.env.LEDGER_VERSION ?? '8';
const ledgerVersion = parseInt(ledgerVersionStr, 10);

if (!SUPPORTED_LEDGER_VERSIONS.includes(ledgerVersion)) {
  console.error(`Unsupported LEDGER_VERSION: ${ledgerVersionStr} (expected one of ${SUPPORTED_LEDGER_VERSIONS.join(', ')})`);
  process.exit(1);
}

// Dynamically import the appropriate version of the toolkit based on the LEDGER_VERSION environment variable
// and run it.
import(`@midnight-ntwrk/node-toolkit-v${ledgerVersion}`)
  .then(({run}) => run())
  .catch((error) => {
    console.error('Unexpected error running toolkit:', error);
    process.exit(1);
  });
