import { Command } from 'commander';
import { logger } from './logger.js';
import { Kupmios, Lucid } from '@lucid-evolution/lucid';
import { assertNetwork, setWalletFromKeyFile } from './utils.js'
import DustTransactionsUtils from './transactions.js';

const program = new Command();

type Opts = {
  skey: string,
  kupo: string
  ogmios: string
  network: string
}

program
  .name('cnight-mapping-cli')
  .version('0.1.0')
  .description("A CLI for managing cNight mappings")
  .option('-k, --skey <skey-filename>', '.skey file of Cardano wallet')
  .option('-n, --network <network-id>', 'Cardano network id (use "Custom" for local-env)', 'Preview')
  .option('--kupo <kupo-addr>', 'Kupo endpoint', 'http://localhost:1442')
  .option('--ogmios <ogmios-addr>', 'Ogmios endpoint', 'http://localhost:1337')

program
  .command('register <dust-public-key>')
  .description('create a new registration UTXO for the given Dust public key')
  .action(async (dust_public_key: string) => {
    const opts: Opts = program.opts();
    console.log(`options: ${JSON.stringify(opts)}`)
    assertNetwork(opts.network);
    const lucid = await Lucid(new Kupmios(opts.kupo, opts.ogmios), opts.network);
    // await setWalletFromKeyFile(lucid, opts.skey);
    lucid.selectWallet.fromPrivateKey('ed25519e_sk1sz7phkd0edt8rwydm5eyr2j5yytg6yuuakz9azvwxjhju0l2t3djjk0lsq376exusamclxnu2lkp3qmajuyyrtp4n5k7xeaeyjl52jq8r4xyr');

    const utxos = await lucid.wallet().getUtxos();
    console.log(`utxos: ${utxos}`);

    let executor = DustTransactionsUtils.createRegistrationExecutor(lucid, dust_public_key)

    await executor((step, progress) => {
      logger.info(`${progress} %: ${step}`);
    })
  });


program.parse(process.argv);
const options = program.opts();

logger.warn('CLI options: ', options);
