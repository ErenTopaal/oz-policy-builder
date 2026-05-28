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
 * Stream C exposes the high-level install orchestration:
 *  - `installPolicy` + `WalletInstallError` (`./install`)
 * (`verifyInstall` lands in the next commit.)
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

export {
  installPolicy,
  WalletInstallError,
  type InstallPolicyParams,
  type InstallPolicyResult,
  type WalletInstallErrorCode,
} from "./install.js";
