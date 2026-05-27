/**
 * `@oz-policy-builder/wallet-adapter` — public entry point.
 *
 * Stream A exposes:
 *  - SEP-43 shared types (`./sep43`)
 *  - Freighter (browser) adapter (`./adapters/freighter`)
 *
 * Stream B exposes:
 *  - passkey-kit / headless-keypair adapter (`./adapters/passkey`)
 *
 * Stream C will append `installPolicy` + `verifyInstall` orchestration here
 * in its own commit set — do not pre-export those modules from this barrel.
 */

export {
  type SignAuthEntryParams,
  type SignAuthEntryResult,
  type SignTransactionParams,
  type SignTransactionResult,
  type WalletAdapter,
  WalletError,
  WalletErrorCode,
} from "./sep43.js";

export { FreighterWallet } from "./adapters/freighter.js";

export {
  PasskeyWallet,
  type PasskeyWalletOptions,
} from "./adapters/passkey.js";
