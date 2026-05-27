/**
 * `@oz-policy-builder/wallet-adapter` — public entry point.
 *
 * Stream A (this commit set) exposes:
 *  - SEP-43 shared types (`./sep43`)
 *  - Freighter (browser) adapter (`./adapters/freighter`)
 *
 * Stream B (passkey-kit adapter) and Stream C (install/verify orchestration)
 * will append to this barrel in their own commits — do NOT pre-export their
 * modules here.
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
