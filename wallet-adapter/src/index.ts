/** public entry point — sep43 types, adapters, install + verify orchestration. */

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

export {
  buildOzAuthEntry,
  computeAuthDigest,
  computeSignaturePayload,
  encodeAuthPayload,
  encodeContextRuleIdsScVal,
  encodeSignerScVal,
  makeOzSmartAccountAuthEncoder,
  type BuildOzAuthEntryParams,
  type OzAuthPayload,
  type OzSigner,
  type OzSignerWithKey,
} from "./oz_smart_account_auth.js";

export {
  verifyInstall,
  VerifyInstallError,
  type VerifyInstallParams,
  type VerifyInstallReport,
  type VerifyInstallDriftItem,
  type VerifyInstallErrorCode,
} from "./verify.js";
